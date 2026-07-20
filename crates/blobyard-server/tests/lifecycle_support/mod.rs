fn run(arguments: &[&str]) -> std::process::Output {
    server_command()
        .args(arguments)
        .output()
        .expect("server command")
}

fn run_with_stdin(arguments: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = server_command()
        .args(arguments)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("server command");
    std::io::Write::write_all(&mut child.stdin.take().expect("server stdin"), input)
        .expect("write server stdin");
    child.wait_with_output().expect("server output")
}

fn server_command() -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_blobyard-server"));
    command
        .env_remove("BLOBYARD_S3_ACCESS_KEY_ID")
        .env_remove("BLOBYARD_S3_SECRET_ACCESS_KEY")
        .env_remove("BLOBYARD_S3_SESSION_TOKEN");
    command
}
