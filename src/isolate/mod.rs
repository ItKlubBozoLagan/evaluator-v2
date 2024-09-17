pub mod meta;

use crate::util::random_bytes;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

// 1 MiB = 256 blocks if block is 4096 bytes
// TODO: dynamic
const MAX_DISK_QUOTA_BLOCKS: u32 = 25600;

#[derive(thiserror::Error, Debug)]
pub enum IsolateError {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Failed to write stdin into file: {0}")]
    StdinIntoFileError(String),

    #[error("Process is already running")]
    ProcessRunning,

    #[error("Process is not running")]
    ProcessNotRunning,
}

pub struct IsolatedProcess {
    box_id: u8,

    work_dir: PathBuf,
    command: Command,

    running_child: Option<Child>,
}

impl IsolatedProcess {
    // NOTE: maybe use tokio::process::Command if issues arise
    pub fn new(
        work_dir: &Path,
        command: (&str, &[String]),
        is_global_binary: bool,
        extra_dirs: Option<&[String]>,
    ) -> Result<Self, IsolateError> {
        let mut isolate_command = Command::new("/usr/bin/isolate");
        if let Some(dirs) = extra_dirs {
            for dir in dirs {
                isolate_command.arg("--dir").arg(dir);
            }
        }
        isolate_command.arg("--dir");
        isolate_command.arg(format!("{}:rw", work_dir.display()));

        isolate_command.arg("-E");
        isolate_command.arg("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");

        isolate_command.arg("--processes");

        isolate_command.arg("--run");
        isolate_command.arg("--");
        if is_global_binary {
            isolate_command.arg(command.0);
        } else {
            isolate_command.arg(format!("{}/{}", work_dir.display(), command.0));
        }
        isolate_command.args(command.1);

        isolate_command.stdout(Stdio::piped());
        isolate_command.stderr(Stdio::piped());

        dbg!(&isolate_command);
        Ok(IsolatedProcess {
            // TODO: parallel isolate
            box_id: 0,
            work_dir: work_dir.to_path_buf(),
            command: isolate_command,
            running_child: None,
        })
    }

    pub fn spawn(&mut self, stdin: &[u8]) -> Result<(), IsolateError> {
        self.spawn_init()?;

        if self.running_child.is_some() {
            return Err(IsolateError::ProcessRunning);
        }

        write_stdin_to_file(&self.work_dir, stdin)?;
        let stdin_file_name = format!("{}/.stdin", &self.work_dir.display());

        self.command.arg("--stdin");
        self.command.arg(stdin_file_name);

        let child = self.command.spawn()?;
        self.running_child = Some(child);

        Ok(())
    }

    fn spawn_init(&mut self) -> Result<(), IsolateError> {
        let mut isolate_command = Command::new("/usr/bin/isolate");
        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", self.box_id));
        isolate_command.arg("--quota");
        isolate_command.arg(format!("{}", MAX_DISK_QUOTA_BLOCKS));
        isolate_command.arg("--init");
        Ok(isolate_command.spawn().map(|_| ())?)
    }

    fn spawn_cleanup(&mut self) -> Result<(), IsolateError> {
        let mut isolate_command = Command::new("/usr/bin/isolate");
        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", self.box_id));
        isolate_command.arg("--cleanup");
        Ok(isolate_command.spawn().map(|_| ())?)
    }

    pub fn wait_for_output(&mut self) -> Result<std::process::Output, IsolateError> {
        let child = self
            .running_child
            .take()
            .ok_or_else(|| IsolateError::ProcessNotRunning)?;
        let output = child.wait_with_output()?;

        self.spawn_cleanup()?;
        self.running_child = None;
        Ok(output)
    }
}

// TODO: extract into separate file
pub fn make_program_work_dir() -> std::io::Result<PathBuf> {
    let dir = loop {
        let dir_location = format!("/dev/shm/kontestis-{}", random_bytes(16));
        let local_dir = Path::new(&dir_location);
        if !local_dir.exists() {
            break local_dir.to_path_buf();
        }
    };

    std::fs::create_dir_all(&dir)?;

    std::os::unix::fs::chown(&dir, Some(60000), Some(60000))?;

    Ok(dir)
}

fn write_stdin_to_file(dir: &Path, stdin: &[u8]) -> Result<(), IsolateError> {
    // TODO: cleanup
    let mut file = File::create(dir.join(".stdin"))
        .map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

    file.write_all(stdin)
        .map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

    Ok(())
}
