use sha2::{Digest, Sha256};

pub fn sha256(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}
