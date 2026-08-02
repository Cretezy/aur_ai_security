use std::{future::Future, path::Path, pin::Pin};

use anyhow::{bail, Context, Result};
use rig_core::schemars::JsonSchema;
use serde::Deserialize;
use tracing::debug;

mod anthropic;
mod codex;
mod openai;
mod openrouter;
mod read_file;

pub(super) const REVIEW_PROMPT: &str = r#"You are reviewing an Arch User Repository PKGBUILD for malware and supply-chain risk.

Return exactly one verdict.
Use Safe for ordinary packaging behavior. Use Suspicious when behavior needs human review, and explain the concrete concern. Use Dangerous only for strong evidence of malicious behavior, and explain the evidence.

Pay particular attention to unexpected downloads, changed domains or repository owners, obfuscated shell, credential or private-data access, persistence, privilege escalation, and code execution hidden in packaging steps. Normal dependencies and standard build/install commands are not suspicious by themselves.

Do not flag an update that only changes version numbers and their corresponding checksums when the package still uses the same established upstream provenance. New releases from the same project owner, repository, domain, and release mechanism are generally routine and safe.

A provenance change is not routine. Pay closer attention when an update changes the upstream domain, repository owner or name, download host, source path pattern, signing identity, or delivery mechanism. Explain the concrete change and risk when one of these changes warrants human review; do not infer maliciousness from a version bump or checksum change alone.

Executing an upstream program during packaging is not inherently suspicious. Treat common packaging tasks such as invoking a checksum-verified upstream binary to generate shell completions, manuals, metadata, or caches as ordinary behavior when the artifact comes from the package's established upstream source and the invocation matches its documented purpose. Likewise, an unused auxiliary checksum file is not a security concern when makepkg already verifies the actual source artifact. Flag execution only when its provenance, purpose, arguments, side effects, or placement are unexpected or insufficiently verified."#;

pub(super) type ProviderFuture<'a> = Pin<Box<dyn Future<Output = Result<Assessment>> + Send + 'a>>;

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
    Codex,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Openrouter => "openrouter",
            Self::Codex => "codex",
        }
    }

    fn implementation(self) -> Box<dyn AiProvider> {
        match self {
            Self::Openai => Box::new(openai::OpenAi),
            Self::Anthropic => Box::new(anthropic::Anthropic),
            Self::Openrouter => Box::new(openrouter::OpenRouter),
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
pub(super) struct CodexAssessment {
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
        "Package: {package_name}\n\nCurrent PKGBUILD:\n```bash\n{pkgbuild}\n```\n\nChanges from the first parent commit (PKGBUILD is listed first when changed):\n```diff\n{diff}{truncation_notice}\n```"
    )
}

pub(super) fn parse_assessment(response: &str) -> Result<Assessment> {
    serde_json::from_str(response.trim()).context("AI returned an invalid assessment")
}

pub(super) fn parse_codex_assessment(response: &str) -> Result<Assessment> {
    let wire: CodexAssessment =
        serde_json::from_str(response.trim()).context("Codex returned an invalid assessment")?;

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
        let schema = serde_json::to_value(schemars::schema_for!(CodexAssessment)).unwrap();
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
}
