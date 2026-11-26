pub fn sign(bytes: &mut [u8]) {
    let (payload, footer) = bytes.split_at_mut(bytes.len() - 8);
    let mut crc = 0u64;
    for byte in payload {
        crc ^= u64::from(*byte) << 56;
        for _ in 0..8 {
            crc = (crc << 1) ^ (0x42f0_e1eb_a9ea_3693 & 0u64.wrapping_sub(crc >> 63));
        }
    }
    footer.copy_from_slice(&crc.to_le_bytes());
}
