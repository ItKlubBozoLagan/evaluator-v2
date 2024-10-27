pub mod meta;

use crate::isolate::meta::{ProcessMeta, ProcessStatus};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const ISOLATE_BINARY_LOCATION: &str = "/usr/local/bin/isolate";

// 1 MiB = 256 blocks if block is 4096 bytes
// TODO: dynamic
const MAX_DISK_QUOTA_BLOCKS: u32 = 25600;
const MAX_DISK_QUOTA_INODES: u32 = 10;

const MAX_OPEN_FILES_SYSTEM: u32 = 256;

const MAX_WALL_TIME_LIMIT_SECONDS: f32 = 30.0;

// https://github.com/ioi/isolate/issues/95
const EXTRA_TIME_PERCENT: u8 = 125;

#[derive(thiserror::Error, Debug)]
pub enum IsolateError {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Failed to write stdin into file: {0}")]
    StdinIntoFileError(String),

    #[error("Process is already/still running")]
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

    // special flag used for controlled executions like compilation
    //  currently, this will mount /etc and increase open file limit
    pub system: bool,
}

#[derive(Debug, Clone)]
pub struct IsolateLimits {
    pub time_limit: f32,
    pub memory_limit: u32,
}

pub enum ProcessInput {
    StdIn(Vec<u8>),
    Piped(OwnedFd),
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

    dir_mounts: Vec<String>,

    running_child: Option<IsolateRunningChild>,
}

impl IsolatedProcess {
    // NOTE: maybe use tokio::process::Command if issues arise
    pub fn new(
        execution_id: u8,
        command_meta: &CommandMeta,
        limits: &IsolateLimits,
        dir_mounts: Vec<String>,
    ) -> Result<Self, IsolateError> {
        let mut isolate_command = Command::new(ISOLATE_BINARY_LOCATION);

        isolate_command.arg("-E");
        isolate_command.arg("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");

        isolate_command.arg("--processes");
        isolate_command.arg("--cg");

        isolate_command.arg("--wall-time");
        isolate_command.arg(format!(
            "{}",
            MAX_WALL_TIME_LIMIT_SECONDS.min(limits.time_limit * 2.0)
        ));

        isolate_command.arg("--time");
        isolate_command.arg(format!("{}", limits.time_limit));

        let extra_time = limits.time_limit as f64 * (EXTRA_TIME_PERCENT as f64 / 100.0);

        isolate_command.arg("--extra-time");
        isolate_command.arg(format!("{:.2}", extra_time));

        isolate_command.arg("--cg-mem");
        isolate_command.arg(format!("{}", limits.memory_limit));

        isolate_command.arg("--meta");
        isolate_command.arg(format!("/tmp/.meta-{}", execution_id));

        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", execution_id));

        isolate_command.stderr(Stdio::piped());

        Ok(IsolatedProcess {
            box_id: execution_id,
            command_meta: command_meta.clone(),
            command: isolate_command,
            dir_mounts,
            running_child: None,
        })
    }

    pub fn spawn_with_hooks<F>(
        &mut self,
        input: ProcessInput,
        output_fd: Option<OwnedFd>,
        pre_hook: F,
    ) -> Result<(), IsolateError>
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

        if self.command_meta.system {
            self.command.arg("--open-files");
            self.command.arg(format!("{}", MAX_OPEN_FILES_SYSTEM));
        }

        match input {
            ProcessInput::StdIn(stdin) => {
                Self::write_stdin_to_file(&dir, &stdin)?;

                self.command.arg("--stdin");
                self.command.arg("/box/.stdin");
            }
            ProcessInput::Piped(fd) => {
                self.command.stdin(Stdio::from(fd));
            }
        };

        match output_fd {
            Some(fd) => {
                self.command.stdout(Stdio::from(fd));
            }
            None => {
                self.command.stdout(Stdio::piped());
            }
        }

        for dir_mount in self.dir_mounts.iter() {
            self.command.arg("--dir");
            self.command.arg(dir_mount);
        }

        self.command.arg("--run");
        self.command.arg("--");

        if self.command_meta.in_path {
            self.command.arg(&self.command_meta.executable);
        } else {
            self.command
                .arg(format!("./{}", &self.command_meta.executable));
        }
        self.command.args(&self.command_meta.args);

        let child = self.command.spawn()?;

        self.running_child = Some(IsolateRunningChild {
            child: Some(child),
            work_dir: dir,
        });

        Ok(())
    }

    pub fn spawn(
        &mut self,
        input: ProcessInput,
        output_fd: Option<OwnedFd>,
    ) -> Result<(), IsolateError> {
        self.spawn_with_hooks(input, output_fd, |_| Ok(()))
    }

    fn spawn_init(&mut self) -> Result<PathBuf, IsolateError> {
        let mut isolate_command = Command::new(ISOLATE_BINARY_LOCATION);

        isolate_command.arg("--box-id");
        isolate_command.arg(format!("{}", self.box_id));

        isolate_command.arg("--quota");
        isolate_command.arg(format!(
            "{},{}",
            MAX_DISK_QUOTA_BLOCKS, MAX_DISK_QUOTA_INODES
        ));

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

        std::fs::remove_file(format!("/tmp/.meta-{}", self.box_id))?;

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

    pub fn load_meta(&self) -> Result<ProcessMeta, IsolateError> {
        let Some(child) = &self.running_child else {
            return Err(IsolateError::ProcessNotRunning);
        };

        // TODO: maybe replace with some state property
        // child will be none if the output has been consumed somehow (i.e. process is finished)
        if child.child.is_some() {
            return Err(IsolateError::ProcessRunning);
        };

        let meta_file_content = std::fs::read_to_string(format!("/tmp/.meta-{}", self.box_id))?;

        let meta = Self::parse_meta(&meta_file_content)?;

        Ok(meta)
    }

    pub fn parse_meta(meta_content: &str) -> Result<ProcessMeta, IsolateError> {
        let key_value: HashMap<String, String> = meta_content
            .lines()
            .map(|line| line.split(':').map(String::from).collect::<Vec<String>>())
            .filter(|kv| kv.len() == 2)
            .map(|kv| (kv[0].clone(), kv[1].clone()))
            .collect();

        let meta = ProcessMeta {
            cg_mem_kb: key_value
                .get("cg-mem")
                .and_then(|val| val.parse::<u32>().ok())
                .unwrap_or(0),
            status: key_value
                .get("status")
                .and_then(|it| TryInto::<ProcessStatus>::try_into(it).ok()),
            time_ms: key_value
                .get("time")
                .and_then(|val| val.parse::<f64>().ok())
                .map(|val| (val * 1000.0) as u32)
                .unwrap_or(0),
            cg_oom_killed: key_value
                .get("cg-oom-killed")
                .map(|val| val == "1")
                .unwrap_or(false),
        };

        Ok(meta)
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

impl Drop for IsolatedProcess {
    fn drop(&mut self) {
        if self.running_child.is_some() {
            let _ = self.cleanup_and_reset();
        }
    }
}
