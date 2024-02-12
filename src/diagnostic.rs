use std::{path::PathBuf, process::Command};

use serde::Deserialize;

use crate::log;

pub struct Diagnostic;

#[derive(Deserialize, Debug)]
pub struct CalyxError {
    #[allow(unused)]
    pub file_name: String,
    pub pos_start: usize,
    pub pos_end: usize,
    pub msg: String,
}

impl Diagnostic {
    pub fn did_save(path: &PathBuf) -> Vec<CalyxError> {
        let output = Command::new("calyx")
            .arg(path.to_str().unwrap())
            .args(["-l", "/Users/sgt/Research/calyx"])
            .args(["-p", "none"])
            .arg("--json-error")
            .output()
            .expect("Failed to run command");
        serde_json::from_slice(&output.stdout)
            .map(|e| vec![e])
            .unwrap_or(vec![])
    }
}
