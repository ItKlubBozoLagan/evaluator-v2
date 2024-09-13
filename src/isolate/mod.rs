use crate::util::random_bytes;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(thiserror::Error, Debug)]
pub enum IsolateError {
    #[error("Failed to write stdin into file: {0}")]
    StdinIntoFileError(String),
}

// NOTE: maybe use tokio::process::Command if issues arise
pub fn wrap_isolate(
    work_dir: &PathBuf,
    command: (&str, &[String]),
    extra_dirs: Option<&[String]>,
    stdin: &[u8],
) -> Result<Command, IsolateError> {
    write_stdin_to_file(work_dir, stdin)?;
    let stdin_file_name = format!("{}/.stdin", work_dir.display());

    let mut isolate_command = Command::new("/usr/bin/isolate");
    if let Some(dirs) = extra_dirs {
        for dir in dirs {
            isolate_command.arg("--dir").arg(dir);
        }
    }
    isolate_command.arg("--dir");
    isolate_command.arg(work_dir);

    isolate_command.arg("-E");
    isolate_command.arg("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");

    isolate_command.arg("--stdin");
    isolate_command.arg(stdin_file_name);

    isolate_command.arg("--run");
    isolate_command.arg("--");
    isolate_command.arg(format!("{}/{}", work_dir.display(), command.0));
    isolate_command.args(command.1);

    dbg!(&isolate_command);
    Ok(isolate_command)
}

// TODO: extract into separate file
pub fn make_program_work_dir() -> std::io::Result<PathBuf> {
    let dir = loop {
        let dir_location = format!("/tmp/kontestis-{}", random_bytes(16));
        let local_dir = Path::new(&dir_location);
        if !local_dir.exists() {
            break local_dir.to_path_buf();
        }
    };

    std::fs::create_dir_all(&dir)?;

    Ok(dir)
}

fn write_stdin_to_file(dir: &PathBuf, stdin: &[u8]) -> Result<(), IsolateError> {
    // TODO: cleanup
    let mut file = File::create(Path::join(dir, ".stdin"))
        .map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

    file.write_all(stdin)
        .map_err(|err| IsolateError::StdinIntoFileError(err.to_string()))?;

    Ok(())
}
