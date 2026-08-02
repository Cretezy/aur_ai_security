use anyhow::Context;
use rig_core::{
    agent::OutputMode,
    client::{CompletionClient, ProviderClient},
    completion::Prompt,
    providers::anthropic,
};

use super::read_file::ReadFile;
use super::{
    package_prompt, parse_assessment, AiProvider, Assessment, ProviderFuture, REVIEW_PROMPT,
};

pub(super) struct Anthropic;

impl AiProvider for Anthropic {
    fn assess<'a>(
        &'a self,
        model: &'a str,
        package_name: &'a str,
        pkgbuild: &'a str,
        commit_diff: &'a str,
        repository_path: &'a std::path::Path,
    ) -> ProviderFuture<'a> {
        Box::pin(async move {
            let client = anthropic::Client::from_env()
                .context("ANTHROPIC_API_KEY must be set when using the Anthropic provider")?;
            let agent = client
                .agent(model)
                .preamble(REVIEW_PROMPT)
                .output_schema::<Assessment>()
                .output_mode(OutputMode::Native)
                .tool(ReadFile::new(repository_path)?)
                .build();
            let response = agent
                .prompt(package_prompt(package_name, pkgbuild, commit_diff))
                .max_turns(4)
                .await?;

            parse_assessment(&response)
        })
    }
}
