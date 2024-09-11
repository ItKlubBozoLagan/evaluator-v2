use std::io::Write;
use std::process::{Child, Command, ExitStatus, Output};
use libc::{execv, fork};
use rand::RngCore;
use serde::__private::de::IdentifierDeserializer;
use crate::messages::{Evaluation, EvaluationLanguage};

#[derive!(Debug)]
struct EvaluationResult {
    compile_success: bool,
    compile_error: Option<String>,
    successful_evaluation: Option<SuccessfulEvaluation>
}

#[derive!(Debug)]
struct SuccessfulEvaluation {
    verdict: Verdict,
    max_time: u32,
    max_memory: u32,
    testcases: Vec<TestcaseResult>
}

#[derive!(Debug)]
struct TestcaseResult {
    id: u32,
    verdict: Verdict,
    time: u32,
    memory: u32,
    error: Option<String>
}

pub enum Verdict {
    Accepted,
    WrongAnswer,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    CompilationError,
    JudgingError,
    SystemError,
    Skipped
}

pub fn begin_evaluation(evaluation: Evaluation) {
    println!("Received evaluation: {:#?}", evaluation);


    let compilation_result = compile(evaluation.code.as_str(), evaluation.language);


}

type SpawnProcess = fn () -> Child;

#[derive(Debug)]
struct CompilationResult {
    success: bool,
    spawn_process: Option<SpawnProcess>,
    error: Option<String>
}

pub fn random_bytes(n: u32) -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = vec![0u8; n as usize];
    rng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}


// Get compiler command and arguments based on language
fn get_compiler_command(language: EvaluationLanguage, out_file: String) -> (String, Vec<String>) {
    match language {
        EvaluationLanguage::C => ("gcc".to_string(), vec!["-x".to_string(), "c".to_string(), "-O3".to_string(), "-Wall".to_string(), "-o".to_string(), out_file, "-".to_string()]),
        EvaluationLanguage::Cpp => ("g++".to_string(), vec!["-x".to_string(), "c++".to_string(), "-O3".to_string(), "-Wall".to_string(), "-o".to_string(), out_file, "-".to_string()]),
        EvaluationLanguage::Rust => ("rustc".to_string(), vec!["-o".to_string(), out_file, "-".to_string()]),
        EvaluationLanguage::Java => ("javac".to_string(), vec![]),
        EvaluationLanguage::Go => ("go".to_string(), vec!["build".to_string(), "-o".to_string(), out_file, "-".to_string()]),
        _ => panic!("Unsupported language")
    }
}
pub fn compile(code: &str, language: EvaluationLanguage) -> Result<CompilationResult, std::io::Error> {

    let output_file = random_bytes(32);

    let (compiler, args) = get_compiler_command(language, output_file.clone());

    let mut child = Command::new(compiler)
                .args(args)
                .stdin(std::process::Stdio::piped())
                .spawn()?;

    let mut child_stdin = child.stdin.take().unwrap();
    child_stdin.write_all(code.as_bytes())?;
    drop(child_stdin);

    let output = child.wait_with_output()?;

    let pid = unsafe {
        fork()
    };

    if pid == 0 {
        // Child process
        println!("Child process");

        unsafe {
            execv("/bin/ls".into().as_ptr(), ());
        }
    } else {
        // Parent process
        println!("Parent process");
    }


    if output.status.success() {
        Ok(CompilationResult {
            success: true,
            spawn_process: Some(|| {
                Command::new(&output_file)
                    .spawn()
                    .unwrap()
            }),
            error: None
        })
    } else {
        Ok(CompilationResult {
            success: false,
            spawn_process: None,
            error: Some(String::from_utf8_lossy(&output.stderr).to_string())
        })
    }

}

pub fn process_compilation_step(code: &str, language: EvaluationLanguage) -> CompilationResult {

    match language {
        EvaluationLanguage::Python => CompilationResult {
            success: true,
            spawn_process: Some(|| {
                Command::new("python")
                    .args(&["-c", code])
                    .spawn()
                    .unwrap()
            }),
            error: None
        },
        _ => compile(code, language).unwrap()
    }
}