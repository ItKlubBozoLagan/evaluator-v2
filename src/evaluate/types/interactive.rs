use crate::evaluate::compilation::{process_compilation, CompilationError};
use crate::evaluate::output::CheckerResult;
use crate::evaluate::runnable::{ProcessRunError, RunnableProcess};
use crate::evaluate::{SuccessfulEvaluation, TestcaseResult, Verdict};
use crate::isolate::meta::ProcessStatus;
use crate::isolate::{IsolateError, IsolateLimits, ProcessInput};
use crate::messages::{InteractiveEvaluation, Testcase};
use crate::util::fd::{write_to_fd_safe, LargeWriteStrategy, SafeFdWriteError};
use crate::util::general::random_bytes;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::os::fd::AsFd;
use std::path::PathBuf;
use thiserror::Error;

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
enum InteractError {
    #[error("syscall error: {0}")]
    NixError(#[from] nix::Error),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Parse int error: {0}")]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("Process run error: {0}")]
    ProcessRunError(#[from] ProcessRunError),

    #[error("Isolate error: {0}")]
    IsolateError(#[from] IsolateError),

    #[error("FD write error: {0}")]
    FdWriteError(#[from] SafeFdWriteError),
}

fn interact_with_testcase(
    process: &RunnableProcess,
    interactor: &RunnableProcess,
    testcase: &Testcase,
    limits: &IsolateLimits,
    box_id: u8,
    interactor_box_id: u8,
) -> Result<TestcaseResult, InteractError> {
    let (interactor_input, process_output) = nix::unistd::pipe()?;
    let (process_input, interactor_output) = nix::unistd::pipe()?;

    let write_handle = write_to_fd_safe(
        process_output.as_fd(),
        &[testcase.input.as_bytes(), b"\n"].concat(),
        LargeWriteStrategy::Async,
    )?;

    let mut interactor = interactor.just_run(
        interactor_box_id,
        ProcessInput::Piped(interactor_input),
        limits,
        Some(interactor_output),
    )?;

    let mut process = process.just_run(
        box_id,
        ProcessInput::Piped(process_input),
        limits,
        Some(process_output),
    )?;

    let process_output = process.wait_for_output()?;
    let _ = interactor.wait_for_output()?;

    drop(write_handle);

    let process_meta = process.load_meta()?;

    // FIXME: repeated
    if !process_output.status.success() {
        process.cleanup_and_reset()?;
        interactor.cleanup_and_reset()?;

        let verdict = if let Some(ProcessStatus::TimedOut) = process_meta.status {
            Verdict::TimeLimitExceeded
        } else if process_meta.cg_oom_killed {
            Verdict::MemoryLimitExceeded
        } else {
            Verdict::RuntimeError
        };

        return Ok(TestcaseResult {
            id: testcase.id.clone(),
            verdict,
            memory: process_meta.cg_mem_kb,
            time: process_meta.time_ms,
            error: Some(String::from_utf8_lossy(&process_output.stderr).to_string()),
        });
    }

    let out_meta_file = PathBuf::from(format!("/tmp/{}", random_bytes(8)));
    interactor.move_out_of_box("interactor_meta.out", &out_meta_file)?;

    process.cleanup_and_reset()?;
    interactor.cleanup_and_reset()?;

    let mut interactor_meta_file = File::open(&out_meta_file)?;

    let mut interactor_result = String::new();

    interactor_meta_file.read_to_string(&mut interactor_result)?;

    fs::remove_file(&out_meta_file)?;

    let check_result = CheckerResult::try_from(interactor_result.trim());

    let check_result = match check_result {
        Ok(result) => result,
        Err(err) => {
            return Ok(TestcaseResult {
                id: testcase.id.clone(),
                verdict: Verdict::from(&err),
                memory: 0,
                time: 0,

                error: Some(err.to_string()),
            })
        }
    };

    let verdict = match check_result {
        CheckerResult::Accepted => Verdict::Accepted,
        CheckerResult::WrongAnswer => Verdict::WrongAnswer,
        CheckerResult::Custom(message) => Verdict::Custom(message),
    };

    Ok(TestcaseResult {
        id: testcase.id.clone(),
        verdict,
        // TODO: backend most likely wants bytes
        memory: process_meta.cg_mem_kb,
        time: process_meta.time_ms,
        error: None,
    })
}

pub fn evaluate(
    evaluation: &InteractiveEvaluation,
    box_id: u8,
    interactor_box_id: u8,
) -> Result<SuccessfulEvaluation, CompilationError> {
    let compiled_program = process_compilation(&evaluation.code, &evaluation.language, box_id)?;

    let compiled_interactor = process_compilation(
        &evaluation.checker.script,
        &evaluation.checker.language,
        interactor_box_id,
    )?;

    let program = compiled_program.process;

    let interactor = compiled_interactor.process;

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
                error: None,
            });
            continue;
        }

        let result = interact_with_testcase(
            &program,
            &interactor,
            testcase,
            &limits,
            box_id,
            interactor_box_id,
        );

        let result = match result {
            Ok(res) => res,
            Err(err) => TestcaseResult {
                id: testcase.id.clone(),
                verdict: Verdict::SystemError,
                time: 0,
                memory: 0,
                error: Some(err.to_string()),
            },
        };

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
    })
}
