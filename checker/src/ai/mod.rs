use std::{future::Future, path::Path, pin::Pin};

use anyhow::{bail, Context, Result};
use rig_core::{
    agent::{AgentHook, Flow, HookContext, StepEvent},
    completion::CompletionModel,
    schemars::JsonSchema,
};
use serde::Deserialize;
use tracing::debug;

mod anthropic;
mod claude;
mod codex;
mod openai;
mod openrouter;
mod read_file;

pub(super) const REVIEW_PROMPT: &str = r#"You are reviewing an Arch User Repository PKGBUILD for malware and supply-chain risk.

Return exactly one verdict.
Use Safe for ordinary packaging behavior. Use Suspicious when behavior needs human review, and explain the concrete concern. Use Dangerous only for strong evidence of malicious behavior, and explain the evidence.

Pay particular attention to unexpected downloads, changed domains or repository owners, obfuscated shell, credential or private-data access, persistence, privilege escalation, and code execution hidden in packaging steps. Normal dependencies and standard build/install commands are not suspicious by themselves.

Focus the review on what the update adds or changes, using unchanged content only as context.
Comment-only changes, including maintainer updates, are routine and safe.
Do not flag pre-existing behavior unchanged by the update.

If there is no meaningful prior diff, review the full PKGBUILD for concrete malicious behavior.

Version and checksum updates, as well as release archive changes such as ZIP to TAR.GZ and corresponding extraction changes, are routine and safe when they retain the same established upstream owner, repository, domain, and release location. Flag them only when there is separate concrete evidence of risk.

A provenance change crosses a trust boundary, such as changing the upstream owner, repository, domain, download host, or signing identity. A path or archive-format change within the same trusted release source is not a provenance change.

Do not use Suspicious merely because a legitimate change cannot be independently confirmed; require a concrete risk introduced by the update.

Executing an upstream program during packaging is not inherently suspicious. Treat common packaging tasks such as invoking a checksum-verified upstream binary to generate shell completions, manuals, metadata, or caches as ordinary behavior when the artifact comes from the package's established upstream source and the invocation matches its documented purpose. Likewise, an unused auxiliary checksum file is not a security concern when makepkg already verifies the actual source artifact. Flag execution only when its provenance, purpose, arguments, side effects, or placement are unexpected or insufficiently verified."#;

pub(super) type ProviderFuture<'a> = Pin<Box<dyn Future<Output = Result<Assessment>> + Send + 'a>>;

#[derive(Clone, Copy)]
pub(super) struct LoggingHook;

impl<M: CompletionModel> AgentHook<M> for LoggingHook {
    async fn on_event(&self, _ctx: &HookContext, event: StepEvent<'_, M>) -> Flow {
        match event {
            StepEvent::CompletionCall { turn, .. } => {
                debug!(turn, "starting model turn");
            }
            StepEvent::ToolCall {
                tool_name, args, ..
            } => {
                tracing::info!(tool_name, args, "calling agent tool");
            }
            _ => {}
        }
        Flow::cont()
    }
}

pub trait AiProvider: Send + Sync {
    fn assess<'a>(
        &'a self,
        model: &'a str,
        package_name: &'a str,
        pkgbuild: &'a str,
        commit_diff: &'a str,
        repository_path: &'a Path,
    ) -> ProviderFuture<'a>;
}

#[derive(Clone, Copy, Debug)]
pub enum Provider {
    Openai,
    Anthropic,
    Openrouter,
    Claude,
    Codex,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Openrouter => "openrouter",
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    fn implementation(self) -> Box<dyn AiProvider> {
        match self {
            Self::Openai => Box::new(openai::OpenAi),
            Self::Anthropic => Box::new(anthropic::Anthropic),
            Self::Openrouter => Box::new(openrouter::OpenRouter),
            Self::Claude => Box::new(claude::Claude),
            Self::Codex => Box::new(codex::Codex),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Assessment {
    pub verdict: Verdict,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    tag = "classification",
    content = "explanation",
    rename_all = "snake_case"
)]
pub enum Verdict {
    Safe,
    Suspicious(String),
    Dangerous(String),
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CliAssessment {
    classification: Classification,
    /// Empty only when classification is safe; otherwise explain the concrete concern.
    explanation: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Classification {
    Safe,
    Suspicious,
    Dangerous,
}

impl Verdict {
    pub fn database_fields(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::Safe => ("safe", None),
            Self::Suspicious(explanation) => ("suspicious", Some(explanation)),
            Self::Dangerous(explanation) => ("dangerous", Some(explanation)),
        }
    }
}

pub async fn assess(
    provider: Provider,
    model: &str,
    package_name: &str,
    pkgbuild: &str,
    commit_diff: &str,
    repository_path: &Path,
) -> Result<Assessment> {
    debug!(
        provider = provider.as_str(),
        model, package_name, "starting AI agent"
    );
    let assessment = provider
        .implementation()
        .assess(model, package_name, pkgbuild, commit_diff, repository_path)
        .await?;
    debug!(
        provider = provider.as_str(),
        model, package_name, "AI agent returned an assessment"
    );
    Ok(assessment)
}

pub(super) fn package_prompt(package_name: &str, pkgbuild: &str, commit_diff: &str) -> String {
    const MAX_REVIEW_DIFF_BYTES: usize = 256 * 1024;
    let end = commit_diff
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= MAX_REVIEW_DIFF_BYTES)
        .last()
        .unwrap_or(0);
    let (diff, truncation_notice) = if commit_diff.len() > MAX_REVIEW_DIFF_BYTES {
        (
            &commit_diff[..end],
            "\n[remaining commit diff omitted from the review prompt]",
        )
    } else {
        (commit_diff, "")
    };

    format!(
        "Package: {package_name}\n\nCurrent PKGBUILD:\n```bash\n{pkgbuild}\n```\n\nChanges from the review baseline to the current commit (PKGBUILD is listed first when changed):\n```diff\n{diff}{truncation_notice}\n```"
    )
}

pub(super) fn parse_assessment(response: &str) -> Result<Assessment> {
    serde_json::from_str(response.trim()).context("AI returned an invalid assessment")
}

pub(super) fn parse_codex_assessment(response: &str) -> Result<Assessment> {
    let wire: CliAssessment =
        serde_json::from_str(response.trim()).context("Codex returned an invalid assessment")?;

    cli_assessment(wire)
}

pub(super) fn cli_assessment(wire: CliAssessment) -> Result<Assessment> {
    let explanation = wire.explanation.trim().to_owned();
    let verdict = match wire.classification {
        Classification::Safe => Verdict::Safe,
        Classification::Suspicious if explanation.is_empty() => {
            bail!("AI returned suspicious without an explanation")
        }
        Classification::Suspicious => Verdict::Suspicious(explanation),
        Classification::Dangerous if explanation.is_empty() => {
            bail!("AI returned dangerous without an explanation")
        }
        Classification::Dangerous => Verdict::Dangerous(explanation),
    };

    Ok(Assessment { verdict })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_schema_does_not_use_one_of() {
        let schema = serde_json::to_value(schemars::schema_for!(CliAssessment)).unwrap();
        assert!(!schema.to_string().contains("oneOf"));
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn parses_explained_verdict() {
        let assessment = parse_codex_assessment(
            r#"{"classification":"suspicious","explanation":"changed domain"}"#,
        )
        .unwrap();
        assert!(matches!(assessment.verdict, Verdict::Suspicious(_)));
    }

    #[test]
    fn rejects_unexplained_non_safe_verdict() {
        assert!(
            parse_codex_assessment(r#"{"classification":"dangerous","explanation":""}"#).is_err()
        );
    }

    #[test]
    fn review_prompt_treats_same_upstream_archive_changes_as_safe() {
        assert!(REVIEW_PROMPT.contains(
            "release archive changes such as ZIP to TAR.GZ and corresponding extraction changes"
        ));
        assert!(REVIEW_PROMPT
            .contains("are routine and safe when they retain the same established upstream"));
        assert!(REVIEW_PROMPT.contains(
            "A path or archive-format change within the same trusted release source is not a provenance change"
        ));
    }

    #[test]
    fn review_prompt_requires_concrete_risk_and_handles_missing_diffs() {
        assert!(REVIEW_PROMPT.contains(
            "Do not use Suspicious merely because a legitimate change cannot be independently confirmed"
        ));
        assert!(REVIEW_PROMPT
            .contains("If there is no meaningful prior diff, review the full PKGBUILD"));
    }
}
