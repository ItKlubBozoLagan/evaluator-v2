use crate::isolate::wrap_isolate;
use std::process::Child;

#[derive(Debug)]
pub enum RunnableProcess {
    Compiled(String),
    Python(String)
}

impl RunnableProcess {
    pub fn run(&self) -> std::io::Result<Child> {
        match self {
            RunnableProcess::Compiled(file) => wrap_isolate((file, &[]), None)
                .spawn(),
            RunnableProcess::Python(code) =>
                wrap_isolate(("python", &["-c".to_string(), code.clone()]), None).spawn()

        }
    }
}