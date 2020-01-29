use std::io;
use std::process;
use std::vec::Vec;

pub fn run(command: String, args: Vec<String>) -> Result<process::Output, io::Error> {
    process::Command::new(command)
        .args(args)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .spawn()
        .and_then(|child| child.wait_with_output())
}
