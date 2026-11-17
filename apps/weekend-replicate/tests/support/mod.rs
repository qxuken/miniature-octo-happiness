use std::{fs, io::Write, path::Path};

pub fn credentials(path: &Path) {
    fs::create_dir(path).unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(["weekend-replicate".into()]).unwrap();
    fs::write(path.join("cert.der"), cert.der()).unwrap();
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path.join("key.der"))
        .unwrap()
        .write_all(&signing_key.serialize_der())
        .unwrap();
}
