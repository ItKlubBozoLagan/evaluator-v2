use crate::environment::Environment;
use std::cmp::min;
use std::os::fd::{AsRawFd, BorrowedFd};
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
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

pub fn write_to_fd_safe(fd: BorrowedFd, input: &[u8]) -> Result<WriteHandle, SafeFdWriteError> {
    let current_pipe_buf_size =
        nix::fcntl::fcntl(fd.as_raw_fd(), nix::fcntl::FcntlArg::F_GETPIPE_SZ)?;

    let input_size = input.len();

    if input_size < (current_pipe_buf_size as usize) {
        nix::unistd::write(fd, input)?;

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

        return Ok(WriteHandle::Direct);
    }

    let fd_clone = fd.try_clone_to_owned()?;
    let input_clone = input.to_vec();
    let handle = Handle::current().spawn(async move {
        if let Err(err) = nix::unistd::write(&fd_clone, &input_clone) {
            warn!("failed to async write to pipe: {}", err);
        };
    });

    Ok(WriteHandle::Async(handle))
}
