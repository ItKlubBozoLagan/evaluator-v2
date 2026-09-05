use crate::evaluate::compilation::process_compilation;
use crate::evaluate::output::CheckerResult;
use crate::evaluate::runnable::{ProcessRunError, RunnableProcess};
use crate::evaluate::{
    EvaluationError, SuccessfulEvaluation, TestcaseResult, Verdict, aggregate_verdict,
};
use crate::isolate::meta::ProcessStatus;
use crate::isolate::{IsolateError, IsolateLimits, ProcessInput};
use crate::messages::{InteractiveEvaluation, Testcase};
use crate::util::general::random_bytes;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::thread;
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

    #[error("interactive pipe worker failed")]
    PipeWorkerFailed,
}

fn interact_with_testcase(
    process: &RunnableProcess,
    interactor: &RunnableProcess,
    testcase: &Testcase,
    limits: &IsolateLimits,
    interactor_limits: &IsolateLimits,
    box_id: u8,
    interactor_box_id: u8,
) -> Result<TestcaseResult, InteractError> {
    let (interactor_input, interactor_stdin) = nix::unistd::pipe()?;
    let (process_stdout, process_output) = nix::unistd::pipe()?;
    let (process_input, process_stdin) = nix::unistd::pipe()?;
    let (interactor_stdout, interactor_output) = nix::unistd::pipe()?;
    let output_limit = crate::environment::Environment::get().max_output_bytes;
    let process_pump = pump_bounded(
        process_stdout,
        interactor_stdin,
        [testcase.input.as_bytes(), b"\n"].concat(),
        output_limit,
    );
    let interactor_pump = pump_bounded(interactor_stdout, process_stdin, Vec::new(), output_limit);

    let mut interactor = interactor.just_run(
        interactor_box_id,
        ProcessInput::Piped(interactor_input),
        interactor_limits,
        Some(interactor_output),
    )?;

    let mut process = process.just_run(
        box_id,
        ProcessInput::Piped(process_input),
        limits,
        Some(process_output),
    )?;

    let process_output = process.wait_for_output()?;
    let interactor_output = interactor.wait_for_output()?;
    let process_meta = process.load_meta()?;
    let process_limit_exceeded = process_pump
        .join()
        .map_err(|_| InteractError::PipeWorkerFailed)??;
    let interactor_limit_exceeded = interactor_pump
        .join()
        .map_err(|_| InteractError::PipeWorkerFailed)??;

    if process_limit_exceeded || interactor_limit_exceeded {
        process.cleanup_and_reset()?;
        interactor.cleanup_and_reset()?;
        return Ok(TestcaseResult {
            id: testcase.id.clone(),
            verdict: if process_limit_exceeded {
                Verdict::OutputLimitExceeded
            } else {
                Verdict::JudgingError
            },
            memory: process_meta.cg_mem_kb,
            time: process_meta.time_ms,
            output: None,
            error: Some(if process_limit_exceeded {
                "contestant output exceeded the output limit".to_string()
            } else {
                "interactor output exceeded the output limit".to_string()
            }),
        });
    }

    if !interactor_output.status.success() {
        process.cleanup_and_reset()?;
        interactor.cleanup_and_reset()?;
        return Ok(TestcaseResult {
            id: testcase.id.clone(),
            verdict: Verdict::JudgingError,
            memory: process_meta.cg_mem_kb,
            time: process_meta.time_ms,
            output: None,
            error: Some(String::from_utf8_lossy(&interactor_output.stderr).to_string()),
        });
    }

    // TODO: may not work, stdout is connected to interactor
    let process_stdout = String::from_utf8_lossy(&process_output.stdout).to_string();

    // FIXME: repeated
    if !process_output.status.success() {
        process.cleanup_and_reset()?;
        interactor.cleanup_and_reset()?;

        let verdict = match process_meta.status {
            Some(ProcessStatus::TimedOut) => Verdict::TimeLimitExceeded,
            Some(ProcessStatus::SandboxError) => Verdict::SystemError,
            _ if process_meta.cg_oom_killed => Verdict::MemoryLimitExceeded,
            _ => Verdict::RuntimeError,
        };

        return Ok(TestcaseResult {
            id: testcase.id.clone(),
            verdict,
            memory: process_meta.cg_mem_kb,
            time: process_meta.time_ms,
            output: Some(process_stdout),
            error: Some(String::from_utf8_lossy(&process_output.stderr).to_string()),
        });
    }

    let out_meta_file = PathBuf::from(format!("/tmp/{}", random_bytes(8)));
    interactor.move_out_of_box("interactor_meta.out", &out_meta_file)?;

    let interactor_result = read_and_remove_interactor_result(&out_meta_file);
    let process_cleanup = process.cleanup_and_reset();
    let interactor_cleanup = interactor.cleanup_and_reset();
    process_cleanup?;
    interactor_cleanup?;
    let interactor_result = interactor_result?;
    if interactor_result.len() > crate::environment::Environment::get().max_output_bytes {
        return Ok(TestcaseResult {
            id: testcase.id.clone(),
            verdict: Verdict::JudgingError,
            memory: process_meta.cg_mem_kb,
            time: process_meta.time_ms,
            output: None,
            error: Some("interactor result exceeded the output limit".to_string()),
        });
    }

    let check_result = CheckerResult::try_from(interactor_result.trim());

    let check_result = match check_result {
        Ok(result) => result,
        Err(err) => {
            return Ok(TestcaseResult {
                id: testcase.id.clone(),
                verdict: Verdict::from(&err),
                memory: 0,
                time: 0,
                output: Some(process_stdout),
                error: Some(err.to_string()),
            });
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
        output: Some(process_stdout),
        error: None,
    })
}

fn read_and_remove_interactor_result(path: &Path) -> std::io::Result<String> {
    let result = (|| {
        let mut file = File::open(path)?;
        let mut output = String::new();
        (&mut file)
            .take(crate::environment::Environment::get().max_output_bytes as u64 + 1)
            .read_to_string(&mut output)?;
        Ok(output)
    })();
    let remove_result = fs::remove_file(path);
    match (result, remove_result) {
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(output), Ok(())) => Ok(output),
    }
}

fn pump_bounded(
    input: OwnedFd,
    output: OwnedFd,
    prefix: Vec<u8>,
    limit: usize,
) -> thread::JoinHandle<std::io::Result<bool>> {
    thread::spawn(move || {
        let mut input = File::from(input);
        let mut output = File::from(output);
        if let Err(err) = output.write_all(&prefix) {
            if err.kind() == std::io::ErrorKind::BrokenPipe {
                return Ok(false);
            }
            return Err(err);
        }

        let mut transferred = 0_usize;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                return Ok(false);
            }
            let remaining = limit.saturating_sub(transferred);
            if let Err(err) = output.write_all(&buffer[..read.min(remaining)]) {
                if err.kind() == std::io::ErrorKind::BrokenPipe {
                    return Ok(false);
                }
                return Err(err);
            }
            transferred += read.min(remaining);
            if read > remaining {
                return Ok(true);
            }
        }
    })
}

pub fn evaluate(
    evaluation: &InteractiveEvaluation,
    box_id: u8,
    interactor_box_id: u8,
) -> Result<SuccessfulEvaluation, EvaluationError> {
    let compiled_program = process_compilation(&evaluation.code, &evaluation.language, box_id)
        .map_err(EvaluationError::contestant_compilation)?;

    let compiled_interactor = process_compilation(
        &evaluation.checker.script,
        &evaluation.checker.language,
        interactor_box_id,
    )
    .map_err(EvaluationError::judging_compilation)?;

    let program = compiled_program.process;

    let interactor = compiled_interactor.process;

    let limits = IsolateLimits {
        time_limit: evaluation.time_limit as f32 / 1000.0,
        memory_limit: evaluation.memory_limit,
    };
    let interactor_limits = IsolateLimits {
        time_limit: 30.0,
        memory_limit: crate::environment::Environment::get().system_memory_reserve_kib,
    };

    let mut global_verdict = Verdict::Accepted;

    let mut testcase_results = Vec::<TestcaseResult>::new();

    for testcase in &evaluation.testcases {
        if crate::deadline::exceeded() {
            return Err(EvaluationError::Deadline);
        }
        if !evaluation.evaluate_all
            && (global_verdict != Verdict::Accepted
                && !matches!(global_verdict, Verdict::Custom(_)))
        {
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

        let result = interact_with_testcase(
            &program,
            &interactor,
            testcase,
            &limits,
            &interactor_limits,
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
                output: None,
                error: Some(err.to_string()),
            },
        };

        let result_verdict = result.verdict.clone();

        testcase_results.push(result);

        global_verdict = aggregate_verdict(&global_verdict, &result_verdict);
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
        compiler_output: compiled_program.compiler_stderr,
    })
}
