use crate::environment::Environment;
use crate::evaluate::compilation::{process_compilation, CompilationError};
use crate::evaluate::output::CheckerResult;
use crate::evaluate::runnable::{ProcessRunError, RunnableProcess};
use crate::evaluate::{SuccessfulEvaluation, TestcaseResult, Verdict};
use crate::isolate::meta::ProcessStatus;
use crate::isolate::{IsolateError, IsolateLimits, ProcessInput};
use crate::messages::{InteractiveEvaluation, Testcase};
use crate::util::random_bytes;
use log::warn;
use std::cmp::min;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::path::PathBuf;
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tracing::debug;

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
}

#[derive(Debug)]
enum WriteHandle {
    Direct,
    Async(JoinHandle<()>),
}

impl Drop for WriteHandle {
    fn drop(&mut self) {
        let WriteHandle::Async(handle) = self else {
            return;
        };

        handle.abort();
    }
}

fn write_to_fd_safe(fd: BorrowedFd, input: &[u8]) -> Result<WriteHandle, InteractError> {
    let current_pipe_buf_size =
        nix::fcntl::fcntl(fd.as_raw_fd(), nix::fcntl::FcntlArg::F_GETPIPE_SZ)?;

    let input_size = input.len();

    if input_size < (current_pipe_buf_size as usize) {
        nix::unistd::write(fd, input)?;
        nix::unistd::write(fd, b"\n")?;

        return Ok(WriteHandle::Direct);
    }

    let needed_pipe_buf = min(
        input_size + 1,
        Environment::get().system_environment.pipe_max_size,
    );

    // 2 cases from this point:
    //  - input is within bounds of pipe_max_size so extend pipe to that, write directly
    //  - input is larger than pipe_max_size, write async,
    //      extend pipe to pipe_max_size (or input_size if pipe_max_size is not available)
    nix::fcntl::fcntl(
        fd.as_raw_fd(),
        nix::fcntl::FcntlArg::F_SETPIPE_SZ(needed_pipe_buf as i32),
    )?;
    debug!("increasing pipe buffer size to {}", needed_pipe_buf);

    if input_size < needed_pipe_buf {
        nix::unistd::write(fd, input)?;
        nix::unistd::write(fd, b"\n")?;

        return Ok(WriteHandle::Direct);
    }

    let fd_clone = fd.try_clone_to_owned()?;
    let input_clone = input.to_vec();
    let handle = Handle::current().spawn(async move {
        // handle directly, this handle is killed as soon as both interactor and client are done

        if let Err(err) = nix::unistd::write(&fd_clone, &input_clone) {
            warn!("failed to async write to pipe: {}", err);
        };

        if let Err(err) = nix::unistd::write(&fd_clone, b"\n") {
            warn!("failed to async write to pipe: {}", err);
        };
    });

    Ok(WriteHandle::Async(handle))
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

    let write_handle = write_to_fd_safe(process_output.as_fd(), testcase.input.as_bytes())?;

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
