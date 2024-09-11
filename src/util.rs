use rand::RngCore;

pub fn random_bytes(n: u32) -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = vec![0u8; n as usize];
    rng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}