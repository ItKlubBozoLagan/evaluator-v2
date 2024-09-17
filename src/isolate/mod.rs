pub mod meta;

use std::borrow::Borrow;
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

    #[error("Isolate init failed")]
    InitFailed,
}

pub struct CommandMeta {
    pub executable: String,
    pub args: Vec<String>,
    pub in_path: bool,
}

pub struct IsolatedProcess {
    box_id: u8,

    command: Command,

    running_child: Option<Child>,
}

impl IsolatedProcess {
    // NOTE: maybe use tokio::process::Command if issues arise
    pub fn new(execution_id: u8, command: impl Borrow<CommandMeta>) -> Result<Self, IsolateError> {
        let command = command.borrow();

        let mut isolate_command = Command::new("/usr/bin/isolate");

        isolate_command.arg("-E");
        isolate_command.arg("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");

        isolate_command.arg("--processes");
        isolate_command.arg("--cg");

        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", execution_id));

        isolate_command.arg("--run");
        isolate_command.arg("--");
        if command.in_path {
            isolate_command.arg(&command.executable);
        } else {
            isolate_command.arg(format!("./{}", &command.executable));
        }
        isolate_command.args(&command.args);

        isolate_command.stdout(Stdio::piped());
        isolate_command.stderr(Stdio::piped());

        dbg!(&isolate_command);
        Ok(IsolatedProcess {
            box_id: execution_id,
            command: isolate_command,
            running_child: None,
        })
    }

    pub fn spawn(&mut self, stdin: &[u8]) -> Result<(), IsolateError> {
        let dir = self.spawn_init()?;

        if self.running_child.is_some() {
            return Err(IsolateError::ProcessRunning);
        }

        Self::write_stdin_to_file(&dir, stdin)?;
        let stdin_file_name = format!("{}/.stdin", &dir.display());

        self.command.arg("--stdin");
        self.command.arg(stdin_file_name);

        let child = self.command.spawn()?;
        self.running_child = Some(child);

        Ok(())
    }

    fn spawn_init(&mut self) -> Result<PathBuf, IsolateError> {
        let mut isolate_command = Command::new("/usr/bin/isolate");

        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", self.box_id));

        isolate_command.arg("--quota");
        isolate_command.arg(format!("{}", MAX_DISK_QUOTA_BLOCKS));

        isolate_command.arg("--cg");

        isolate_command.arg("--init");

        let out = isolate_command.spawn()?.wait_with_output()?;

        if !out.status.success() {
            return Err(IsolateError::InitFailed);
        }

        let out = PathBuf::from(&String::from_utf8_lossy(&out.stdout).trim());

        if !out.exists() {
            return Err(IsolateError::InitFailed);
        }

        Ok(out)
    }

    pub fn cleanup_and_reset(&mut self) -> Result<(), IsolateError> {
        let mut isolate_command = Command::new("/usr/bin/isolate");

        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", self.box_id));

        isolate_command.arg("--cg");

        isolate_command.arg("--cleanup");

        let child = isolate_command.spawn()?;

        child.wait_with_output()?;

        self.running_child = None;
        Ok(())
    }

    pub fn wait_for_output(&mut self) -> Result<std::process::Output, IsolateError> {
        let child = self
            .running_child
            .take()
            .ok_or_else(|| IsolateError::ProcessNotRunning)?;
        let output = child.wait_with_output()?;

        Ok(output)
    }

    fn write_stdin_to_file(dir: &Path, stdin: &[u8]) -> Result<(), IsolateError> {
        let mut file = File::create(dir.join(".stdin"))
            .map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

        file.write_all(stdin)
            .map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

        Ok(())
    }
}
