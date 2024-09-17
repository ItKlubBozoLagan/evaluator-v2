use crate::evaluate::compilation::process_compilation;
use crate::evaluate::runnable::RunnableProcess;
use crate::evaluate::CompilationError;
use crate::messages::{CheckerData, Testcase};
use crate::util::random_bytes;

pub enum OutputChecker {
    Script(RunnableProcess),
    Raw,
}

pub enum CheckerResult {
    Accepted,
    WrongAnswer,
    Custom(String),
}

fn trim_every_line(input: &str) -> String {
    input
        .split('\n')
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

impl OutputChecker {
    pub fn check(&self, output: &str, testcase: &Testcase) -> anyhow::Result<CheckerResult> {
        dbg!(&output);
        dbg!(&testcase);

        match self {
            OutputChecker::Script(process) => {
                let mut separator = String::from("[");
                separator.push_str(&random_bytes(32));
                separator.push_str("]\n");

                let mut input = String::new();
                input.push_str(&separator);
                input.push_str(&testcase.input);
                input.push('\n');
                input.push_str(&separator);
                input.push_str(&testcase.output);
                input.push('\n');
                input.push_str(&separator);
                input.push_str(output);
                input.push('\n');
                input.push_str(&separator);

                let output = process.run(input.as_bytes())?;

                if !output.status.success() {
                    todo!("throw error")
                }

                let text_output = String::from_utf8_lossy(&output.stdout);
                let text_output = text_output.trim();

                if text_output.starts_with("custom:") {
                    let (_, message) = text_output.split_once(':').unwrap();

                    return Ok(CheckerResult::Custom(message.to_string()));
                }

                let text_output = text_output.to_ascii_lowercase();

                if text_output == "ac" || text_output == "accepted" {
                    return Ok(CheckerResult::Accepted);
                }

                if text_output == "wa" || text_output == "wrong_answer" {
                    return Ok(CheckerResult::WrongAnswer);
                }

                // TODO: gracefully handle
                unreachable!();
            }
            OutputChecker::Raw => {
                if trim_every_line(output) == trim_every_line(&testcase.output) {
                    return Ok(CheckerResult::Accepted);
                }

                Ok(CheckerResult::WrongAnswer)
            }
        }
    }
}

impl TryFrom<&Option<CheckerData>> for OutputChecker {
    type Error = CompilationError;

    fn try_from(value: &Option<CheckerData>) -> Result<Self, Self::Error> {
        match value {
            Some(CheckerData { script, language }) => {
                let compiled_checker = process_compilation(script, language)?;

                Ok(OutputChecker::Script(compiled_checker.process))
            }
            _ => Ok(OutputChecker::Raw),
        }
    }
}
