use std::collections::HashMap;
use std::convert::From;
use std::default::Default;
use std::error;
use std::fmt;
use std::str::FromStr;
use std::thread::sleep;
use std::time::Duration;

use rusoto_cloudformation::{
    CloudFormation as CF, CloudFormationClient, CreateStackError, CreateStackInput,
    DescribeStacksError, DescribeStacksInput, Output as CFOutput, Stack, UpdateStackError,
    UpdateStackInput,
};
use rusoto_core::RusotoError;
use rusoto_credential::ProfileProvider;
use rusoto_signature::region::Region;
use serde::{Deserialize, Serialize};

const CLOUDFORMATION: &str = "cloudformation";

const AWS_DEFAULT_REGION: &str = "AWS_DEFAULT_REGION";
const AWS_REGION: &str = "AWS_REGION";

const CAPABILITY_IAM: &str = "CAPABILITY_IAM";
const CAPABILITY_NAMED_IAM: &str = "CAPABILITY_NAMED_IAM";
const CAPABILITY_AUTO_EXPAND: &str = "CAPABILITY_AUTO_EXPAND";

const CREATE_COMPLETE: &str = "CREATE_COMPLETE";
const CREATE_FAILED: &str = "CREATE_FAILED";
const DELETE_COMPLETE: &str = "DELETE_COMPLETE";
const DELETE_FAILED: &str = "DELETE_FAILED";
const ROLLBACK_FAILED: &str = "ROLLBACK_FAILED";
const ROLLBACK_COMPLETE: &str = "ROLLBACK_COMPLETE";

const UPDATE_COMPLETE: &str = "UPDATE_COMPLETE";
const UPDATE_FAILED: &str = "UPDATE_FAILED";
const UPDATE_ROLLBACK_FAILED: &str = "UPDATE_ROLLBACK_FAILED";
const UPDATE_ROLLBACK_COMPLETE: &str = "UPDATE_ROLLBACK_COMPLETE";

const SLEEP_SECS: u64 = 5;

#[derive(Debug, Serialize, Deserialize)]
pub enum Error {
    CloudFormationError(String),
    StackNotFoundError,
    RegionNotFoundError,
    NoUpdateError,
    UnknownError,
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

impl From<RusotoError<DescribeStacksError>> for Error {
    fn from(e: RusotoError<DescribeStacksError>) -> Self {
        let stack_with_id = "Stack with id";
        let does_not_exist = "does not exist";
        let e_string = e.to_string();
        if e_string.contains(&stack_with_id) && e_string.contains(&does_not_exist) {
            Error::StackNotFoundError
        } else {
            Error::CloudFormationError(e.to_string())
        }
    }
}

impl From<RusotoError<CreateStackError>> for Error {
    fn from(e: RusotoError<CreateStackError>) -> Self {
        Error::CloudFormationError(e.to_string())
    }
}

impl From<RusotoError<UpdateStackError>> for Error {
    fn from(e: RusotoError<UpdateStackError>) -> Self {
        let no_updates = "No updates are to be performed";
        if e.to_string().contains(no_updates) {
            Error::NoUpdateError
        } else {
            Error::CloudFormationError(e.to_string())
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

pub enum Template {
    TemplateBody(String),
    TemplateURL(String),
}

#[derive(Debug, Serialize)]
pub struct Output {
    outputs: HashMap<String, String>,
}

#[typetag::serialize]
impl crate::ferro::Output for Output {
    fn to_value(&self) -> Result<serde_json::value::Value, serde_json::error::Error> {
        serde_json::to_value(self)
    }
}

pub struct CloudFormation {
    pub stack_name: Box<crate::lazy::String>,
    pub template: Box<dyn Fn(&crate::ferro::Context) -> Template>,
    cfn: CloudFormationClient,
}

impl CloudFormation {
    pub fn new(
        stack_name: Box<crate::lazy::String>,
        template: Box<dyn Fn() -> Template>,
    ) -> Result<Self, Error> {
        get_region().and_then(|region| {
            let cfn = CloudFormationClient::new(region);
            Ok(CloudFormation {
                stack_name: stack_name,
                template: template(),
                cfn: cfn,
            })
        })
    }

    fn get_stack_info(&self, stack_name: &String) -> Result<Stack, Error> {
        let describe_stacks = self.cfn.describe_stacks(DescribeStacksInput {
            next_token: None,
            stack_name: Some(stack_name.to_owned()),
        });

        let result = describe_stacks.sync()?;

        match result.stacks {
            Some(stacks) => {
                for stack in stacks {
                    return Ok(stack);
                }
                // Should not reach here.
                Err(Error::UnknownError)
            }
            // Probably won't reach here either, as AWS returns an error
            // when the stack is not found, which is handled above.
            None => Err(Error::UnknownError),
        }
    }

    fn wait_for_stack_create(&self, stack_name: &String) -> Result<(), Error> {
        let states = vec![
            CREATE_FAILED.to_owned(),
            DELETE_COMPLETE.to_owned(),
            DELETE_FAILED.to_owned(),
            ROLLBACK_FAILED.to_owned(),
            ROLLBACK_COMPLETE.to_owned(),
        ];
        self.wait_for_stack(states, CREATE_COMPLETE.to_owned(), stack_name)
    }

    fn wait_for_stack_update(&self, stack_name: &String) -> Result<(), Error> {
        let states = vec![
            UPDATE_FAILED.to_owned(),
            UPDATE_ROLLBACK_FAILED.to_owned(),
            UPDATE_ROLLBACK_COMPLETE.to_owned(),
        ];
        self.wait_for_stack(states, UPDATE_COMPLETE.to_owned(), stack_name)
    }

    fn wait_for_stack(
        &self,
        states: Vec<String>,
        desired_state: String,
        stack_name: &String,
    ) -> Result<(), Error> {
        loop {
            let stack = self.get_stack_info(stack_name)?;
            if stack.stack_status == desired_state {
                return Ok(());
            } else if states.contains(&stack.stack_status) {
                return Err(Error::CloudFormationError(stack.stack_status.to_owned()));
            } else {
                sleep(Duration::from_secs(SLEEP_SECS));
            }
        }
    }

    fn create_stack(
        &self,
        stack_name: &String,
        template: &Template,
    ) -> Result<Option<Output>, Error> {
        let mut create_stack_input = CreateStackInput {
            stack_name: stack_name.to_owned(),
            capabilities: Some(vec![
                CAPABILITY_IAM.to_owned(),
                CAPABILITY_NAMED_IAM.to_owned(),
                CAPABILITY_AUTO_EXPAND.to_owned(),
            ]),
            ..Default::default()
        };
        match template {
            Template::TemplateBody(body) => {
                create_stack_input.template_body = Some(body.to_owned())
            }
            Template::TemplateURL(url) => create_stack_input.template_url = Some(url.to_owned()),
        };

        self.cfn.create_stack(create_stack_input).sync()?;

        self.wait_for_stack_create(stack_name)
            .and_then(|_| self.get_stack_info(stack_name))
            .and_then(|stack| {
                stack.outputs.map_or(Ok(None), |outputs| {
                    Ok(Some(Output {
                        outputs: outputs_to_map(outputs),
                    }))
                })
            })
            .map_err(|e| Error::CloudFormationError(e.to_string()))
    }

    fn update_stack(
        &self,
        stack_name: &String,
        template: &Template,
    ) -> Result<Option<Output>, Error> {
        let mut update_stack_input = UpdateStackInput {
            stack_name: stack_name.to_owned(),
            capabilities: Some(vec![
                CAPABILITY_IAM.to_owned(),
                CAPABILITY_NAMED_IAM.to_owned(),
                CAPABILITY_AUTO_EXPAND.to_owned(),
            ]),
            ..Default::default()
        };
        match template {
            Template::TemplateBody(body) => {
                update_stack_input.template_body = Some(body.to_owned())
            }
            Template::TemplateURL(url) => update_stack_input.template_url = Some(url.to_owned()),
        };

        self.cfn.update_stack(update_stack_input).sync()?;

        self.wait_for_stack_update(stack_name)
            .and_then(|_| self.get_stack_info(stack_name))
            .and_then(|stack| {
                stack.outputs.map_or(Ok(None), |outputs| {
                    Ok(Some(Output {
                        outputs: outputs_to_map(outputs),
                    }))
                })
            })
            .map_err(|e| Error::CloudFormationError(e.to_string()))
    }
}

impl crate::ferro::Module for CloudFormation {
    fn name(&self) -> String {
        CLOUDFORMATION.to_owned()
    }

    fn apply(
        &self,
        context: &crate::ferro::Context,
    ) -> Result<crate::ferro::Response, crate::ferro::Error> {
        let stack_name = (self.stack_name)(context);
        let template = (self.template)(context);
        match self.get_stack_info(&stack_name) {
            Ok(_) => match self.update_stack(&stack_name, &template) {
                Ok(opt) => opt.map_or_else(
                    || crate::ferro::result_response(true, None),
                    |output| crate::ferro::result_response(true, Some(Box::new(output))),
                ),
                Err(Error::NoUpdateError) => self
                    .get_stack_info(&stack_name)
                    .map_err(|e| crate::ferro::error(false, e.to_string()))
                    .and_then(|stack| {
                        stack.outputs.map_or_else(
                            || crate::ferro::result_response(false, None),
                            |outputs| {
                                crate::ferro::result_response(
                                    false,
                                    Some(Box::new(Output {
                                        outputs: outputs_to_map(outputs),
                                    })),
                                )
                            },
                        )
                    }),
                Err(e) => crate::ferro::result_error(true, e.to_string()),
            },

            Err(Error::StackNotFoundError) => match self.create_stack(&stack_name, &template) {
                Ok(Some(output)) => crate::ferro::result_response(true, Some(Box::new(output))),
                Ok(None) => crate::ferro::result_response(true, None),
                Err(e) => crate::ferro::result_error(true, e.to_string()),
            },

            Err(e) => crate::ferro::result_error(false, e.to_string()),
        }
    }

    fn destroy(&self) -> Result<crate::ferro::Response, crate::ferro::Error> {
        Ok(crate::ferro::Response {
            changed: false,
            output: None,
        })
    }
}

fn get_region() -> Result<Region, Error> {
    match std::env::var(AWS_DEFAULT_REGION).or_else(|_| std::env::var(AWS_REGION)) {
        Ok(ref v) => Region::from_str(v).map_err(|_| Error::RegionNotFoundError),
        Err(_) => match ProfileProvider::region() {
            Ok(Some(region)) => Region::from_str(&region).map_err(|_| Error::RegionNotFoundError),
            _ => Err(Error::RegionNotFoundError),
        },
    }
}

fn outputs_to_map(outputs: Vec<CFOutput>) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for output in outputs.into_iter() {
        if let (Some(k), Some(v)) = (output.output_key, output.output_value) {
            map.insert(k, v);
        }
    }
    map
}
