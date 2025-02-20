use lazy_static::lazy_static;
use rand::RngCore;
use std::fmt::Write;
use std::fs;

lazy_static! {
    pub static ref ETC_JAVA_DIRECTORIES: Vec<String> = get_etc_java_directories();
}

pub fn random_bytes(n: u32) -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = vec![0u8; n as usize];
    rng.fill_bytes(&mut bytes);
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

// ugh, java
fn get_etc_java_directories() -> Vec<String> {
    let etc_dir_names = fs::read_dir("/etc")
        .map(|res| {
            res.into_iter()
                .filter_map(|entry| entry.ok().map(|dir_entry| dir_entry.path()))
                .filter(|entry| entry.is_dir())
                .map(|path| path.display().to_string())
                .collect()
        })
        .unwrap_or_else(|_| vec![]);

    let java_dir_names: Vec<String> = etc_dir_names
        .into_iter()
        .filter(|path| path.starts_with("/etc/java"))
        .collect();

    java_dir_names
}
