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
            id: testcase.id,
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
        id: testcase.id,
        verdict,
        memory: 0,
        time: 0,
        error: None,
    }
}

pub fn evaluate(
    evaluation: &OutputOnlyEvaluation,
) -> Result<SuccessfulEvaluation, EvaluationError> {
    let checker = (&evaluation.checker).try_into()?;

    let mut global_verdict = Verdict::Accepted;

    let mut testcase_results = Vec::<TestcaseResult>::new();

    for testcase in &evaluation.testcases {
        if global_verdict != Verdict::Accepted && !matches!(global_verdict, Verdict::Custom(_)) {
            testcase_results.push(TestcaseResult {
                id: testcase.id,
                verdict: Verdict::Skipped,
                memory: 0,
                time: 0,
                error: None,
            });
            continue;
        }

        let result = evaluate_with_testcase(&evaluation.output, &checker, testcase);
        let result_verdict = result.verdict.clone();

        testcase_results.push(result);

        global_verdict = result_verdict;
    }

    Ok(SuccessfulEvaluation {
        verdict: global_verdict,
        max_memory: 0,
        max_time: 0,
        testcases: testcase_results,
    })
}
