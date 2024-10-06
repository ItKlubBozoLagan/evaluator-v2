use crate::evaluate::compilation::{process_compilation, CompilationError};
use crate::evaluate::runnable::{ProcessRunError, RunnableProcess};
use crate::evaluate::Verdict;
use crate::isolate::{IsolateLimits, ProcessInput};
use crate::messages::{CheckerData, Testcase};
use crate::util::random_bytes;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CheckerError {
    #[error("Process run error: {0}")]
    ProcessError(#[from] ProcessRunError),

    // time limit, memory limit, etc.
    #[error("Checker failed")]
    CheckerFailed,

    // checker returned invalid verdict
    #[error("Invalid checker")]
    InvalidChecker,
}

impl From<&CheckerError> for Verdict {
    fn from(value: &CheckerError) -> Self {
        match value {
            CheckerError::ProcessError(_) => Verdict::SystemError,
            CheckerError::CheckerFailed | CheckerError::InvalidChecker => Verdict::JudgingError,
        }
    }
}

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
    pub fn check(&self, output: &str, testcase: &Testcase) -> Result<CheckerResult, CheckerError> {
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

                let output = process
                    .run(
                        ProcessInput::StdIn(input.as_bytes().to_vec()),
                        // TODO: extract into variables
                        &IsolateLimits {
                            time_limit: 30.0,
                            memory_limit: 1 << 20, // 1 GiB
                        },
                        None,
                    )?
                    .output;

                if !output.status.success() {
                    return Err(CheckerError::CheckerFailed);
                }

                let text_output = String::from_utf8_lossy(&output.stdout);
                let text_output = text_output.trim();

                // FIXME: legacy
                text_output.try_into()
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

impl TryFrom<&str> for CheckerResult {
    type Error = CheckerError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.starts_with("custom:") {
            let (_, message) = value.split_once(':').unwrap();

            return Ok(CheckerResult::Custom(message.to_string()));
        }

        let text_output = value.to_ascii_lowercase();

        if text_output == "ac" || text_output == "accepted" {
            return Ok(CheckerResult::Accepted);
        }

        if text_output == "wa" || text_output == "wrong_answer" {
            return Ok(CheckerResult::WrongAnswer);
        }

        Err(CheckerError::InvalidChecker)
    }
}
