//! Own a small fixed VHD for exercising actual NTFS exhaustion.
use crate::{support::path, windows_trace::call};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HANDLE, INVALID_HANDLE_VALUE},
    Storage::{
        FileSystem::{
            CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW, GetDiskFreeSpaceW,
            GetLogicalDrives, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, OPEN_EXISTING,
        },
        Vhd::{
            ATTACH_VIRTUAL_DISK_FLAG_NO_DRIVE_LETTER, AttachVirtualDisk,
            CREATE_VIRTUAL_DISK_FLAG_FULL_PHYSICAL_ALLOCATION, CREATE_VIRTUAL_DISK_PARAMETERS,
            CREATE_VIRTUAL_DISK_PARAMETERS_0, CREATE_VIRTUAL_DISK_PARAMETERS_0_0,
            CREATE_VIRTUAL_DISK_VERSION_1, CreateVirtualDisk, DETACH_VIRTUAL_DISK_FLAG_NONE,
            DetachVirtualDisk, GetVirtualDiskPhysicalPath, VIRTUAL_DISK_ACCESS_ALL,
            VIRTUAL_STORAGE_TYPE, VIRTUAL_STORAGE_TYPE_DEVICE_VHD,
            VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT,
        },
    },
    System::{
        IO::DeviceIoControl, Ioctl::VOLUME_DISK_EXTENTS, SystemInformation::GetSystemDirectoryW,
    },
};

pub const CLUSTER: u64 = 4096;

#[derive(Clone, Copy)]
pub enum Capacity {
    Append,
    Maintenance,
}

impl Capacity {
    const fn bytes(self) -> u64 {
        match self {
            Self::Append => 64 * 1024 * 1024,
            Self::Maintenance => 256 * 1024 * 1024,
        }
    }
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain([0]).collect()
}

fn status(code: u32, context: &str) -> Result<()> {
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32)).context(context.to_owned());
    }
    Ok(())
}

pub fn space(directory: &Path, capacity: Capacity) -> Result<u64> {
    let bytes = capacity.bytes();
    let mut available = 0;
    let mut total = 0;
    let mut free = 0;
    // SAFETY: bounded, terminated path and three writable native outputs.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide(directory.as_os_str()).as_ptr(),
            &mut available,
            &mut total,
            &mut free,
        )
    };
    ensure!(ok != 0, "volume free space: {}", io::Error::last_os_error());
    ensure!(
        available == free && free <= total && (bytes / 2..=bytes).contains(&total),
        "bounded private volume without a per-user quota required"
    );
    Ok(free)
}

fn drives() -> Result<u32> {
    // SAFETY: this API takes no pointers or handles.
    let found = unsafe { GetLogicalDrives() };
    ensure!(found != 0, "drive map: {}", io::Error::last_os_error());
    Ok(found)
}

fn disk_number(name: &str) -> Result<u32> {
    let digits = name
        .strip_prefix(r"\\.\PhysicalDrive")
        .context("physical disk path")?;
    ensure!(
        !digits.is_empty() && digits.bytes().all(|ch| ch.is_ascii_digit()),
        "physical disk number"
    );
    let number = digits.parse()?;
    ensure!(number > 0, "refusing disk zero");
    Ok(number)
}

fn physical_disk(handle: HANDLE) -> Result<u32> {
    let mut name = [0; 128];
    let mut bytes = std::mem::size_of_val(&name) as u32;
    // SAFETY: the handle is live and both outputs match their specified sizes.
    status(
        unsafe { GetVirtualDiskPhysicalPath(handle, &mut bytes, name.as_mut_ptr()) },
        "locate attached VHD",
    )?;
    let length = name
        .iter()
        .position(|ch| *ch == 0)
        .context("physical path terminator")?;
    disk_number(&String::from_utf16(&name[..length])?)
}

fn create(image: &Path, bytes: u64) -> Result<OwnedHandle> {
    let store = VIRTUAL_STORAGE_TYPE {
        DeviceId: VIRTUAL_STORAGE_TYPE_DEVICE_VHD,
        VendorId: VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT,
    };
    let parameters = CREATE_VIRTUAL_DISK_PARAMETERS {
        Version: CREATE_VIRTUAL_DISK_VERSION_1,
        Anonymous: CREATE_VIRTUAL_DISK_PARAMETERS_0 {
            Version1: CREATE_VIRTUAL_DISK_PARAMETERS_0_0 {
                MaximumSize: bytes,
                SectorSizeInBytes: 512,
                ..Default::default()
            },
        },
    };
    let mut handle = null_mut();
    // SAFETY: parameters match version 1, no source or parent, and all pointed
    // data stays live during synchronous creation at the new private path.
    status(
        unsafe {
            CreateVirtualDisk(
                &store,
                wide(image.as_os_str()).as_ptr(),
                VIRTUAL_DISK_ACCESS_ALL,
                null_mut(),
                CREATE_VIRTUAL_DISK_FLAG_FULL_PHYSICAL_ALLOCATION,
                0,
                &parameters,
                null(),
                &mut handle,
            )
        },
        "create private fixed VHD",
    )?;
    // SAFETY: successful creation transferred ownership of the new VHD handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

fn diskpart() -> Result<PathBuf> {
    let mut directory = [0; 32768];
    // SAFETY: an initialized UTF-16 buffer of the supplied length.
    let length =
        unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
    ensure!(
        length > 0 && length < directory.len(),
        "resolve Windows system directory"
    );
    Ok(PathBuf::from(OsString::from_wide(&directory[..length])).join("diskpart.exe"))
}

fn verify_extent(letter: char, expected: u32, bytes: u64) -> Result<()> {
    let device = wide(OsStr::new(&format!(r"\\.\{letter}:")));
    // SAFETY: open the drive-letter device for metadata only, with no sharing
    // restrictions on other readers or writers.
    let handle = unsafe {
        CreateFileW(
            device.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    ensure!(
        handle != INVALID_HANDLE_VALUE,
        "open volume device: {}",
        io::Error::last_os_error()
    );
    // SAFETY: the successful open returned a uniquely owned handle.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut extents = VOLUME_DISK_EXTENTS::default();
    let mut written = 0;
    // SAFETY: synchronous metadata query into storage for one extent.
    let ok = unsafe {
        DeviceIoControl(
            handle.as_raw_handle(),
            IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
            null(),
            0,
            (&mut extents as *mut VOLUME_DISK_EXTENTS).cast(),
            std::mem::size_of_val(&extents) as u32,
            &mut written,
            null_mut(),
        )
    };
    ensure!(
        ok != 0 && written as usize >= std::mem::size_of_val(&extents),
        "volume extents: {}",
        io::Error::last_os_error()
    );
    let extent = extents.Extents[0];
    ensure!(
        extents.NumberOfDiskExtents == 1
            && extent.DiskNumber == expected
            && extent.StartingOffset > 0
            && extent.ExtentLength > 0
            && extent
                .StartingOffset
                .checked_add(extent.ExtentLength)
                .is_some_and(|end| end <= bytes as i64),
        "volume does not resolve to the bounded owned disk"
    );
    Ok(())
}

pub fn with_volume(
    proof: &mut Value,
    output: &Path,
    env: &BTreeMap<OsString, OsString>,
    capacity: Capacity,
    test: impl FnOnce(&mut Value, &Path) -> Result<()>,
) -> Result<()> {
    let bytes = capacity.bytes();
    let scratch = output.join("volume");
    fs::create_dir(&scratch)?;
    let image = scratch.join("owned.vhd");
    let before = drives()?;
    let letter = ('R'..='Z')
        .rev()
        .find(|letter| before & bit(*letter) == 0)
        .context("free test drive letter")?;
    let root = PathBuf::from(format!("{letter}:\\"));
    let mut handle = None;
    let mut attached = false;
    let result = (|| {
        handle = Some(create(&image, bytes)?);
        let handle = handle.as_ref().unwrap().as_raw_handle();
        // SAFETY: valid owned disk, synchronous call, drive letter assignment
        // disabled. Closing this handle is a fallback detach on process exit.
        status(
            unsafe {
                AttachVirtualDisk(
                    handle,
                    null_mut(),
                    ATTACH_VIRTUAL_DISK_FLAG_NO_DRIVE_LETTER,
                    0,
                    null(),
                    null(),
                )
            },
            "attach private VHD",
        )?;
        attached = true;
        let disk = physical_disk(handle)?;
        ensure!(
            drives()? & bit(letter) == 0,
            "test drive letter was claimed"
        );
        let script = scratch.join("format.txt");
        use std::io::Write;
        write!(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&script)?,
            "select disk {disk}\r\ncreate partition primary\r\nformat fs=ntfs unit=4096 quick label=WDBFULL\r\nassign letter={letter}\r\n"
        )?;
        call(
            proof,
            output,
            vec![path(&diskpart()?), "/s".into(), path(&script)],
            &scratch,
            "format",
            env,
            false,
        )?;
        ensure!(
            physical_disk(handle)? == disk,
            "attached physical disk changed"
        );
        verify_extent(letter, disk, bytes)?;
        let mut volume = crate::windows_process::volume(&root)?;
        let mut sectors = 0;
        let mut sector_bytes = 0;
        // SAFETY: initialized writable outputs and a terminated root path.
        let geometry = unsafe {
            GetDiskFreeSpaceW(
                wide(root.as_os_str()).as_ptr(),
                &mut sectors,
                &mut sector_bytes,
                null_mut(),
                null_mut(),
            )
        };
        ensure!(
            geometry != 0 && u64::from(sectors) * u64::from(sector_bytes) == CLUSTER,
            "NTFS allocation unit must be 4096 bytes"
        );
        volume["physical_disk"] = json!(disk);
        volume["virtual_bytes"] = json!(bytes);
        volume["allocation_unit_bytes"] = json!(CLUSTER);
        volume["free_before"] = json!(space(&root, capacity)?);
        ensure!(
            volume["free_before"]
                .as_u64()
                .is_some_and(|free| free > bytes / 2),
            "unusable test volume capacity"
        );
        proof["private_volume"] = volume;
        let data = root.join("weekend-test");
        fs::create_dir(&data)?;
        test(proof, &data)
    })();

    let detach = if attached {
        // SAFETY: detach only the attachment owned by the live VHD handle.
        status(
            unsafe {
                DetachVirtualDisk(
                    handle.as_ref().unwrap().as_raw_handle(),
                    DETACH_VIRTUAL_DISK_FLAG_NONE,
                    0,
                )
            },
            "detach private VHD",
        )
    } else {
        Ok(())
    };
    drop(handle);
    let remove = match fs::remove_file(&image) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    };
    let verify = (|| -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while drives()? & bit(letter) != 0 {
            ensure!(Instant::now() < deadline, "test drive letter still mounted");
            std::thread::sleep(Duration::from_millis(25));
        }
        ensure!(!image.try_exists()?, "VHD image still present");
        Ok(())
    })();
    proof["volume_cleanup"] = json!({"verified":detach.is_ok() && remove.is_ok() && verify.is_ok(),
        "detach_error":detach.as_ref().err().map(ToString::to_string),
        "remove_error":remove.as_ref().err().map(ToString::to_string),
        "verify_error":verify.as_ref().err().map(ToString::to_string)});
    result?;
    detach?;
    remove?;
    verify
}

fn bit(letter: char) -> u32 {
    1 << (letter as u32 - 'A' as u32)
}

pub const fn full(free: u64) -> bool {
    free < CLUSTER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_disk_selection_cannot_fall_back_or_supply_other_commands() {
        assert_eq!(disk_number(r"\\.\PhysicalDrive42").unwrap(), 42);
        for name in [
            r"\\.\PhysicalDrive0",
            r"\\.\PhysicalDrive",
            "42",
            "\\\\.\\PhysicalDrive2\r\nclean",
        ] {
            assert!(disk_number(name).is_err());
        }
        assert!(full(0));
        assert!(!full(CLUSTER));
    }
}
