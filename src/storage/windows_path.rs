//! Windows UTF-16 path preparation shared by the Win32 adapter and host tests.
use crate::{AllocatorHandle, Error, Result};

const MAX_PATH_UNITS: usize = 32_767;
const BACKSLASH: u16 = b'\\' as u16;
const SLASH: u16 = b'/' as u16;
const COLON: u16 = b':' as u16;
const QUERY: u16 = b'?' as u16;
const DOT: u16 = b'.' as u16;
const VERBATIM: &[u16] = &[BACKSLASH, BACKSLASH, QUERY, BACKSLASH];
const NT_PREFIX: &[u16] = &[BACKSLASH, QUERY, QUERY, BACKSLASH];
const VERBATIM_UNC: &[u16] = &[
    BACKSLASH, BACKSLASH, QUERY, BACKSLASH, 85, 78, 67, BACKSLASH,
];
type WidePath = Vec<u16, AllocatorHandle>;

pub(super) fn validate_path(units: impl Iterator<Item = u16> + Clone) -> Result<()> {
    if units.clone().count() >= MAX_PATH_UNITS {
        return Err(Error::Limit);
    }
    if units.clone().any(|unit| unit == 0) || matches!(units.last(), Some(32 | 46 | 47 | 92) | None)
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

pub(super) fn validate_leaf(mut units: impl Iterator<Item = u16>) -> Result<()> {
    if units
        .any(|unit| u8::try_from(unit).is_ok_and(|byte| byte < 32 || b"\"*/:<>?|".contains(&byte)))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

pub(super) fn append_leaf(
    path: &mut WidePath,
    name: impl Iterator<Item = u16> + Clone,
) -> Result<()> {
    let parent = path.strip_suffix(&[0]).ok_or(Error::Io)?;
    let separator = !parent.ends_with(&[BACKSLASH]);
    let extra = name.clone().count() + usize::from(separator);
    if path.len() + extra > MAX_PATH_UNITS {
        return Err(Error::Limit);
    }
    path.try_reserve_exact(extra)
        .map_err(|_| Error::OutOfMemory)?;
    path.pop();
    if separator {
        path.push(BACKSLASH);
    }
    path.extend(name);
    path.push(0);
    Ok(())
}

// These path queries return the required size including NUL when short, and
// the written length excluding NUL on success. Reuse the input buffer once the
// parent handle has been opened.
pub(super) fn read_path(
    path: &mut WidePath,
    mut read: impl FnMut(&mut [u16]) -> u32,
    on_error: impl FnOnce() -> Error,
) -> Result<()> {
    let mut size = path.capacity().clamp(260, MAX_PATH_UNITS);
    loop {
        path.clear();
        path.try_reserve_exact(size)
            .map_err(|_| Error::OutOfMemory)?;
        path.resize(size, 0);
        let count = read(path) as usize;
        if count == 0 {
            return Err(on_error());
        }
        if count < size {
            path.truncate(count + 1);
            return Ok(());
        }
        size = count.max(size + 1);
        if size > MAX_PATH_UNITS {
            return Err(Error::Limit);
        }
    }
}

pub(super) fn parent_path(
    units: impl Iterator<Item = u16> + Clone,
    allocator: AllocatorHandle,
    mut normalize: impl FnMut(&[u16], &mut [u16]) -> u32,
    on_error: impl FnOnce() -> Error,
) -> Result<WidePath> {
    let mut input = Vec::new_in(allocator.clone());
    input
        .try_reserve_exact(units.clone().count() + 1)
        .map_err(|_| Error::OutOfMemory)?;
    input.extend(units);
    input.push(0);
    if input.starts_with(VERBATIM) || input.starts_with(NT_PREFIX) {
        return Ok(input);
    }
    // Keep the short absolute and drive-root fast path used by Windows std.
    if input.len() < 248
        && matches!(
            input.as_slice(),
            [_, COLON, 0] | [_, COLON, SLASH | BACKSLASH, ..]
        )
    {
        return Ok(input);
    }

    let mut path = Vec::new_in(allocator);
    read_path(&mut path, |out| normalize(&input, out), on_error)?;
    let (prefix, skip): (&[u16], usize) = match path.as_slice() {
        [_, COLON, BACKSLASH, ..] => (VERBATIM, 0),
        [BACKSLASH, BACKSLASH, DOT, BACKSLASH, ..] => (VERBATIM, 4),
        [BACKSLASH, BACKSLASH, QUERY, BACKSLASH, ..] | [BACKSLASH, QUERY, QUERY, BACKSLASH, ..] => {
            return Ok(path);
        }
        [BACKSLASH, BACKSLASH, ..] => (VERBATIM_UNC, 2),
        _ => return Ok(path),
    };
    let extra = prefix.len() - skip;
    let len = path.len();
    if len + extra > MAX_PATH_UNITS {
        return Err(Error::Limit);
    }
    path.try_reserve_exact(extra)
        .map_err(|_| Error::OutOfMemory)?;
    path.resize(len + extra, 0);
    path.copy_within(skip..len, prefix.len());
    path[..prefix.len()].copy_from_slice(prefix);
    Ok(path)
}

pub(super) fn canonical_drive(path: &[u16]) -> Result<u8> {
    if let Some([letter, COLON, BACKSLASH, ..]) = path.strip_prefix(VERBATIM)
        && let Ok(letter) = u8::try_from(*letter)
        && letter.is_ascii_alphabetic()
    {
        return Ok(letter.to_ascii_uppercase());
    }
    Err(Error::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::test_allocator::TestAllocator;
    use std::sync::Arc;

    fn wide(input: &str) -> Vec<u16> {
        input.encode_utf16().chain([0]).collect()
    }

    fn buffer(input: &[u16], allocator: AllocatorHandle) -> WidePath {
        let mut path = Vec::with_capacity_in(input.len(), allocator);
        path.extend_from_slice(input);
        path
    }

    #[test]
    fn wide_input_and_leaf_admission_keep_distinct_errors() {
        for input in ["", "file ", "file.", "file/", "file\\", "fi\0le"] {
            assert_eq!(validate_path(input.encode_utf16()), Err(Error::Invalid));
        }
        for len in [MAX_PATH_UNITS - 1, MAX_PATH_UNITS] {
            let input = "x".repeat(len);
            assert_eq!(
                validate_path(input.encode_utf16()),
                if len == MAX_PATH_UNITS {
                    Err(Error::Limit)
                } else {
                    Ok(())
                }
            );
        }
        assert_eq!(validate_path("C:\\λ.db".encode_utf16()), Ok(()));
        assert_eq!(validate_leaf("x λ.db".encode_utf16()), Ok(()));
        // Windows paths can carry raw UTF-16 that is not a Rust string.
        assert_eq!(validate_leaf([0xd800].into_iter()), Ok(()));
        for unit in b"\"*/:<>?|"
            .iter()
            .map(|&byte| u16::from(byte))
            .chain([1, 31])
        {
            assert_eq!(
                validate_leaf([b'x' as u16, unit, b'y' as u16].into_iter()),
                Err(Error::Invalid)
            );
        }
        assert_eq!(validate_leaf([32].into_iter()), Ok(()));
    }

    #[test]
    fn read_path_grows_for_short_reports_and_keeps_the_terminator() {
        let mut path = Vec::new_in(AllocatorHandle::default());
        let mut lengths = Vec::new();
        assert_eq!(
            read_path(
                &mut path,
                |out| {
                    lengths.push(out.len());
                    match lengths.len() {
                        1 => 260, // Equal report must make progress.
                        2 => 600,
                        3 => {
                            assert_eq!(out.len(), 600);
                            out.fill(u16::from(b'x'));
                            out[3] = 0;
                            3
                        }
                        _ => panic!("successful query retried"),
                    }
                },
                || panic!("positive result consulted the error callback")
            ),
            Ok(())
        );
        assert_eq!(lengths, [260, 261, 600]);
        assert_eq!(path.as_slice(), wide("xxx"));
    }

    #[test]
    fn read_path_keeps_native_error_and_rejects_over_cap_without_retry() {
        let mut path = Vec::new_in(AllocatorHandle::default());
        assert_eq!(
            read_path(&mut path, |_| 0, || Error::Busy),
            Err(Error::Busy)
        );
        let mut calls = 0;
        assert_eq!(
            read_path(
                &mut path,
                |_| {
                    calls += 1;
                    assert_eq!(calls, 1, "over-cap query retried");
                    (MAX_PATH_UNITS + 1) as u32
                },
                || panic!("positive length consulted the error callback")
            ),
            Err(Error::Limit)
        );
        assert_eq!(calls, 1);
        assert_eq!(path.capacity(), 260);

        let mut sizes = Vec::new();
        read_path(
            &mut path,
            |out| {
                sizes.push(out.len());
                if out.len() == MAX_PATH_UNITS {
                    out[..2].copy_from_slice(&[u16::from(b'x'), 0]);
                    1
                } else {
                    MAX_PATH_UNITS as u32
                }
            },
            || panic!("unexpected zero query"),
        )
        .unwrap();
        assert_eq!(sizes, [260, MAX_PATH_UNITS]);
        assert_eq!(path.as_slice(), wide("x"));
    }

    #[test]
    fn read_path_reclaims_selected_buffer_after_refused_growth() {
        let allocator = Arc::new(TestAllocator::default());
        let handle = allocator.handle();
        let live = allocator.live_bytes();
        for allowed in 0..=1 {
            let mut path = Vec::new_in(handle.clone());
            let mut calls = 0;
            assert_eq!(
                allocator.with_refusal(allowed, || read_path(
                    &mut path,
                    |_| {
                        calls += 1;
                        300
                    },
                    || panic!("allocation refusal has no native error"),
                )),
                Err(Error::OutOfMemory)
            );
            assert_eq!(calls, allowed);
            drop(path);
            assert_eq!(allocator.live_bytes(), live);
        }
    }

    #[test]
    fn parent_fast_paths_and_normalization_arms_preserve_windows_shapes() {
        for input in [
            r"C:",
            r"C:\short",
            "C:/short",
            r"\\?\C:\long",
            r"\??\C:\long",
        ] {
            let output = parent_path(
                input.encode_utf16(),
                AllocatorHandle::default(),
                |_, _| panic!("recognized parent went to normalization"),
                || panic!("no query"),
            )
            .unwrap();
            assert_eq!(output.as_slice(), wide(input));
        }
        for (resolved, expected) in [
            (r"C:\parent", r"\\?\C:\parent"),
            (r"\\.\C:\parent", r"\\?\C:\parent"),
            (r"\\?\C:\parent", r"\\?\C:\parent"),
            (r"\??\C:\parent", r"\??\C:\parent"),
            (r"\\server\share", r"\\?\UNC\server\share"),
            ("relative", "relative"),
        ] {
            let resolved = wide(resolved);
            let output = parent_path(
                "relative".encode_utf16(),
                AllocatorHandle::default(),
                |input, out| {
                    assert_eq!(input, wide("relative"));
                    out[..resolved.len()].copy_from_slice(&resolved);
                    (resolved.len() - 1) as u32
                },
                || panic!("successful normalization consults no native error"),
            )
            .unwrap();
            assert_eq!(output.as_slice(), wide(expected));
        }
        let mut normalized = false;
        let near = format!("C:\\{}", "x".repeat(244));
        parent_path(
            near.encode_utf16(),
            AllocatorHandle::default(),
            |_, out| {
                normalized = true;
                out[..4].copy_from_slice(&wide("C:\\"));
                3
            },
            || panic!("successful normalization"),
        )
        .unwrap();
        assert!(normalized, "248 retained units take the query path");
    }

    #[test]
    fn parent_reserves_each_selected_buffer_and_preserves_query_error() {
        let allocator = Arc::new(TestAllocator::default());
        let handle = allocator.handle();
        let live = allocator.live_bytes();
        let resolved = wide(&format!("C:\\{}", "x".repeat(256)));
        for allowed in 0..=2 {
            let mut calls = 0;
            let result = allocator.with_refusal(allowed, || {
                parent_path(
                    "relative".encode_utf16(),
                    handle.clone(),
                    |_, out| {
                        calls += 1;
                        assert_eq!(out.len(), 260);
                        out.copy_from_slice(&resolved);
                        259
                    },
                    || panic!("allocation refusal has no native error"),
                )
            });
            assert_eq!(result.err(), Some(Error::OutOfMemory));
            assert_eq!(calls, usize::from(allowed == 2));
            assert_eq!(allocator.live_bytes(), live);
        }
        assert_eq!(
            parent_path(
                "relative".encode_utf16(),
                handle,
                |_, _| 0,
                || Error::NotFound
            )
            .err(),
            Some(Error::NotFound)
        );
    }

    #[test]
    fn resolved_parent_prefix_and_appended_leaf_obey_the_final_cap() {
        for len in [MAX_PATH_UNITS - 4, MAX_PATH_UNITS - 3] {
            let resolved = wide(&format!("C:\\{}", "x".repeat(len - 4)));
            let result = parent_path(
                "relative".encode_utf16(),
                AllocatorHandle::default(),
                |_, out| {
                    if out.len() < resolved.len() {
                        return resolved.len() as u32;
                    }
                    out[..resolved.len()].copy_from_slice(&resolved);
                    (resolved.len() - 1) as u32
                },
                || panic!("query returned a positive length"),
            );
            if len == MAX_PATH_UNITS - 4 {
                assert_eq!(result.unwrap().len(), MAX_PATH_UNITS);
            } else {
                assert_eq!(result.err(), Some(Error::Limit));
            }
        }
        for parent in [r"\\?\C:\parent", r"\\?\C:\parent\"] {
            let original = wide(parent);
            let overhead = usize::from(!parent.ends_with('\\'));
            let mut path = buffer(&original, AllocatorHandle::default());
            let leaf = "x".repeat(MAX_PATH_UNITS - original.len() - overhead);
            append_leaf(&mut path, leaf.encode_utf16()).unwrap();
            assert_eq!(path.len(), MAX_PATH_UNITS);
            assert_eq!(path.last(), Some(&0));
            let mut path = buffer(&original, AllocatorHandle::default());
            assert_eq!(
                append_leaf(&mut path, format!("{leaf}x").encode_utf16()),
                Err(Error::Limit)
            );
            assert_eq!(path.as_slice(), original);
        }
    }

    #[test]
    fn leaf_assembly_preserves_the_parent_on_failure() {
        let allocator = Arc::new(TestAllocator::default());
        let original = wide(r"\\?\C:\parent");
        let mut path = buffer(&original, allocator.handle());
        let live = allocator.live_bytes();
        assert_eq!(
            allocator.with_refusal(0, || append_leaf(&mut path, "leaf".encode_utf16())),
            Err(Error::OutOfMemory)
        );
        assert_eq!(allocator.live_bytes(), live);
        assert_eq!(path.as_slice(), original);
        append_leaf(&mut path, "leaf".encode_utf16()).unwrap();
        assert_eq!(path.as_slice(), wide(r"\\?\C:\parent\leaf"));
        let mut missing = buffer(&[u16::from(b'x')], allocator.handle());
        assert_eq!(
            append_leaf(&mut missing, "leaf".encode_utf16()),
            Err(Error::Io)
        );
    }

    #[test]
    fn canonical_parent_requires_a_verbatim_alphabetic_drive() {
        assert_eq!(canonical_drive(&wide(r"\\?\c:\parent")), Ok(b'C'));
        assert_eq!(canonical_drive(&wide(r"\\?\Z:\")), Ok(b'Z'));
        for input in [
            r"C:\parent",
            r"\\?\C:/parent",
            r"\\?\λ:\parent",
            r"\\?\7:\parent",
            r"\\?\UNC\host\share",
        ] {
            assert_eq!(
                canonical_drive(&wide(input)),
                Err(Error::UnsupportedPlatform)
            );
        }
    }
}
