use crate::evaluate::compilation::{process_compilation, CompilationError};
use crate::evaluate::output::CheckerResult;
use crate::evaluate::runnable::{ProcessRunError, RunnableProcess};
use crate::evaluate::{SuccessfulEvaluation, TestcaseResult, Verdict};
use crate::isolate::meta::ProcessStatus;
use crate::isolate::{IsolateError, IsolateLimits, ProcessInput};
use crate::messages::{InteractiveEvaluation, Testcase};
use crate::util::random_bytes;
use log::warn;
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

    #[error("{0}")]
    Custom(String),
}

fn read_pipe_max_size() -> Result<usize, InteractError> {
    let content = fs::read_to_string("/proc/sys/fs/pipe-max-size")?;
    let pipe_max_size: usize = content.trim().parse()?;

    Ok(pipe_max_size)
}

async fn write_to_fd_async(_fd: BorrowedFd<'_>, _input: &[u8]) {
    todo!()
}

// 2MB
const HARD_PIPE_MAX_SIZE_LIMIT: usize = 2 << 20;

fn write_to_fd_safe(fd: BorrowedFd, input: &[u8]) -> Result<Option<JoinHandle<()>>, InteractError> {
    let current_pipe_buf_size =
        nix::fcntl::fcntl(fd.as_raw_fd(), nix::fcntl::FcntlArg::F_GETPIPE_SZ)?;

    debug!("input size: {}", input.len());
    debug!("current pipe buf size: {}", current_pipe_buf_size);

    let input_size = input.len();

    if input.len() < (current_pipe_buf_size as usize) {
        debug!("writing all directly");

        nix::unistd::write(fd, input)?;
        nix::unistd::write(fd, b"\n")?;

        return Ok(None);
    }

    // handle in separate thread
    let pipe_max_size = read_pipe_max_size();

    let pipe_max_size = match pipe_max_size {
        Ok(size) => Some(size),
        Err(err) => {
            warn!("failed to read pipe max size: {}", err);
            None
        }
    };

    if let Some(pipe_max_size) = pipe_max_size {
        if pipe_max_size <= HARD_PIPE_MAX_SIZE_LIMIT && input_size < pipe_max_size {
            debug!("increasing pipe buf size to {}", input_size);
            nix::fcntl::fcntl(
                fd.as_raw_fd(),
                nix::fcntl::FcntlArg::F_SETPIPE_SZ(input_size as i32),
            )?;

            nix::unistd::write(fd, input)?;
            nix::unistd::write(fd, b"\n")?;

            return Ok(None);
        }
    };

    debug!("writing in separate thread");

    let fd_clone = fd.try_clone_to_owned()?;
    let input_clone = input.to_vec();
    let handle = Handle::current()
        .spawn(async move { write_to_fd_async(fd_clone.as_fd(), &input_clone).await });

    Ok(Some(handle))
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

    if write_handle.is_some() {
        return Err(InteractError::Custom(
            "testcase input too large, async writing not supported yet".to_string(),
        ));
    }

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
