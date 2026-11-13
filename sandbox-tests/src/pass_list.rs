//! The report a demo script prints: one `PASS <check>: <detail>` line per passed check, and a
//! `FAIL <check>: <reason>` error for the first check that fails.

use anyhow::{anyhow, Result};

#[derive(Debug, Default)]
pub struct PassList {
    lines: Vec<String>,
}

impl PassList {
    /// Records `PASS <name>: <detail>` when the check passed. When it failed, records nothing and
    /// returns an error whose text is `FAIL <name>: <reason>`.
    pub fn check(&mut self, name: &str, outcome: Result<String>) -> Result<()> {
        match outcome {
            Ok(detail) => {
                self.lines.push(format!("PASS {name}: {detail}"));
                Ok(())
            }
            Err(reason) => Err(anyhow!("FAIL {name}: {reason:#}")),
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

/// Prints the PASS lines, then the FAIL line when `outcome` failed, and ends the process: exit 0
/// when every check passed, else exit 1. An error that is not from a check is a setup failure.
pub fn print_and_exit(list: &PassList, outcome: Result<()>) -> ! {
    for line in list.lines() {
        println!("{line}");
    }
    match outcome {
        Ok(()) => std::process::exit(0),
        Err(failure) => {
            let text = format!("{failure:#}");
            if text.starts_with("FAIL ") {
                println!("{text}");
            } else {
                println!("FAIL setup: {text}");
            }
            std::process::exit(1)
        }
    }
}
