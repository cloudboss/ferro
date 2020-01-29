use std::process;
use std::vec::Vec;

pub trait When {
    fn when(&self) -> Result<bool, crate::ferro::Error>;
}

#[derive(Debug)]
pub struct Always;

impl When for Always {
    fn when(&self) -> Result<bool, crate::ferro::Error> {
        Ok(true)
    }
}

#[derive(Debug)]
pub struct Never;

impl When for Never {
    fn when(&self) -> Result<bool, crate::ferro::Error> {
        Ok(false)
    }
}

#[derive(Debug)]
pub struct WhenExecute {
    pub command: String,
    pub args: Vec<String>,
}

impl When for WhenExecute {
    fn when(&self) -> Result<bool, crate::ferro::Error> {
        process::Command::new(self.command.clone())
            .args(self.args.clone())
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .spawn()
            .and_then(|child| {
                child.wait_with_output().and_then(|output| {
                    if output.status.success() {
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                })
            })
            .map_err(|e| crate::ferro::Error {
                changed: false,
                description: e.to_string(),
            })
    }
}

pub fn when_execute(execute: &str) -> WhenExecute {
    let mut parts = execute.split_whitespace();
    let command = parts.next().unwrap_or("").to_owned();
    let args = parts.map(|s| s.to_owned()).collect();
    WhenExecute {
        command: command,
        args: args,
    }
}
