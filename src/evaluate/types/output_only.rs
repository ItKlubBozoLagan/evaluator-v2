use crate::evaluate::compilation::CompilationError;
use crate::evaluate::output::{CheckerResult, OutputChecker};
use crate::evaluate::{SuccessfulEvaluation, TestcaseResult, Verdict};
use crate::messages::{OutputOnlyEvaluation, Testcase};

fn evaluate_with_testcase(
    output: &str,
    checker: &OutputChecker,
    testcase: &Testcase,
    box_id: u8,
) -> TestcaseResult {
    let Ok(check_result) = checker.check(box_id, output, testcase) else {
        return TestcaseResult {
            id: testcase.id.clone(),
            verdict: Verdict::JudgingError,
            memory: 0,
            time: 0,
            output: None,
            error: None,
        };
    };

    let verdict = match check_result {
        CheckerResult::Accepted => Verdict::Accepted,
        CheckerResult::WrongAnswer => Verdict::WrongAnswer,
        CheckerResult::Custom(message) => Verdict::Custom(message),
    };

    TestcaseResult {
        id: testcase.id.clone(),
        verdict,
        memory: 0,
        time: 0,
        output: None,
        error: None,
    }
}

pub fn evaluate(
    evaluation: &OutputOnlyEvaluation,
    box_id: u8,
) -> Result<SuccessfulEvaluation, CompilationError> {
    let checker = OutputChecker::try_from((box_id, &evaluation.checker))?;

    let result = evaluate_with_testcase(&evaluation.output, &checker, &evaluation.testcase, box_id);

    Ok(SuccessfulEvaluation {
        evaluation_id: evaluation.id,
        verdict: result.verdict.clone(),
        max_memory: 0,
        max_time: 0,
        testcases: vec![result],
    })
}
