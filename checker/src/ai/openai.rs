use anyhow::Context;
use rig_core::{
    agent::OutputMode,
    client::{CompletionClient, ProviderClient},
    completion::Prompt,
    providers::openai,
};

use super::read_file::ReadFile;
use super::{
    package_prompt, parse_assessment, AiProvider, Assessment, LoggingHook, ProviderFuture,
    REVIEW_PROMPT,
};

pub(super) struct OpenAi;

impl AiProvider for OpenAi {
    fn assess<'a>(
        &'a self,
        model: &'a str,
        package_name: &'a str,
        pkgbuild: &'a str,
        commit_diff: &'a str,
        repository_path: &'a std::path::Path,
    ) -> ProviderFuture<'a> {
        Box::pin(async move {
            let client = openai::Client::from_env()
                .context("OPENAI_API_KEY must be set when using the OpenAI provider")?;
            let agent = client
                .agent(model)
                .preamble(REVIEW_PROMPT)
                .output_schema::<Assessment>()
                .output_mode(OutputMode::Native)
                .tool(ReadFile::new(repository_path)?)
                .add_hook(LoggingHook)
                .build();
            let response = agent
                .prompt(package_prompt(package_name, pkgbuild, commit_diff))
                .max_turns(4)
                .await?;

            parse_assessment(&response)
        })
    }
}
