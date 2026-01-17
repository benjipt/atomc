use sha2::{Digest, Sha256};

/// Return a stable sha256 hash for diff metadata.
pub fn diff_hash(diff: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(diff.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}
