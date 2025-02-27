use crate::messages::EvaluationLanguage;
use crate::util;

// argh, formatting doesn't work when the format! is called directly where it's used
//  and format! doesn't support const strings as format strings
#[inline]
fn make_go_compile_script(out_file: &str) -> String {
    format!(
        "cat > source.go && GOCACHE=/tmp/.gocache GOFLAGS=\"-count=1\" /usr/bin/go build -o {} source.go && rm source.go",
        out_file
    )
}

#[inline]
fn make_java_compile_script(out_file: &str) -> String {
    format!(
        "cat > source.java && /usr/bin/javac -Xlint:all source.java && mv Main.class {}",
        out_file
    )
}

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
                    "-std=c11",
                    "-fdiagnostics-color=always",
                    "-x",
                    "c",
                    "-O2",
                    "-static",
                    "-Wall",
                    "-o",
                    out_file,
                    "-",
                    "-lm",
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
                    "-fdiagnostics-color=always",
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
                vec!["-c", &make_go_compile_script(out_file)]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                vec![],
            )),
            E::Java => Some((
                "/usr/bin/bash",
                vec!["-c", &make_java_compile_script(out_file)]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                util::general::ETC_JAVA_DIRECTORIES.clone(),
            )),
            E::GnuAsmX86Linux => Some((
                "/usr/bin/gcc",
                vec![
                    "-fdiagnostics-color=always",
                    "-x",
                    "assembler",
                    "-static",
                    "-nostdlib",
                    "-no-pie",
                    "-o",
                    out_file,
                    "-",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![],
            )),
            E::Python => None,
        }
    }
}
