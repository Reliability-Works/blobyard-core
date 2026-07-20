//! Formatting failure propagation tests for core display implementations.

#![allow(
    clippy::expect_used,
    reason = "test fixtures must parse canonical URIs"
)]

use blobyard_core::{BlobyardError, BlobyardUri, ErrorCode};
use std::fmt::{self, Display, Write};

struct FailingWriter {
    writes_remaining: usize,
}

impl Write for FailingWriter {
    fn write_str(&mut self, _value: &str) -> fmt::Result {
        if self.writes_remaining == 0 {
            return Err(fmt::Error);
        }
        self.writes_remaining -= 1;
        Ok(())
    }
}

fn format_with_limit(value: &impl Display, writes_remaining: usize) -> fmt::Result {
    let mut writer = FailingWriter { writes_remaining };
    write!(&mut writer, "{value}")
}

fn assert_propagates_writer_failures(value: &impl Display) {
    let failures = (0..32)
        .filter(|limit| format_with_limit(value, *limit).is_err())
        .count();

    assert!(failures > 1);
    assert!(format_with_limit(value, 32).is_ok());
}

#[test]
fn blobyard_error_propagates_formatter_failures() {
    let error = BlobyardError::new(ErrorCode::AuthRequired, "Sign in first.")
        .with_request_id("req_example");

    assert_propagates_writer_failures(&error);
}

#[test]
fn blobyard_uri_propagates_formatter_failures() {
    let uri = "blobyard://team/mobile/builds/My%20App.ipa?version=2"
        .parse::<BlobyardUri>()
        .expect("canonical URI fixture must parse");

    assert_propagates_writer_failures(&uri);
}
