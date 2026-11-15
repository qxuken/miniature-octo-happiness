//! Bounded parsing of a qualification child's loopback readiness line.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    net::SocketAddr,
    path::Path,
};

pub(crate) fn ready_line(output: &Path) -> Result<Option<String>> {
    let file = match fs::File::open(output) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    BufReader::new(file)
        .take(512)
        .read_until(b'\n', &mut bytes)?;
    ensure!(bytes.len() < 512, "oversized server readiness");
    if !bytes.ends_with(b"\n") {
        return Ok(None);
    }
    Ok(Some(
        std::str::from_utf8(&bytes)?
            .trim_end_matches(['\r', '\n'])
            .to_owned(),
    ))
}

pub(crate) fn loopback_address(line: &str) -> Result<SocketAddr> {
    let address: SocketAddr = line
        .strip_prefix("ready ")
        .context("server readiness missing")?
        .parse()?;
    ensure!(
        address.ip().is_loopback() && address.port() != 0,
        "server advertised an invalid loopback address"
    );
    Ok(address)
}
