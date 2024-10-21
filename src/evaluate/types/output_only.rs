use crate::evaluate::output::{CheckerResult, OutputChecker};
use crate::evaluate::{EvaluationError, SuccessfulEvaluation, TestcaseResult, Verdict};
use crate::messages::{OutputOnlyEvaluation, Testcase};

fn evaluate_with_testcase(
    output: &str,
    checker: &OutputChecker,
    testcase: &Testcase,
) -> TestcaseResult {
    let Ok(check_result) = checker.check(output, testcase) else {
        return TestcaseResult {
            id: testcase.id.clone(),
            verdict: Verdict::JudgingError,
            memory: 0,
            time: 0,
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
        error: None,
    }
}

pub fn evaluate(
    evaluation: &OutputOnlyEvaluation,
) -> Result<SuccessfulEvaluation, EvaluationError> {
    let checker = OutputChecker::try_from(&evaluation.checker)?;

    let result = evaluate_with_testcase(&evaluation.output, &checker, &evaluation.testcase);

    Ok(SuccessfulEvaluation {
        evaluation_id: evaluation.id,
        verdict: result.verdict.clone(),
        max_memory: 0,
        max_time: 0,
        testcases: vec![result],
    })
}
