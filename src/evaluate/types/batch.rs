use crate::evaluate::compilation::process_compilation;
use crate::evaluate::output::{CheckerResult, OutputChecking};
use crate::evaluate::{EvaluationError, SuccessfulEvaluation, TestcaseResult, Verdict};
use crate::messages::Evaluation;

pub fn evaluate(evaluation: &Evaluation) -> Result<SuccessfulEvaluation, EvaluationError> {
    let compilation_result = process_compilation(&evaluation.code, &evaluation.language)?;


    let checker = match &evaluation.checker_script {
        Some(script) if evaluation.checker_language.is_some() => {
            let language = evaluation.checker_language.as_ref().unwrap();

            let compiled_checker = process_compilation(script, language)?;

            OutputChecking::Checker(compiled_checker.process)
        }
        _ => OutputChecking::Raw
    };

    let mut global_verdict = Verdict::Accepted;

    let _testcase_results = Vec::<TestcaseResult>::new();

    for testcase in &evaluation.testcases {
        // TODO: measure time and memory
        let running_process = compilation_result.process.run(testcase.input.as_bytes());
        let Ok(running_process) = running_process else {
            global_verdict = Verdict::JudgingError;
            break;
        };

        let Ok(output) = running_process.wait_with_output() else {
            global_verdict = Verdict::JudgingError;
            break;
        };

        if !output.status.success() {
            global_verdict = Verdict::RuntimeError;
            break;
        }

        let output_str = String::from_utf8_lossy(&output.stdout).to_string();

        let Ok(check_result) = checker.check(&output_str, testcase) else {
            global_verdict = Verdict::JudgingError;
            break;
        };

        let _verdict = match check_result {
            CheckerResult::Accepted => Verdict::Accepted,
            CheckerResult::WrongAnswer => Verdict::WrongAnswer,
            // TODO: ?
            CheckerResult::Custom(_message) => Verdict::Accepted
        };

        // TODO: finish
        // testcase_results.push(TestcaseResult {
        //     verdict,
        //
        // })
    }

    Ok(SuccessfulEvaluation {
        verdict: global_verdict,
        max_memory: 0,
        max_time: 0,
        testcases: Vec::new()
    })
}