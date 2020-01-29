use std::default::Default;
use std::error;
use std::fmt;
use std::process;
use std::string;
use std::vec::Vec;

use serde::Serialize;

const COMMAND: &str = "command";

#[derive(Debug)]
pub enum Error {
    InvalidCommandError,
    CommandError,
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

impl From<Error> for crate::ferro::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::InvalidCommandError => crate::ferro::Error {
                changed: false,
                description: "invalid command".to_owned(),
            },
            Error::CommandError => crate::ferro::Error {
                changed: false,
                description: "command error".to_owned(),
            },
        }
    }
}

impl From<string::FromUtf8Error> for crate::ferro::Error {
    fn from(e: string::FromUtf8Error) -> Self {
        crate::ferro::Error {
            changed: true,
            description: e.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Debug, Serialize)]
pub struct Output {
    exit_status: i32,
    stdout: String,
    stderr: String,
    stdout_lines: Vec<String>,
    stderr_lines: Vec<String>,
}

impl Default for Output {
    fn default() -> Self {
        Output {
            exit_status: 0,
            stdout: "".to_owned(),
            stderr: "".to_owned(),
            stdout_lines: vec![],
            stderr_lines: vec![],
        }
    }
}

#[typetag::serialize]
impl crate::ferro::Output for Output {
    fn to_value(&self) -> Result<serde_json::value::Value, serde_json::error::Error> {
        serde_json::to_value(self)
    }
}

pub struct Command {
    pub command: Box<crate::lazy::String>,
    pub args: Box<crate::lazy::Vec<Box<crate::lazy::String>>>,
    pub creates: Box<crate::lazy::String>,
    pub removes: Box<crate::lazy::String>,
}

impl Default for Command {
    fn default() -> Self {
        Command {
            command: Box::new(|_| "".to_owned()),
            args: Box::new(|_| vec![]),
            creates: Box::new(|_| "".to_owned()),
            removes: Box::new(|_| "".to_owned()),
        }
    }
}

impl crate::ferro::Module for Command {
    fn name(&self) -> String {
        COMMAND.to_owned()
    }

    fn apply(
        &self,
        context: &crate::ferro::Context,
    ) -> Result<crate::ferro::Response, crate::ferro::Error> {
        let args: Vec<String> = (self.args)(context)
            .into_iter()
            .map(|f| f(context))
            .collect();
        let result = process::Command::new((self.command)(context))
            .args(args)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .spawn()
            .and_then(|child| child.wait_with_output());

        match result {
            Ok(out) => {
                let stdout = String::from_utf8(out.stdout)?;
                let stderr = String::from_utf8(out.stderr)?;
                let output = Output {
                    exit_status: out.status.code().unwrap_or(-1),
                    stdout: stdout.clone(),
                    stderr: stderr.clone(),
                    stdout_lines: stdout.lines().map(|l| l.to_owned()).collect(),
                    stderr_lines: stderr.lines().map(|l| l.to_owned()).collect(),
                };
                if out.status.success() {
                    crate::ferro::result_response(true, Some(Box::new(output)))
                } else {
                    crate::ferro::result_error(true, stderr)
                }
            }
            Err(e) => crate::ferro::result_error(true, e.to_string()),
        }
    }

    fn destroy(&self) -> Result<crate::ferro::Response, crate::ferro::Error> {
        crate::ferro::result_response(false, None)
    }
}
