use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tokio::process::Command;
use tracing::debug;

use super::{
    cli_assessment, package_prompt, AiProvider, Assessment, CliAssessment, ProviderFuture,
    REVIEW_PROMPT,
};

pub(super) struct Claude;

#[derive(Debug, Deserialize)]
struct ClaudeOutput {
    subtype: String,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    errors: Vec<String>,
    structured_output: Option<CliAssessment>,
}

impl AiProvider for Claude {
    fn assess<'a>(
        &'a self,
        model: &'a str,
        package_name: &'a str,
        pkgbuild: &'a str,
        commit_diff: &'a str,
        repository_path: &'a std::path::Path,
    ) -> ProviderFuture<'a> {
        Box::pin(async move {
            let prompt = package_prompt(package_name, pkgbuild, commit_diff);
            debug!(
                model,
                package_name,
                working_directory = %repository_path.display(),
                "starting claude CLI provider"
            );
            let output = command(model, &prompt, repository_path)?
                .output()
                .await
                .context("failed to run `claude`; ensure the Claude Code CLI is installed")?;
            debug!(
                model,
                package_name,
                status = %output.status,
                "claude CLI provider finished"
            );

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("`claude` failed with {}: {}", output.status, stderr.trim());
            }

            let response =
                String::from_utf8(output.stdout).context("Claude output was not UTF-8")?;
            parse_claude_assessment(&response)
        })
    }
}

fn command(model: &str, prompt: &str, repository_path: &std::path::Path) -> Result<Command> {
    let schema = serde_json::to_string(&schemars::schema_for!(CliAssessment))?;
    let mut command = Command::new("claude");
    command
        .current_dir(repository_path)
        .arg("--print")
        .arg("--safe-mode")
        .arg("--no-session-persistence")
        .arg("--no-chrome")
        .arg("--disable-slash-commands")
        .arg("--strict-mcp-config")
        .args(["--permission-mode", "dontAsk"])
        .args(["--tools", "Read,Glob,Grep"])
        .args(["--output-format", "json"])
        .args(["--max-turns", "4"])
        .args(["--model", model])
        .args(["--system-prompt", REVIEW_PROMPT])
        .args(["--json-schema", &schema])
        .arg(prompt);
    Ok(command)
}

fn parse_claude_assessment(response: &str) -> Result<Assessment> {
    let output: ClaudeOutput =
        serde_json::from_str(response.trim()).context("Claude returned an invalid result")?;
    if output.is_error || output.subtype != "success" {
        let detail = if output.errors.is_empty() {
            output.subtype
        } else {
            output.errors.join("; ")
        };
        bail!("Claude failed to produce an assessment: {detail}");
    }
    let wire = output
        .structured_output
        .context("Claude returned no structured assessment")?;
    cli_assessment(wire)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;
    use crate::ai::Verdict;

    #[test]
    fn parses_structured_assessment() {
        let assessment = parse_claude_assessment(
            r#"{"subtype":"success","is_error":false,"structured_output":{"classification":"suspicious","explanation":"changed domain"}}"#,
        )
        .unwrap();
        assert!(matches!(assessment.verdict, Verdict::Suspicious(_)));
    }

    #[test]
    fn rejects_error_or_missing_structured_assessment() {
        assert!(parse_claude_assessment(
            r#"{"subtype":"error_max_structured_output_retries","is_error":true,"errors":["schema mismatch"],"structured_output":null}"#,
        )
        .is_err());
        assert!(parse_claude_assessment(
            r#"{"subtype":"success","is_error":false,"structured_output":null}"#,
        )
        .is_err());
    }

    #[test]
    fn command_is_isolated_and_read_only() {
        let command = command("sonnet", "review", std::path::Path::new("/tmp/repository")).unwrap();
        let command = command.as_std();
        let args = command.get_args().collect::<Vec<_>>();

        assert_eq!(command.get_program(), OsStr::new("claude"));
        assert_eq!(
            command.get_current_dir(),
            Some(std::path::Path::new("/tmp/repository"))
        );
        for flag in [
            "--print",
            "--safe-mode",
            "--no-session-persistence",
            "--no-chrome",
            "--disable-slash-commands",
            "--strict-mcp-config",
        ] {
            assert!(args.contains(&OsStr::new(flag)));
        }
        assert!(args
            .windows(2)
            .any(|args| args == [OsStr::new("--permission-mode"), OsStr::new("dontAsk")]));
        assert!(args
            .windows(2)
            .any(|args| args == [OsStr::new("--tools"), OsStr::new("Read,Glob,Grep")]));
        assert!(!args.iter().any(|arg| {
            ["Bash", "Edit", "Write", "WebFetch", "WebSearch"]
                .iter()
                .any(|tool| arg == tool)
        }));
    }
}
