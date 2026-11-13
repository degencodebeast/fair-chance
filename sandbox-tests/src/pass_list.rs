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
