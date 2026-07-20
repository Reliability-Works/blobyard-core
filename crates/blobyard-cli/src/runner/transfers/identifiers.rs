use sha2::{Digest, Sha256};

pub(super) fn idempotency_digest(operation: &str, values: &[&str]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, operation);
    for value in values {
        hash_field(&mut hasher, value);
    }
    hasher.finalize().into()
}

fn hash_field(hasher: &mut Sha256, value: &str) {
    hasher.update(value.len().to_le_bytes());
    hasher.update(value.as_bytes());
}
