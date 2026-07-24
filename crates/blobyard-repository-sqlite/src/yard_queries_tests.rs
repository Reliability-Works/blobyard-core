use super::required_session_hash;
use blobyard_contract::RepositoryError;

#[test]
fn private_yard_requires_a_session_hash() {
    assert_eq!(required_session_hash(None), Err(RepositoryError::NotFound));
    assert_eq!(required_session_hash(Some("hash")), Ok("hash"));
}
