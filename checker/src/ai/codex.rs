use std::io::Write;

use anyhow::{bail, Context};
use tokio::process::Command;
use tracing::debug;

use super::{
    package_prompt, parse_codex_assessment, AiProvider, CliAssessment, ProviderFuture,
    REVIEW_PROMPT,
};

pub(super) struct Codex;

impl AiProvider for Codex {
    fn assess<'a>(
        &'a self,
        model: &'a str,
        package_name: &'a str,
        pkgbuild: &'a str,
        commit_diff: &'a str,
        repository_path: &'a std::path::Path,
    ) -> ProviderFuture<'a> {
        Box::pin(async move {
            let mut schema_file = tempfile::NamedTempFile::new()?;
            serde_json::to_writer(&mut schema_file, &schemars::schema_for!(CliAssessment))?;
            schema_file.flush()?;

            let prompt = package_prompt(package_name, pkgbuild, commit_diff);
            debug!(
                model,
                package_name,
                working_directory = %repository_path.display(),
                "starting codex CLI provider"
            );
            let output = Command::new("codex")
                .current_dir(repository_path)
                .arg("exec")
                .arg("--ephemeral")
                .arg("--ignore-user-config")
                .args(["-c", "features.shell_tool=false"])
                .args(["-c", "web_search=\"disabled\""])
                .args(["--model", model])
                .arg("--output-schema")
                .arg(schema_file.path())
                .arg(format!("{REVIEW_PROMPT}\n\n{prompt}"))
                .output()
                .await
                .context("failed to run `codex exec`; ensure the Codex CLI is installed")?;
            debug!(
                model,
                package_name,
                status = %output.status,
                "codex CLI provider finished"
            );

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!(
                    "`codex exec` failed with {}: {}",
                    output.status,
                    stderr.trim()
                );
            }

            let response =
                String::from_utf8(output.stdout).context("Codex output was not UTF-8")?;
            parse_codex_assessment(&response)
        })
    }
}
