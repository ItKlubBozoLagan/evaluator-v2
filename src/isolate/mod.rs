pub mod meta;

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const ISOLATE_BINARY_LOCATION: &str = "/usr/local/bin/isolate";

// 1 MiB = 256 blocks if block is 4096 bytes
// TODO: dynamic
const MAX_DISK_QUOTA_BLOCKS: u32 = 25600;
const MAX_DISK_QUOTA_INODES: u32 = 10;

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

    #[error("Isolate init failed: {0}")]
    InitFailed(String),
}

#[derive(Debug, Clone)]
pub struct CommandMeta {
    pub executable: String,
    pub args: Vec<String>,
    pub in_path: bool,
}

pub struct IsolateRunningChild {
    // child can be taken
    child: Option<Child>,
    work_dir: PathBuf,
}

pub struct IsolatedProcess {
    box_id: u8,

    command_meta: CommandMeta,
    command: Command,

    running_child: Option<IsolateRunningChild>,
}

impl IsolatedProcess {
    // NOTE: maybe use tokio::process::Command if issues arise
    pub fn new(execution_id: u8, command_meta: &CommandMeta) -> Result<Self, IsolateError> {
        let mut isolate_command = Command::new(ISOLATE_BINARY_LOCATION);

        isolate_command.arg("-E");
        isolate_command.arg("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");

        isolate_command.arg("--processes");
        isolate_command.arg("--cg");

        isolate_command.arg("--stdin");
        isolate_command.arg("/box/.stdin");

        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", execution_id));

        isolate_command.stdout(Stdio::piped());
        isolate_command.stderr(Stdio::piped());

        Ok(IsolatedProcess {
            box_id: execution_id,
            command_meta: command_meta.clone(),
            command: isolate_command,
            running_child: None,
        })
    }

    pub fn spawn_with_hooks<F>(&mut self, stdin: &[u8], pre_hook: F) -> Result<(), IsolateError>
    where
        F: Fn(&mut IsolatedProcess) -> Result<(), IsolateError>,
    {
        if self.running_child.is_some() {
            return Err(IsolateError::ProcessRunning);
        }

        let dir = self.spawn_init()?;
        let dir = dir.join("box");

        self.running_child = Some(IsolateRunningChild {
            child: None,
            work_dir: dir.clone(),
        });

        pre_hook(self)?;

        Self::write_stdin_to_file(&dir, stdin)?;

        self.command.arg("--run");
        self.command.arg("--");

        if self.command_meta.in_path {
            self.command.arg(&self.command_meta.executable);
        } else {
            self.command
                .arg(format!("./{}", &self.command_meta.executable));
        }
        self.command.args(&self.command_meta.args);

        dbg!(&self.command);

        let child = self.command.spawn()?;

        self.running_child = Some(IsolateRunningChild {
            child: Some(child),
            work_dir: dir,
        });

        Ok(())
    }

    pub fn spawn(&mut self, stdin: &[u8]) -> Result<(), IsolateError> {
        self.spawn_with_hooks(stdin, |_| Ok(()))
    }

    fn spawn_init(&mut self) -> Result<PathBuf, IsolateError> {
        let mut isolate_command = Command::new(ISOLATE_BINARY_LOCATION);

        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", self.box_id));

        // TODO:
        // isolate_command.arg("--quota");
        // isolate_command.arg(format!(
        //     "{},{}",
        //     MAX_DISK_QUOTA_BLOCKS, MAX_DISK_QUOTA_INODES
        // ));

        isolate_command.arg("--cg");

        isolate_command.arg("--init");

        isolate_command.stdout(Stdio::piped());
        isolate_command.stderr(Stdio::piped());

        let out = isolate_command.spawn()?.wait_with_output()?;

        if !out.status.success() {
            return Err(IsolateError::InitFailed(
                String::from_utf8_lossy(&out.stderr).to_string(),
            ));
        }

        let out = PathBuf::from(&String::from_utf8_lossy(&out.stdout).trim());

        if !out.exists() {
            return Err(IsolateError::InitFailed(
                "box directory doesn't exist".to_string(),
            ));
        }

        Ok(out)
    }

    pub fn cleanup_and_reset(&mut self) -> Result<(), IsolateError> {
        let mut isolate_command = Command::new(ISOLATE_BINARY_LOCATION);

        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", self.box_id));

        isolate_command.arg("--cg");

        isolate_command.arg("--cleanup");

        isolate_command.stdout(Stdio::piped());
        isolate_command.stderr(Stdio::piped());

        let child = isolate_command.spawn()?;

        child.wait_with_output()?;

        self.running_child = None;
        Ok(())
    }

    pub fn move_out_of_box(&mut self, path: &str, out_file: &Path) -> Result<(), IsolateError> {
        let Some(running) = &self.running_child else {
            return Err(IsolateError::ProcessNotRunning);
        };

        let path_in_box = running.work_dir.join(path);

        std::fs::copy(path_in_box, out_file)?;

        Ok(())
    }

    pub fn copy_in_box(&mut self, in_file: &Path, path: &str) -> Result<(), IsolateError> {
        let Some(running) = &self.running_child else {
            return Err(IsolateError::ProcessNotRunning);
        };

        let path_in_box = running.work_dir.join(path);

        std::fs::copy(in_file, path_in_box)?;

        Ok(())
    }

    pub fn wait_for_output(&mut self) -> Result<std::process::Output, IsolateError> {
        let mut child = self
            .running_child
            .take()
            .ok_or_else(|| IsolateError::ProcessNotRunning)?;

        let child_process = child
            .child
            .take()
            .ok_or_else(|| IsolateError::ProcessNotRunning)?;

        let output = child_process.wait_with_output()?;

        // re-set because of take
        self.running_child = Some(IsolateRunningChild {
            child: None,
            work_dir: child.work_dir,
        });

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
