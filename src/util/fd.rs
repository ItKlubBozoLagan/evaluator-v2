use crate::environment::Environment;
use std::cmp::min;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use thiserror::Error;
use tracing::{debug, warn};

#[derive(Error, Debug)]
pub enum SafeFdWriteError {
    #[error("syscall error: {0}")]
    NixError(#[from] nix::Error),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
}

#[derive(Debug)]
pub enum WriteHandle {
    Ignored,
    Direct,
    Async(Option<std::thread::JoinHandle<()>>),
}

#[derive(Debug)]
pub enum LargeWriteStrategy {
    Async,
    Ignore,
}

impl Drop for WriteHandle {
    fn drop(&mut self) {
        if let WriteHandle::Async(handle) = self
            && let Some(handle) = handle.take()
        {
            let _ = handle.join();
        }
    }
}

pub fn write_to_fd_safe(
    fd: BorrowedFd,
    input: &[u8],
    strategy: LargeWriteStrategy,
) -> Result<WriteHandle, SafeFdWriteError> {
    let current_pipe_buf_size =
        nix::fcntl::fcntl(fd.as_raw_fd(), nix::fcntl::FcntlArg::F_GETPIPE_SZ)?;

    let input_size = input.len();

    if input_size < (current_pipe_buf_size as usize) {
        write_all_fd(fd, input)?;

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
        write_all_fd(fd, input)?;

        return Ok(WriteHandle::Direct);
    }

    match strategy {
        LargeWriteStrategy::Async => {
            let fd = fd.try_clone_to_owned()?;
            let input = input.to_vec();
            let handle = std::thread::spawn(move || {
                if let Err(err) = write_all_fd(fd.as_fd(), &input) {
                    warn!("failed to write to interactive pipe: {err}");
                }
            });
            Ok(WriteHandle::Async(Some(handle)))
        }
        LargeWriteStrategy::Ignore => Ok(WriteHandle::Ignored),
    }
}

fn write_all_fd(fd: BorrowedFd<'_>, mut input: &[u8]) -> Result<(), SafeFdWriteError> {
    while !input.is_empty() {
        match nix::unistd::write(fd, input) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "pipe write returned zero bytes",
                )
                .into());
            }
            Ok(written) => input = &input[written..],
            Err(nix::errno::Errno::EINTR) => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}
