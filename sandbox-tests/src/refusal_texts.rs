//! Plain text for each refusal code, from `refusal-texts/refusal-texts.json` (the same file the
//! page and the watcher read).

use anyhow::{bail, Context, Result};
use serde_json::Value;

const REFUSAL_TEXTS: &str = include_str!("../../refusal-texts/refusal-texts.json");

/// The plain text for one refusal code. An error for a code the file does not name.
pub fn plain_text(code: &str) -> Result<String> {
    let texts: Value = serde_json::from_str(REFUSAL_TEXTS).context("refusal-texts.json")?;
    texts[code]
        .as_str()
        .map(str::to_string)
        .with_context(|| format!("refusal-texts.json has no text for {code}"))
}

/// Checks that a failure text names `code`, and returns `<code> (<plain text>)`.
pub fn refused_with(failure: &str, code: &str) -> Result<String> {
    let text = plain_text(code)?;
    if !failure.contains(code) {
        bail!("expected a refusal with {code}, got: {failure}");
    }
    Ok(format!("{code} ({text})"))
}
