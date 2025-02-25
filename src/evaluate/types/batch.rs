use crate::evaluate::compilation::{process_compilation, CompilationError};
use crate::evaluate::output::{CheckerResult, OutputChecker};
use crate::evaluate::runnable::{ProcessRunResult, RunnableProcess};
use crate::evaluate::{SuccessfulEvaluation, TestcaseResult, Verdict};
use crate::isolate::meta::ProcessStatus;
use crate::isolate::{IsolateLimits, ProcessInput};
use crate::messages::{BatchEvaluation, Testcase};

fn evaluate_with_testcase(
    box_id: u8,
    process: &RunnableProcess,
    checker: &OutputChecker,
    testcase: &Testcase,
    limits: &IsolateLimits,
) -> TestcaseResult {
    let running_process = process.run(
        box_id,
        ProcessInput::StdIn(testcase.input.as_bytes().to_vec()),
        limits,
        None,
    );

    let ProcessRunResult { output, meta } = match running_process {
        Ok(it) => it,
        Err(err) => {
            return TestcaseResult {
                id: testcase.id.clone(),
                verdict: Verdict::SystemError,
                memory: 0,
                time: 0,
                output: None,
                error: Some(err.to_string()),
            }
        }
    };

    let output_str = String::from_utf8_lossy(&output.stdout).to_string();

    // FIXME: repeated
    if !output.status.success() {
        let verdict = if let Some(ProcessStatus::TimedOut) = meta.status {
            Verdict::TimeLimitExceeded
        } else if meta.cg_oom_killed {
            Verdict::MemoryLimitExceeded
        } else {
            Verdict::RuntimeError
        };

        return TestcaseResult {
            id: testcase.id.clone(),
            verdict,
            memory: meta.cg_mem_kb,
            time: meta.time_ms,
            output: Some(output_str),
            error: Some(String::from_utf8_lossy(&output.stderr).to_string()),
        };
    }

    let check_result = match checker.check(box_id, &output_str, testcase) {
        Ok(result) => result,
        Err(err) => {
            return TestcaseResult {
                id: testcase.id.clone(),
                verdict: (&err).into(),
                memory: 0,
                time: 0,
                output: Some(output_str),
                error: Some(err.to_string()),
            }
        }
    };

    let verdict = match check_result {
        CheckerResult::Accepted => Verdict::Accepted,
        CheckerResult::WrongAnswer => Verdict::WrongAnswer,
        CheckerResult::Custom(message) => Verdict::Custom(message),
    };

    TestcaseResult {
        id: testcase.id.clone(),
        verdict,
        memory: meta.cg_mem_kb,
        time: meta.time_ms,
        output: Some(output_str),
        error: None,
    }
}

pub fn evaluate(
    evaluation: &BatchEvaluation,
    box_id: u8,
) -> Result<SuccessfulEvaluation, CompilationError> {
    let compilation_result = process_compilation(&evaluation.code, &evaluation.language, box_id)?;

    let checker = OutputChecker::try_from((box_id, &evaluation.checker))?;

    let limits = IsolateLimits {
        time_limit: evaluation.time_limit as f32 / 1000.0,
        memory_limit: evaluation.memory_limit,
    };

    let mut global_verdict = Verdict::Accepted;

    let mut testcase_results = Vec::<TestcaseResult>::new();

    for testcase in &evaluation.testcases {
        if global_verdict != Verdict::Accepted && !matches!(global_verdict, Verdict::Custom(_)) {
            testcase_results.push(TestcaseResult {
                id: testcase.id.clone(),
                verdict: Verdict::Skipped,
                memory: 0,
                time: 0,
                output: None,
                error: None,
            });
            continue;
        }

        let result = evaluate_with_testcase(
            box_id,
            &compilation_result.process,
            &checker,
            testcase,
            &limits,
        );
        let result_verdict = result.verdict.clone();

        testcase_results.push(result);

        global_verdict = result_verdict;
    }

    Ok(SuccessfulEvaluation {
        evaluation_id: evaluation.id,
        verdict: global_verdict,
        max_memory: testcase_results
            .iter()
            .map(|it| it.memory)
            .max()
            .unwrap_or(0),
        max_time: testcase_results.iter().map(|it| it.time).max().unwrap_or(0),
        testcases: testcase_results,
        compiler_output: compilation_result.compiler_stderr,
    })
}
