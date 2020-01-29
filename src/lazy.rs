use serde_json::value::Value;

#[macro_export]
macro_rules! lazy_format {
    ($s:expr, $($arg:expr),*) => {
        |context| format!($s, $($arg(context)),*)
    };
}

pub fn var(path: std::string::String) -> impl Fn(&crate::ferro::Context) -> std::string::String {
    move |context| {
        if let Some(value) = context.vars.get(&path) {
            value.to_owned()
        } else {
            "".to_owned()
        }
    }
}

pub fn state(
    task_description: std::string::String,
    path: std::string::String,
) -> impl Fn(&crate::ferro::Context) -> std::string::String {
    move |context| {
        if let Some(task) = context.state.get(&task_description) {
            if let Ok(Value::String(value)) = crate::ferro::find(&path, task) {
                value.to_owned()
            } else {
                "".to_owned()
            }
        } else {
            "".to_owned()
        }
    }
}

pub fn with_default(
    f: impl Fn(&crate::ferro::Context) -> std::string::String,
    default: impl Fn(&crate::ferro::Context) -> std::string::String,
) -> impl Fn(&crate::ferro::Context) -> std::string::String {
    move |context| {
        let value = f(context);
        if value != "" {
            value
        } else {
            default(context)
        }
    }
}

pub fn string(s: std::string::String) -> impl Fn(&crate::ferro::Context) -> std::string::String {
    move |_context| s.to_owned()
}

pub type String = dyn Fn(&crate::ferro::Context) -> std::string::String;

pub type Vec<T> = dyn Fn(&crate::ferro::Context) -> std::vec::Vec<T>;
