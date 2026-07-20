use super::super::{
    LoginPort, SystemLoginPort, activation_url, browser_command, instruction_text, launch_command,
    poll_delay,
};
use blobyard_core::SecretString;
use std::ffi::OsString;
use std::process::Command;
use std::time::Duration;

#[test]
fn activation_url_encodes_the_user_code_and_rejects_invalid_origins() {
    let url = activation_url("https://blobyard.com/cli/activate", "AB CD").ok();
    assert_eq!(
        url.as_ref().and_then(|value| value.query()),
        Some("user_code=AB+CD")
    );
    assert!(activation_url("not a URL", "ABCD-2345").is_err());
}

#[test]
fn login_instructions_and_poll_delay_are_bounded() {
    assert_eq!(
        instruction_text("https://blobyard.com/cli/activate", "ABCD-2345"),
        "Open https://blobyard.com/cli/activate and enter code ABCD-2345."
    );
    assert_eq!(poll_delay(5), Ok(Duration::from_secs(5)));
    assert!(poll_delay(0).is_err());
}

#[tokio::test]
async fn system_port_reports_waits_and_uses_an_explicit_browser_program() {
    let port = SystemLoginPort {
        browser_program: Some(OsString::from("true")),
    };
    let code = SecretString::new("ABCD-2345").expect("user code");
    port.present("https://blobyard.com/cli/activate", &code);
    assert!(port.open_browser("https://blobyard.com/cli/activate?user_code=ABCD-2345"));
    port.wait(Duration::ZERO).await;
    assert_eq!(format!("{port:?}"), "SystemLoginPort");
}

#[test]
fn platform_browser_command_and_launch_failure_are_explicit() {
    let command = browser_command("https://blobyard.com", None);
    #[cfg(target_os = "macos")]
    assert_eq!(command.get_program(), "open");
    #[cfg(target_os = "linux")]
    assert_eq!(command.get_program(), "xdg-open");
    #[cfg(target_os = "windows")]
    assert_eq!(command.get_program(), "rundll32");
    assert!(!launch_command(Command::new(
        "blobyard-missing-browser-command"
    )));
}
