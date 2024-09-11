use std::process::Command;

// NOTE: maybe use tokio::process::Command if issues arise
pub fn wrap_isolate(command: (&str, &[String]), extra_dirs: Option<&[String]>) -> Command {
    let mut isolate_command = Command::new("/home/visen/isolate/isolate");
    if let Some(dirs) = extra_dirs {
        for dir in dirs {
            isolate_command.arg("--dir").arg(dir);
        }
    }
    isolate_command.arg("--run");
    isolate_command.arg("--");
    isolate_command.arg(command.0);
    isolate_command.args(command.1);
    isolate_command
}