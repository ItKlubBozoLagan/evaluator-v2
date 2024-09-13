use rand::RngCore;
use std::fmt::Write;

pub fn random_bytes(n: u32) -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = vec![0u8; n as usize];
    rng.fill_bytes(&mut bytes);
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}
