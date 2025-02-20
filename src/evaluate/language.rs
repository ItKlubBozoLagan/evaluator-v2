use crate::messages::EvaluationLanguage;
use crate::util;

impl EvaluationLanguage {
    // get compiler command and arguments based on language
    pub fn get_compiler_command(
        &self,
        out_file: &str,
    ) -> Option<(&'static str, Vec<String>, Vec<String>)> {
        type E = EvaluationLanguage;
        match self {
            E::C => Some((
                "/usr/bin/gcc",
                vec![
                    "-std=c11", "-x", "c", "-O2", "-static", "-Wall", "-o", out_file, "-", "-lm",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![],
            )),
            E::Cpp => Some((
                "/usr/bin/g++",
                vec![
                    "-std=c++17",
                    "-x",
                    "c++",
                    "-O2",
                    "-static",
                    "-Wall",
                    "-o",
                    out_file,
                    "-",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![],
            )),
            E::Rust => Some((
                "/usr/bin/rustc",
                vec![
                    "-C",
                    "opt-level=2",
                    "-C",
                    "target-feature=+crt-static",
                    "-o",
                    out_file,
                    "-",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![],
            )),
            E::Go => Some((
                "/usr/bin/bash",
                vec![
                    "-c",
                    &format!("cp /dev/stdin source.go && GOCACHE=/tmp/.gocache /usr/bin/go build -o {out_file} source.go && rm source.go"),
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![],
            )),
            E::Java => {
                Some((
                    "/usr/bin/bash",
                    vec![
                        "-c",
                        &format!("cp /dev/stdin source.java && /usr/bin/javac source.java && mv Main.class {out_file}"),
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                    util::general::ETC_JAVA_DIRECTORIES.clone(),
                ))
            },
            E::GnuAsmX86Linux => {
                Some((
                    "/usr/bin/gcc",
                    vec![
                        "-x", "assembler", "-static", "-nostdlib", "-no-pie", "-o", out_file, "-",
                    ]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                    vec![],
                ))
            }
            E::Python => None,
        }
    }
}
