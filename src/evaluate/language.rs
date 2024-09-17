use crate::messages::EvaluationLanguage;

impl EvaluationLanguage {
    // get compiler command and arguments based on language
    pub fn get_compiler_command(&self, out_file: &str) -> Option<(&'static str, Vec<String>)> {
        let out_file = out_file.to_string();

        type E = EvaluationLanguage;
        match self {
            E::C => Some((
                "/usr/bin/gcc",
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
                "/usr/bin/g++",
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
            E::Java => Some(("/usr/bin/javac", vec![])),
            E::Go => Some((
                "/usr/bin/go",
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
