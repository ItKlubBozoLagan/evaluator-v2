use tokio::process::Command;
pub fn wrap_isolate(command: Vec<String>, extra_dirs: Vec<String>) -> Command {
    let mut isolate_command = Command::new("/home/visen/isolate/isolate");
    for dir in extra_dirs {
        isolate_command.arg("--dir").arg(dir);
    }
    isolate_command.arg("--run");
    isolate_command.args(command);
    isolate_command
}