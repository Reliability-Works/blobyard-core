use blobyard_core::SecretString;

#[allow(
    clippy::print_stderr,
    reason = "bootstrap authority is an explicit local operator output"
)]
#[doc(hidden)]
pub fn show_new_token(token: Option<SecretString>) {
    if let Some(token) = token {
        eprintln!(
            "Blob Yard bootstrap token, shown once: {}",
            token.expose_secret()
        );
    }
}
