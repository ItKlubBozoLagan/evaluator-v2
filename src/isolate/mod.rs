use crate::util::random_bytes;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(thiserror::Error, Debug)]
pub enum IsolateError {
    #[error("Failed to write stdin into file: {0}")]
    StdinIntoFileError(String)
}

// NOTE: maybe use tokio::process::Command if issues arise
pub fn wrap_isolate(command: (&str, &[String]), extra_dirs: Option<&[String]>, stdin: &[u8]) -> Result<Command, IsolateError> {
    let stdin_dir = write_stdin_to_file(stdin)?.display().to_string();
    let stdin_file_name =format!("{}/.stdin", stdin_dir);

    let mut isolate_command = Command::new("/home/visen/isolate/isolate");
    if let Some(dirs) = extra_dirs {
        for dir in dirs {
            isolate_command.arg("--dir").arg(dir);
        }
    }
    isolate_command.arg("--dir");
    isolate_command.arg(stdin_dir);
    isolate_command.arg("--stdin");
    isolate_command.arg(stdin_file_name);
    isolate_command.arg("--run");
    isolate_command.arg("--");
    isolate_command.arg(command.0);
    isolate_command.args(command.1);

    dbg!(&isolate_command);
    Ok(isolate_command)
}

fn write_stdin_to_file(stdin: &[u8]) -> Result<PathBuf, IsolateError> {
    let mut dir: PathBuf = PathBuf::new();

    // TODO: cleanup
    loop {
        let dir_location = format!("/tmp/kontestis-{}", random_bytes(16));
        let local_dir = Path::new(&dir_location);
        if !local_dir.exists() {
            dir = local_dir.to_path_buf();
            break;
        }
    }

    std::fs::create_dir_all(&dir).map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

    let mut file = File::create(Path::join(&dir, ".stdin")).map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

    file.write_all(stdin).map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

    Ok(dir)
}