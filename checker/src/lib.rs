use std::path::Path;

use anyhow::{anyhow, Context, Result};
use git2::{DiffOptions, Patch, Repository};
use tracing::debug;

const MAX_FILE_DIFF_BYTES: usize = 256 * 1024;
const MAX_COMMIT_DIFF_BYTES: usize = 1024 * 1024;

pub mod ai;

pub use ai::{AiProvider, Assessment, Provider, Verdict};

#[derive(Debug)]
pub struct PackageCheck {
    pub commit_id: String,
    pub commit_diff: String,
    pub pkgbuild: String,
    pub assessment: Assessment,
}

pub async fn check_package(
    provider: Provider,
    model: &str,
    package_name: &str,
    package_base: &str,
) -> Result<PackageCheck> {
    let directory = tempfile::tempdir()?;
    let url = format!("https://aur.archlinux.org/{package_base}.git");
    debug!(package_name, package_base, %url, "cloning AUR repository");
    Repository::clone(&url, directory.path()).with_context(|| format!("failed to clone {url}"))?;
    check_repository(provider, model, package_name, directory.path(), None).await
}

/// Checks an existing AUR repository at `HEAD`.
///
/// When `baseline` is present, the AI receives the cumulative tree diff from
/// that revision to `HEAD`. Otherwise, the diff is taken from `HEAD`'s first
/// parent, preserving [`check_package`]'s behavior for callers without a
/// previously reviewed revision.
pub async fn check_repository(
    provider: Provider,
    model: &str,
    package_name: &str,
    repository_path: &Path,
    baseline: Option<&str>,
) -> Result<PackageCheck> {
    let repository = Repository::open(repository_path)
        .with_context(|| format!("failed to open {}", repository_path.display()))?;
    let commit = repository.head()?.peel_to_commit()?;
    let tree = commit.tree()?;
    let entry = tree
        .get_name("PKGBUILD")
        .ok_or_else(|| anyhow!("repository has no PKGBUILD"))?;
    let blob = repository.find_blob(entry.id())?;
    let pkgbuild = std::str::from_utf8(blob.content()).context("PKGBUILD is not UTF-8")?;
    let commit_diff = commit_diff(&repository, &commit, baseline)?;
    debug!(
        package_name,
        commit = %commit.id(),
        pkgbuild_bytes = pkgbuild.len(),
        diff_bytes = commit_diff.len(),
        "prepared package review input"
    );
    let assessment = ai::assess(
        provider,
        model,
        package_name,
        pkgbuild,
        &commit_diff,
        repository_path,
    )
    .await?;
    Ok(PackageCheck {
        commit_id: commit.id().to_string(),
        commit_diff,
        pkgbuild: pkgbuild.to_owned(),
        assessment,
    })
}

fn commit_diff(
    repository: &Repository,
    commit: &git2::Commit<'_>,
    baseline: Option<&str>,
) -> Result<String> {
    let current_tree = commit.tree()?;
    let baseline_tree = match baseline {
        Some(revision) => Some(
            repository
                .revparse_single(revision)
                .with_context(|| format!("failed to resolve baseline revision {revision}"))?
                .peel_to_commit()
                .with_context(|| format!("baseline revision {revision} is not a commit"))?
                .tree()?,
        ),
        None if commit.parent_count() == 0 => None,
        None => Some(commit.parent(0)?.tree()?),
    };
    let mut options = DiffOptions::new();
    let diff = repository.diff_tree_to_tree(
        baseline_tree.as_ref(),
        Some(&current_tree),
        Some(&mut options),
    )?;
    let mut indexes: Vec<_> = (0..diff.deltas().len()).collect();
    indexes.sort_by_key(|index| {
        let delta = diff.get_delta(*index).expect("diff index must be valid");
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .unwrap_or_else(|| std::path::Path::new("unknown"));
        (path != std::path::Path::new("PKGBUILD"), path.to_owned())
    });

    let mut output = String::new();
    for index in indexes {
        let delta = diff.get_delta(index).expect("diff index must be valid");
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .unwrap_or_else(|| std::path::Path::new("unknown"))
            .display();
        let section = match Patch::from_diff(&diff, index)? {
            Some(mut patch) => {
                let buffer = patch.to_buf()?;
                if buffer.len() > MAX_FILE_DIFF_BYTES {
                    format!(
                        "diff --git a/{path} b/{path}\n[diff omitted: file patch is {} bytes; limit is {} bytes]\n",
                        buffer.len(),
                        MAX_FILE_DIFF_BYTES
                    )
                } else {
                    String::from_utf8_lossy(buffer.as_ref()).into_owned()
                }
            }
            None => format!(
                "diff --git a/{path} b/{path}\n[diff omitted: binary or unrenderable file]\n"
            ),
        };

        if output.len() + section.len() > MAX_COMMIT_DIFF_BYTES {
            output.push_str(&format!(
                "\ndiff --git a/{path} b/{path}\n[diff omitted: commit diff reached the {} byte limit]\n",
                MAX_COMMIT_DIFF_BYTES
            ));
            continue;
        }
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&section);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use git2::{IndexAddOption, Signature};

    use super::*;

    #[test]
    fn commit_diff_puts_pkgbuild_first_and_omits_large_files() {
        let directory = tempfile::tempdir().unwrap();
        let repository = Repository::init(directory.path()).unwrap();
        fs::write(directory.path().join("z-file"), "ordinary file\n").unwrap();
        fs::write(directory.path().join("PKGBUILD"), "pkgname=example\n").unwrap();
        fs::write(
            directory.path().join("large-file"),
            vec![b'a'; MAX_FILE_DIFF_BYTES + 1],
        )
        .unwrap();

        let mut index = repository.index().unwrap();
        index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let signature = Signature::now("Test", "test@example.com").unwrap();
        let commit_id = repository
            .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let commit = repository.find_commit(commit_id).unwrap();

        let rendered = commit_diff(&repository, &commit, None).unwrap();
        assert!(rendered.starts_with("diff --git a/PKGBUILD b/PKGBUILD"));
        assert!(rendered.contains("diff --git a/z-file b/z-file"));
        assert!(rendered.contains("diff omitted: file patch is"));
    }

    #[test]
    fn commit_diff_uses_the_explicit_cumulative_baseline() {
        let directory = tempfile::tempdir().unwrap();
        let repository = Repository::init(directory.path()).unwrap();
        let signature = Signature::now("Test", "test@example.com").unwrap();

        fs::write(directory.path().join("PKGBUILD"), "pkgver=1\n").unwrap();
        let baseline = commit_all(&repository, &signature, "initial", &[]);

        fs::write(directory.path().join("install.sh"), "echo added\n").unwrap();
        let parent = repository.find_commit(baseline).unwrap();
        let middle = commit_all(&repository, &signature, "add install", &[&parent]);
        drop(parent);

        fs::write(directory.path().join("PKGBUILD"), "pkgver=2\n").unwrap();
        let parent = repository.find_commit(middle).unwrap();
        let head_id = commit_all(&repository, &signature, "bump", &[&parent]);
        drop(parent);

        let head = repository.find_commit(head_id).unwrap();
        let rendered = commit_diff(&repository, &head, Some(&baseline.to_string())).unwrap();
        assert!(rendered.starts_with("diff --git a/PKGBUILD b/PKGBUILD"));
        assert!(rendered.contains("diff --git a/install.sh b/install.sh"));
        assert!(rendered.contains("+pkgver=2"));
    }

    fn commit_all(
        repository: &Repository,
        signature: &Signature<'_>,
        message: &str,
        parents: &[&git2::Commit<'_>],
    ) -> git2::Oid {
        let mut index = repository.index().unwrap();
        index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        repository
            .commit(Some("HEAD"), signature, signature, message, &tree, parents)
            .unwrap()
    }
}
