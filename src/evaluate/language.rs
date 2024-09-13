use crate::messages::EvaluationLanguage;

impl EvaluationLanguage {
    // Get compiler command and arguments based on language
    pub fn get_compiler_command(&self, out_file: String) -> Option<(&'static str, Vec<String>)> {
        type E = EvaluationLanguage;
        match self {
            E::C => Some((
                "gcc",
                vec![
                    "-x".to_string(),
                    "c".to_string(),
                    "-O3".to_string(),
                    "-Wall".to_string(),
                    "-o".to_string(),
                    out_file,
                    "-".to_string(),
                ],
            )),
            E::Cpp => Some((
                "g++",
                vec![
                    "-x".to_string(),
                    "c++".to_string(),
                    "-O3".to_string(),
                    "-Wall".to_string(),
                    "-o".to_string(),
                    out_file,
                    "-".to_string(),
                ],
            )),
            E::Rust => Some(("rustc", vec!["-o".to_string(), out_file, "-".to_string()])),
            E::Java => Some(("javac", vec![])),
            E::Go => Some((
                "go",
                vec![
                    "build".to_string(),
                    "-o".to_string(),
                    out_file,
                    "-".to_string(),
                ],
            )),
            _ => None,
        }
    }
}
