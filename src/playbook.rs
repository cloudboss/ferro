#[macro_export]
macro_rules! playbook {
    () => {};

    (@when) => {
        Box::new($crate::when::Always)
    };

    (@when $when:tt) => {
        Box::new($when)
    };

    (@task $description:tt {
        module: $module:tt {
            $($field:ident: $field_value:expr),*
        }
	$(, when: $when:tt )?
    }) => {{
        $crate::ferro::Task {
            description: $description.to_owned(),
            module: Box::new($module {
                $( $field: Box::new($field_value), )*
                ..Default::default()
            }),
            when: playbook! { @when $($when)? }
        }
    }};

    (
        vars { $($key:tt: $value:tt),* }
        $( task $description:tt $rest:tt )*
    ) => {{
        use ::serde_json::value::Value;
        use ::std::collections::HashMap;

        let mut vars = HashMap::<String, String>::new();
        $( vars.insert($key.to_owned(), $value.to_owned()); )*

        let mut tasks = Vec::<crate::ferro::Task>::new();
        $( tasks.push(playbook! { @task $description $rest }); )*

        crate::ferro::Playbook {
            context: crate::ferro::Context {
                vars: vars,
                state: HashMap::<String, Value>::new(),
            },
            tasks: tasks,
        }
    }};
}

#[cfg(test)]
mod tests {
    use crate::ferro::NullModule;
    use crate::modules::aws::cloudformation::{CloudFormation, Template};
    use crate::modules::command::Command;
    use std::fs;

    #[test]
    fn test_playbook() {
        let cf_template = fs::read_to_string("cf.yml").unwrap();
        let mut pb = playbook! {
            vars {
                "hi": "hello",
                "bye": "goodbye"
            }

            task "do nothing" {
                module: NullModule {},
                when: (crate::when::Never)
            }

            task "run a command" {
                module: Command {
                    command: crate::lazy::string("ls".to_owned()),
                    args: |_| {
                        vec![
                            Box::new(crate::lazy::string("/etc".to_owned())),
                        ]
                    }
                },
                when: (crate::when::when_execute("/bin/true"))
            }

            task "run another command" {
                module: Command {
                    command: crate::lazy::string("/bin/echo".to_owned()),
                    args: |_| {
                        vec![
                            Box::new(crate::lazy::var("bye".to_owned())),
                        ]
                    }
                }
            }

            task "run cloudformation" {
                module: CloudFormation {
                    stack_name: lazy_format!(
                        "{}-{}", crate::lazy::var("hi".to_owned()),
                        crate::lazy::string("test-stack".to_owned())),
                    template: move |_| Template::TemplateBody(cf_template.clone())
                }
            }
        };

        let results = pb.run();
        assert!(results.into_iter().all(|r| r.succeeded));
    }
}
