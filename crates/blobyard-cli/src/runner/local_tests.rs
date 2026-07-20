#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{map_encoding, map_write};
use blobyard_core::ErrorCode;

#[test]
fn local_result_mappers_preserve_success_and_hide_provider_errors() {
    assert_eq!(map_encoding::<_, ()>(Ok("text")), Ok("text"));
    assert_eq!(
        map_encoding::<(), _>(Err("provider detail"))
            .expect_err("encoding failure")
            .code(),
        ErrorCode::InternalError
    );
    assert_eq!(map_write::<_, ()>(Ok(7)), Ok(7));
    assert_eq!(
        map_write::<(), _>(Err("provider detail"))
            .expect_err("write failure")
            .code(),
        ErrorCode::InternalError
    );
}
