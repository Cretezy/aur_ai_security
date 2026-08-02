use std::{
    fmt,
    path::{Component, Path, PathBuf},
};

use rig_core::{
    schemars::{self, JsonSchema},
    tool::Tool,
};
use serde::{Deserialize, Serialize};
use tracing::debug;

const MAX_FILE_BYTES: u64 = 128 * 1024;

#[derive(Clone)]
pub(super) struct ReadFile {
    repository: PathBuf,
}

impl ReadFile {
    pub(super) fn new(repository: &Path) -> std::io::Result<Self> {
        Ok(Self {
            repository: repository.canonicalize()?,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadFileArgs {
    /// Repository-relative path to a UTF-8 text file, such as `.SRCINFO` or `install.sh`.
    path: String,
}

#[derive(Debug)]
pub(super) struct ReadFileError(String);

impl fmt::Display for ReadFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ReadFileError {}

impl Tool for ReadFile {
    const NAME: &'static str = "read_file";

    type Error = ReadFileError;
    type Args = ReadFileArgs;
    type Output = String;

    fn description(&self) -> String {
        "Read a UTF-8 text file from the cloned AUR repository. Use this when the PKGBUILD or commit diff references another repository file that is relevant to the security verdict. Paths must be relative to the repository."
            .to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(ReadFileArgs))
            .expect("read_file arguments must have a valid JSON schema")
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let requested = Path::new(&args.path);
        if requested.as_os_str().is_empty()
            || requested.is_absolute()
            || requested.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ReadFileError(
                "path must be a non-empty repository-relative path without `..`".to_owned(),
            ));
        }

        let path = self
            .repository
            .join(requested)
            .canonicalize()
            .map_err(|error| ReadFileError(format!("could not resolve {}: {error}", args.path)))?;
        if !path.starts_with(&self.repository) {
            return Err(ReadFileError(
                "path resolves outside the cloned repository".to_owned(),
            ));
        }

        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|error| ReadFileError(format!("could not inspect {}: {error}", args.path)))?;
        if !metadata.is_file() {
            return Err(ReadFileError(format!("{} is not a file", args.path)));
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(ReadFileError(format!(
                "{} is too large to read ({} bytes; limit is {} bytes)",
                args.path,
                metadata.len(),
                MAX_FILE_BYTES
            )));
        }

        let contents = tokio::fs::read(path)
            .await
            .map_err(|error| ReadFileError(format!("could not read {}: {error}", args.path)))?;
        debug!(path = %args.path, bytes = contents.len(), "read_file tool read repository file");
        String::from_utf8(contents)
            .map_err(|_| ReadFileError(format!("{} is not a UTF-8 text file", args.path)))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[tokio::test]
    async fn reads_repository_file_and_rejects_parent_paths() {
        let repository = tempfile::tempdir().unwrap();
        fs::write(repository.path().join("install.sh"), "echo safe\n").unwrap();
        let tool = ReadFile::new(repository.path()).unwrap();

        let contents = tool
            .call(ReadFileArgs {
                path: "install.sh".to_owned(),
            })
            .await
            .unwrap();
        assert_eq!(contents, "echo safe\n");
        assert!(tool
            .call(ReadFileArgs {
                path: "../outside".to_owned(),
            })
            .await
            .is_err());
    }
}
