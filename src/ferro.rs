use serde::Serialize;
use serde_json::value::Value;
use std::collections::HashMap;
use std::default::Default;
use std::error;
use std::fmt;

#[derive(fmt::Debug, Serialize)]
pub struct Response {
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Box<dyn Output>>,
}

#[derive(fmt::Debug, Serialize)]
pub struct Error {
    pub changed: bool,
    pub description: String,
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description)
    }
}

pub trait Module {
    fn name(&self) -> String;
    fn apply(&self, context: &Context) -> Result<Response, Error>;
    fn destroy(&self) -> Result<Response, Error>;
}

#[typetag::serialize(tag = "type")]
pub trait Output: fmt::Debug {
    fn to_value(&self) -> Result<Value, serde_json::error::Error>;
}

pub struct Context {
    pub vars: HashMap<String, String>,
    pub state: HashMap<String, Value>,
}

#[derive(fmt::Debug, Serialize)]
pub struct NullOutput;

#[typetag::serialize]
impl Output for NullOutput {
    fn to_value(&self) -> Result<Value, serde_json::error::Error> {
        serde_json::to_value(self)
    }
}

#[derive(fmt::Debug)]
pub struct NullError;

#[derive(fmt::Debug)]
pub struct NullModule;

impl Default for NullModule {
    fn default() -> Self {
        NullModule
    }
}

impl Module for NullModule {
    fn name(&self) -> String {
        "null".to_owned()
    }

    fn apply(&self, _context: &Context) -> Result<Response, Error> {
        Ok(Response {
            changed: false,
            output: Some(Box::new(NullOutput)),
        })
    }

    fn destroy(&self) -> Result<Response, Error> {
        Ok(Response {
            changed: false,
            output: Some(Box::new(NullOutput)),
        })
    }
}

#[derive(fmt::Debug, Serialize)]
pub struct TaskResult {
    pub module: String,
    pub succeeded: bool,
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Box<dyn Output>>,
}

pub struct Task {
    pub description: String,
    pub module: Box<dyn Module>,
    pub when: Box<dyn crate::when::When>,
}

impl Task {
    pub fn run(&self, context: &Context) -> Box<TaskResult> {
        let result = crate::when::When::when(self.when.as_ref())
            .and_then(|proceed| {
                if proceed {
                    self.module
                        .apply(context)
                        .and_then(|response| result_response(response.changed, response.output))
                        .map_err(|e| error(e.changed, e.description))
                } else {
                    result_response(false, None)
                }
            })
            .map_err(|e| error(false, e.to_string()));

        match result {
            Ok(response) => Box::new(TaskResult {
                module: self.module.name(),
                succeeded: true,
                changed: response.changed,
                error: None,
                output: response.output,
            }),
            Err(e) => Box::new(TaskResult {
                module: self.module.name(),
                succeeded: false,
                changed: e.changed,
                error: Some(e.description),
                output: None,
            }),
        }
    }
}

pub struct Playbook {
    pub tasks: Vec<Task>,
    pub context: Context,
}

impl Playbook {
    pub fn run(&mut self) -> Vec<Box<TaskResult>> {
        let mut results = vec![];
        for task in &self.tasks {
            let result = task.run(&self.context);
            if let Some(output) = result.output.as_ref() {
                if let Ok(value) = output.to_value() {
                    self.context.state.insert(task.description.clone(), value);
                }
            }
            let _ = serde_json::to_string_pretty(&result).and_then(|json_out| {
                println!("{}", json_out);
                Ok(())
            });
            let succeeded = result.succeeded;
            results.push(result);
            if succeeded {
                continue;
            } else {
                break;
            }
        }
        results
    }
}

pub fn error(changed: bool, description: String) -> Error {
    Error {
        changed: changed,
        description: description,
    }
}

pub fn result_error(changed: bool, description: String) -> Result<Response, Error> {
    Err(Error {
        changed: changed,
        description: description,
    })
}

pub fn response(changed: bool, output: Option<Box<dyn Output>>) -> Response {
    Response {
        changed: changed,
        output: output,
    }
}

pub fn result_response(changed: bool, output: Option<Box<dyn Output>>) -> Result<Response, Error> {
    Ok(Response {
        changed: changed,
        output: output,
    })
}

pub fn find(path: &str, obj: &Value) -> Result<Value, Error> {
    fn inner(path: &str, parts: &[&str], obj: &Value) -> Result<Value, Error> {
        let first_rest = parts.split_first();
        if first_rest.is_none() {
            return Ok(obj.clone());
        }
        let (first, rest) = first_rest.unwrap();
        let not_found = format!("value not found at path {}", path.to_owned());
        let not_array_index = format!("array index must be numeric at path {}", path.to_owned());

        match obj {
            v @ Value::Null | v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_) => {
                Ok(v.clone())
            }
            Value::Array(v) => {
                let is_numeric_key = first.chars().all(char::is_numeric);
                if is_numeric_key {
                    let index: usize = first.parse().unwrap();
                    match v.get(index) {
                        Some(value) => inner(path, rest, value),
                        None => Err(error(false, not_found)),
                    }
                } else {
                    Err(error(false, not_array_index))
                }
            }
            Value::Object(o) => match o.get(first.clone()) {
                Some(value) => inner(path, rest, value),
                None => Err(error(false, not_found)),
            },
        }
    }

    let parts: Vec<&str> = path.split('.').collect();
    inner(path, parts.as_slice(), obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn test_find() {
        let value = json!({
            "k1": {
                "k1": ["1", "2", "3"],
                "k2": [1, 2, 3]
            },
            "k2": {
                "k1": [{
                    "k1": "v1",
                    "k2": "v2"
                }]
            }
        });

        let found_1 = find("k1.k1.1", &value).unwrap();
        assert_eq!(found_1, "2");

        let found_2 = find("k1.k2.1", &value).unwrap();
        assert_eq!(found_2, 2);

        let found_3 = find("k2.k1.0.k1", &value).unwrap();
        assert_eq!(found_3, "v1");

        let found_obj_1 = find("k1.k1", &value).unwrap();
        assert_eq!(found_obj_1, json!(["1", "2", "3"]));

        let found_obj_2 = find("k1", &value).unwrap();
        assert_eq!(found_obj_2, json!({"k1": ["1", "2", "3"], "k2": [1, 2, 3]}));

        let found_obj_3 = find("k1.k2", &value).unwrap();
        assert_eq!(found_obj_3, json!([1, 2, 3]));

        let found_obj_4 = find("k2.k1.0", &value).unwrap();
        assert_eq!(found_obj_4, json!({"k1": "v1", "k2": "v2"}));
    }

    #[test]
    fn test_playbook() {
        let _ = fs::read_to_string("cf.yml")
            .map_err(|_| crate::modules::aws::cloudformation::Error::UnknownError)
            .and_then(|body| {
                let tasks = vec![
                    crate::ferro::Task {
                        description: "do nothing".to_owned(),
                        module: Box::new(crate::ferro::NullModule),
                        when: Box::new(crate::when::Never),
                    },
                    crate::ferro::Task {
                        description: "do nothing again".to_owned(),
                        module: Box::new(crate::ferro::NullModule),
                        when: Box::new(crate::when::Always),
                    },
                    crate::ferro::Task {
                        description: "run ls".to_owned(),
                        module: Box::new(crate::modules::command::Command {
                            command: Box::new(crate::lazy::string("/bin/ls".to_owned())),
                            args: Box::new(|_| {
                                vec![
                                    Box::new(crate::lazy::string("-l".to_owned())),
                                    Box::new(crate::lazy::string("/".to_owned())),
                                ]
                            }),
                            ..Default::default()
                        }),
                        when: Box::new(crate::when::when_execute("/bin/true")),
                    },
                    crate::ferro::Task {
                        description: "run cloudformation".to_owned(),
                        module: Box::new(crate::modules::aws::cloudformation::CloudFormation {
                            stack_name: Box::new(crate::lazy::with_default(
                                crate::lazy::var("stack_ame".to_owned()),
                                lazy_format!("foo-{}", crate::lazy::var("stack_name".to_owned())),
                            )),
                            template: Box::new(move |_| {
                                crate::modules::aws::cloudformation::Template::TemplateBody(
                                    body.clone(),
                                )
                            }),
                            ..Default::default()
                        }),
                        when: Box::new(crate::when::Always),
                    },
                    crate::ferro::Task {
                        description: "run echo".to_owned(),
                        module: Box::new(crate::modules::command::Command {
                            command: Box::new(crate::lazy::string("/bin/echo".to_owned())),
                            args: Box::new(|_| {
                                vec![Box::new(lazy_format!(
                                    "security group is {}",
                                    crate::lazy::state(
                                        "run cloudformation".to_owned(),
                                        "outputs.SecurityGroup".to_owned(),
                                    )
                                ))]
                            }),
                            ..Default::default()
                        }),
                        when: Box::new(crate::when::Always),
                    },
                ];
                let mut vars = HashMap::<String, String>::new();
                vars.insert("stack_name".to_owned(), "test-stack".to_owned());
                let mut playbook = crate::ferro::Playbook {
                    context: crate::ferro::Context {
                        vars: vars,
                        state: HashMap::<String, serde_json::value::Value>::new(),
                    },
                    tasks: tasks,
                };
                Ok(playbook.run())
            });
    }
}
