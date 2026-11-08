use sha2::{Digest, Sha256};

/// Hashes a sequence of byte strings, length-prefixing each so that
///  different splits of the same bytes produce different hashes.
pub fn sha256_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}
