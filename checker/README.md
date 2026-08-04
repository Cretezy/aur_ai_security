# AUR Security checker

`aur_security_checker` is the reusable review library behind
[AUR Security](../README.md). It inspects an AUR package repository, prepares
the current `PKGBUILD` and Git diff, and asks a selected AI provider to classify
the update for malware and supply-chain risk.

The crate does not depend on the package index or database. It can clone an AUR
package for a one-off check or review an existing repository, which also makes
it suitable for integrations such as package managers.

## Local checking

### What a check records

Every successful check returns a [`PackageCheck`](src/lib.rs) containing:

- the reviewed `HEAD` commit;
- the complete current `PKGBUILD`;
- the Git diff supplied as review evidence; and
- the provider's structured assessment.

The diff places `PKGBUILD` first, followed by the other changed files. By
default it compares `HEAD` with its first parent. Callers reviewing a range of
updates can instead provide a baseline commit and receive the cumulative diff
from that revision to `HEAD`.

### Usage

Use `check_package` to clone and review a package directly from the AUR:

```rust,no_run
use aur_security_checker::{check_package, Provider, Verdict};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let checked = check_package(
        Provider::Openai,
        "gpt-5.6-luna",
        "visual-studio-code-bin",
        "visual-studio-code-bin",
    )
    .await?;

    match checked.assessment.verdict {
        Verdict::Safe => println!("safe"),
        Verdict::Suspicious(explanation) => {
            println!("suspicious: {explanation}")
        }
        Verdict::Dangerous(explanation) => {
            println!("dangerous: {explanation}")
        }
    }

    Ok(())
}
```

The package name identifies the package being assessed, while the package base
selects the AUR Git repository to clone. They differ when one package base
produces multiple packages.

Use `check_repository` when the repository has already been cloned:

```rust,no_run
use std::path::Path;

use aur_security_checker::{check_repository, Provider};

async fn review() -> anyhow::Result<()> {
    let checked = check_repository(
        Provider::Codex,
        "gpt-5.6-luna",
        "example-package",
        Path::new("/path/to/aur-repository"),
        Some("0123456789abcdef0123456789abcdef01234567"),
    )
    .await?;

    println!("reviewed {}", checked.commit_id);
    Ok(())
}
```

Pass `None` as the baseline to compare `HEAD` with its first parent. For an
initial commit, the whole tree is treated as new.

### Providers

| Provider | `Provider` variant | Requirement |
| --- | --- | --- |
| OpenAI | `Openai` | `OPENAI_API_KEY` |
| Anthropic | `Anthropic` | `ANTHROPIC_API_KEY` |
| OpenRouter | `Openrouter` | `OPENROUTER_API_KEY` |
| Claude Code | `Claude` | Installed and authenticated `claude` CLI |
| Codex | `Codex` | Installed and authenticated `codex` CLI |

The model argument is passed directly to the selected provider. OpenRouter
models usually use the `provider/model` form.

API providers use [Rig](https://github.com/0xPlaygrounds/rig) and structured
output. Their only repository tool is `read_file`, which can read UTF-8 files
of up to 128 KiB within the repository. Absolute paths, parent traversal, and
symlinks resolving outside the repository are rejected.

The Claude provider runs a non-persistent safe-mode session with only `Read`,
`Glob`, and `Grep`. The Codex provider runs an ephemeral session with user
configuration ignored, shell tools disabled, and web search disabled.

### Scoring

Assessments return one of three verdicts:

- `Safe` for ordinary packaging behavior;
- `Suspicious(String)` when a concrete concern needs human review; or
- `Dangerous(String)` when there is strong evidence of malicious behavior.

The explanation is required for suspicious and dangerous assessments. These
verdicts are review signals, not proof that a package is safe, and the checker
does not analyze downloaded binaries.

### Limits

The checker requires `PKGBUILD` at the repository root and requires it to be
valid UTF-8. Individual file patches over 256 KiB are represented by an
omission notice, the assembled commit diff is limited to 1 MiB, and the review
prompt includes at most 256 KiB of that diff. The full assembled diff remains
available in `PackageCheck` for storage and inspection.

## Remote lookup

The crate also exposes `lookup`, which fetches stored assessments from the
lookup API. The lookup request and response types are shared with the workspace
protocol crate and re-exported by the checker for callers that only depend on
this crate.

```rust,no_run
use aur_security_checker::{lookup, LookupPackage, LookupRequest};

async fn fetch() -> anyhow::Result<()> {
    let response = lookup(
        "https://aur-security.cretezy.com",
        &LookupRequest {
            packages: vec![LookupPackage {
                package_base: "example-package".to_owned(),
                commits: vec!["0123456789abcdef0123456789abcdef01234567".to_owned()],
            }],
        },
    )
    .await?;

    for package in response.results {
        println!("{}: {} commits", package.package_base, package.commits.len());
    }
    Ok(())
}
```

Requests larger than the API's 1,000-commit limit are batched automatically.
The response preserves package and commit order, and an unmatched commit has
an `assessment` of `None`.

## Development

From the workspace root:

```bash
cargo test -p aur_security_checker --locked
cargo clippy -p aur_security_checker --locked --all-targets -- -D warnings
cargo fmt --all
```

## License

Licensed under the [GNU General Public License v3.0](../LICENSE).
