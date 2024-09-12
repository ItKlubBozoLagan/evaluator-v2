use crate::evaluate::runnable::RunnableProcess;
use crate::messages::Testcase;
use crate::util::random_bytes;

pub enum OutputChecking {
    Checker(RunnableProcess),
    Raw
}

pub enum CheckerResult {
    Accepted,
    WrongAnswer,
    Custom(String)
}

fn trim_every_line(input: &str) -> String {
    input.split("\n").map(|line| line.trim()).collect::<Vec<_>>().join(" ")
}

impl OutputChecking {
    pub fn validate(&self, output: &str, testcase: &Testcase) -> anyhow::Result<CheckerResult> {
        match self {
            OutputChecking::Checker(process) => {
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
                input.push_str(&output);
                input.push('\n');
                input.push_str(&separator);

                let running = process.run(input.as_bytes())?;

                let output = running.wait_with_output()?;

                if !output.status.success() {
                    todo!("throw error")
                }

                let text_output = String::from_utf8_lossy(&output.stdout);
                let text_output = text_output.trim();

                if text_output.starts_with("custom:") {
                    let (_, message) = text_output.split_once(':').unwrap();

                    return Ok(CheckerResult::Custom(message.to_string()))
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
            },
            OutputChecking::Raw => {
                if trim_every_line(output) == trim_every_line(&testcase.output) {
                    return Ok(CheckerResult::Accepted)
                }

                Ok(CheckerResult::WrongAnswer)
            }
        }
    }
}