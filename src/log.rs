use std::fs::OpenOptions;
use std::io::Write;

pub struct Debug;

impl Debug {
    #[allow(unused)]
    pub fn log<S: AsRef<str>>(name: &str, msg: S) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/calyx-lsp-debug/{name}.log"))
            .unwrap();
        writeln!(file, "{}", msg.as_ref()).expect("Unable to write file");
    }

    #[allow(unused)]
    pub fn stdout<S: AsRef<str>>(msg: S) {
        Self::log("stdout", msg)
    }

    #[allow(unused)]
    pub fn update<S: AsRef<str>>(name: &str, msg: S) {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(format!("/tmp/calyx-lsp-debug/{name}.log"))
            .unwrap();
        writeln!(file, "{}", msg.as_ref()).expect("Unable to write file");
    }
}
