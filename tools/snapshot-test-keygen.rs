// Windows fixture provisioning in a directory protected by the caller.
pub fn keygen(directory: &std::path::Path) {
    use std::io::Write;
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(["weekend-replicate".to_owned()]).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("key.der"))
        .unwrap()
        .write_all(&signing_key.serialize_der())
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("cert.der"))
        .unwrap()
        .write_all(cert.der().as_ref())
        .unwrap();
}
