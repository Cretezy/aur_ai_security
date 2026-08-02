# aur_ai_security

[aur_ai_security](https://github.com/Cretezy/aur_ai_security) is an AI-assisted review pipeline for the [Arch User Repository](https://aur.archlinux.org/). It indexes package versions, examines AUR Git repositories for malware and supply-chain risk, and catalogs the evidence in a searchable web interface.

The project keeps historical package versions in SQLite, checks only versions that are current in the latest AUR index, and stores each assessment separately from package metadata. Every check preserves the exact commit, full PKGBUILD, and commit diff so its verdict can be reviewed instead of treated as a black box.

## Motivation

An AUR package is user-maintained build metadata, and a `PKGBUILD` or install script is code that runs as part of building and installing software. That trust model became a concrete problem during the June 2026 [active AUR malicious-packages incident](https://archlinux.org/news/active-aur-malicious-packages-incident/). Arch reported a high volume of malicious package adoptions and updates, worked to remove malicious commits, and explicitly encouraged users to review every PKGBUILD and install-script change.

The campaign was not isolated. At the end of July, [Phoronix reported another wave of malicious package adoptions](https://www.phoronix.com/news/Arch-Linux-AUR-Adoptions-Halted), following an earlier campaign that affected more than 1,500 packages. Arch temporarily halted package adoptions while handling the new influx.

Those incidents are the reason this project exists. A familiar or orphaned package can be adopted and receive a malicious update through the same workflow as an ordinary version bump. Manually reviewing every update does not scale, but an unexplained “AI says safe” result is not a useful substitute. The goal is to surface unusual changes and preserve enough evidence for a human to verify the result.

## How it works

1. `update-index` downloads the current AUR metadata index. New package versions are appended to SQLite, while previously seen versions remain available as history.
2. `check` selects current versions that have not been reviewed with the requested provider and model. It clones each package-base repository and records the exact Git commit, complete `PKGBUILD`, and commit diff with `PKGBUILD` first.
3. The selected AI provider reviews the package in repository context. OpenAI, Anthropic, and OpenRouter use [Rig](https://github.com/0xPlaygrounds/rig); the Codex provider runs the Codex CLI from inside the clone with shell access and web search disabled.
4. The verdict, explanation, provider, model, source, and diff are stored separately from package metadata. The [Topcoat](https://github.com/tokio-rs/topcoat) web application catalogs packages and makes current results, history, PKGBUILDs, and diffs searchable and reviewable.

## Review philosophy

Checks return one of three verdicts instead of a misleading numeric score:

- `safe` for ordinary packaging behavior
- `suspicious` when a concrete concern needs human review
- `dangerous` when there is strong evidence of malicious behavior

The prompt is provenance-aware. A version and checksum bump from the same upstream source is generally normal, as is running a checksum-verified upstream binary to generate completions, manuals, or metadata. A changed domain, repository owner, download mechanism, obfuscated command, credential access, persistence, or privilege escalation deserves more scrutiny.

This remains a review aid, not proof that a package is safe. It does not analyze downloaded binaries and can produce false positives or miss malicious behavior. Always inspect suspicious changes, repository history, and upstream provenance before installing an AUR package.

## Demo

[![AUR AI Security showing a suspicious package assessment and highlighted commit diff](docs/aur-ai-security-demo.png)](docs/aur-ai-security-demo.png)

## Workspace layout

All crates live at the repository root:

- `checker` (`aur_ai_security_checker`): reusable library that clones an AUR package base, reads its `PKGBUILD`, captures its commit and diff, runs the selected AI provider, and returns a typed assessment.
- `db` (`aur_ai_security_db`): shared SQLite connection, migrations, and web-facing check queries.
- `cli` (`aur_ai_security`): command-line application, AUR metadata indexer, candidate selection, and result persistence.
- `web` (`aur_ai_security_web`): Topcoat web application for searching indexed packages and browsing check history, PKGBUILDs, and diffs.

The CLI and web app share the database crate. The checker remains independent of clap, SQLx, and the index database.

## Requirements

- Rust and Cargo
- Git/network access to `aur.archlinux.org`
- Credentials for an API provider, or an installed and authenticated Codex CLI
- The `topcoat` CLI for web development and asset bundling

## Build

```bash
cargo build --release
```

The resulting binaries are `target/release/aur_ai_security` and `target/release/aur_ai_security_web`. During CLI development, use `cargo run -p aur_ai_security -- <arguments>`.

## Quick start

Download the current AUR metadata index:

```bash
cargo run -p aur_ai_security -- update-index
```

Preview packages modified during the last day:

```bash
cargo run -p aur_ai_security -- check --dry-run \
  --provider openai \
  --model gpt-5.6-luna \
  --since 24h
```

Run checks with OpenAI:

```bash
export OPENAI_API_KEY="your-api-key"

cargo run -p aur_ai_security -- check \
  --provider openai \
  --model gpt-5.6-luna \
  --filter 010editor spotify
```

The default database is `sqlite.db`. Use another path with the global `--database` option:

```bash
cargo run -p aur_ai_security -- --database data/aur-ai-security.sqlite update-index
```

## Commands

### `update-index`

Downloads `packages-meta-v1.json.gz` from the AUR and records every package's current version:

```bash
cargo run -p aur_ai_security -- update-index
```

Package versions are keyed by package name and version. A newly seen version is appended as a new row. Older versions remain in the database but are marked non-current. Seeing the same package and version again refreshes its metadata instead of creating a duplicate.

Stored metadata includes:

- Package name and version
- Package base
- AUR package ID and package-base ID
- Submitter
- Last-modified timestamp
- AUR popularity
- Snapshot URL path
- First-seen, last-seen, and current-version state

The AUR `ID` identifies the package record, not an individual version. It is stored on each version row as a metadata snapshot.

### `check`

Selects current package versions that have not already been checked with the chosen provider and model. For each package, it clones the package-base Git repository, reads the current `PKGBUILD`, requests an AI assessment, and stores the result.

Both `--provider` and `--model` are required:

```bash
cargo run -p aur_ai_security -- check --provider <PROVIDER> --model <MODEL> [OPTIONS]
```

Supported providers are `openai`, `anthropic`, `openrouter`, and `codex`.

#### Dry run

Print matching package names and versions without cloning repositories or calling the selected provider:

```bash
cargo run -p aur_ai_security -- check --dry-run \
  --provider openai \
  --model gpt-5.6-luna
```

#### Package filters

Pass several exact AUR package names after one `--filter`:

```bash
cargo run -p aur_ai_security -- check --dry-run \
  --provider openai \
  --model gpt-5.6-luna \
  --filter 010editor spotify visual-studio-code-bin
```

The option may also be repeated:

```bash
cargo run -p aur_ai_security -- check --dry-run \
  --provider openai \
  --model gpt-5.6-luna \
  --filter 010editor \
  --filter spotify
```

#### Time filters

`--since` filters on the package's AUR `LastModified` timestamp. It composes with `--filter` and `--dry-run`.

Unit-bearing durations are relative to the command's start time:

```bash
--since 30m
--since 12h
--since 7d
```

Positive integers are absolute Unix timestamps in seconds. Negative integers are relative seconds:

```bash
--since 1773703418
--since -30
```

ISO-8601/RFC 3339 timestamps and dates are also accepted. Values without an explicit timezone are interpreted as UTC:

```bash
--since 2026-03-17T12:00:00Z
--since 2026-03-17T12:00:00
--since 2026-03-17
```

## AI providers

The API-based providers use [Rig](https://github.com/0xPlaygrounds/rig), keeping provider details out of the indexing, checking, and database layers. Each runs as a tool-enabled agent with one capability: `read_file`, which can inspect UTF-8 text files up to 128 KiB inside the cloned AUR repository. Absolute paths, parent traversal, and symlinks that resolve outside the clone are rejected.

All backends implement the shared `AiProvider` trait in `checker/src/ai/mod.rs`. Individual implementations live beside it in `openai.rs`, `anthropic.rs`, `openrouter.rs`, and `codex.rs`.

### OpenAI

Set `OPENAI_API_KEY` and pass an OpenAI model name:

```bash
cargo run -p aur_ai_security -- check \
  --provider openai \
  --model gpt-5.6-luna \
  --filter 010editor
```

### Anthropic

Set `ANTHROPIC_API_KEY` and pass an Anthropic model name:

```bash
cargo run -p aur_ai_security -- check \
  --provider anthropic \
  --model <anthropic-model> \
  --filter 010editor
```

### OpenRouter

Set `OPENROUTER_API_KEY` and pass an OpenRouter model identifier, normally in `provider/model` form:

```bash
cargo run -p aur_ai_security -- check \
  --provider openrouter \
  --model <provider/model> \
  --filter 010editor
```

Structured-output availability through OpenRouter depends on the selected upstream model.

### Codex CLI

The `codex` provider requires the Codex CLI to be installed and authenticated. It invokes `codex exec` from the cloned AUR Git repository with an ephemeral session, ignores user configuration, disables shell tools and web search, and supplies the assessment JSON schema:

```text
codex exec \
  --ephemeral \
  --ignore-user-config \
  -c 'features.shell_tool=false' \
  -c 'web_search="disabled"' \
  --model <MODEL> \
  --output-schema <TEMPORARY_SCHEMA_FILE> \
  '<REVIEW_PROMPT>'
```

Run it through the application with:

```bash
cargo run -p aur_ai_security -- check \
  --provider codex \
  --model <codex-model> \
  --filter 010editor
```

## Assessment results

Every successful check stores:

- One verdict:
  - `safe`
  - `suspicious`, with an explanation
  - `dangerous`, with an explanation
- Provider and model
- Checked Git commit
- The commit diff from its first parent, with `PKGBUILD` first
- The complete PKGBUILD reviewed by the selected provider
- Check timestamp

Checks are unique per package version, provider, and model. Changing either the provider or model allows the same package version to receive a separate assessment.

AI output is a review aid, not a guarantee that a package is safe. Inspect suspicious packages and their complete repository history manually before installing them.

## Database

The shared `aur_ai_security_db` crate owns the migrations, which run automatically when either application starts. The schema has two main tables:

- `package_versions`: historical AUR metadata, keyed by package name and version
- `checks`: AI assessments linked to package-version rows

The project uses one initial migration. After a schema change, delete the database and run `update-index` to initialize a fresh copy.

## Web interface

The `web` crate uses [Topcoat](https://github.com/tokio-rs/topcoat) and Topcoat's Tailwind integration. It reads the same SQLite database as the CLI.

Install Topcoat's development CLI once, then start the app from the workspace root:

```bash
cargo install topcoat-cli
topcoat dev --package aur_ai_security_web
```

It listens on `http://127.0.0.1:3000` by default. Set `HOST` and `PORT` to change the bind address. Set `AUR_AI_SECURITY_DATABASE` when developing against a non-default database:

```bash
AUR_AI_SECURITY_DATABASE=data/aur-ai-security.sqlite topcoat dev --package aur_ai_security_web
```

The routes are:

- `/`: welcome page and package search form
- `/search`: current AUR package search, limited to 100 results ordered by popularity
- `/checks`: checks ordered newest-first, 25 per page, with package and verdict filters
- `/checks/<repo>`: latest check and complete history for an AUR package base
- `/checks/<repo>/<commit>`: assessment metadata with a toggle between the stored commit diff and full PKGBUILD

Search uses the latest indexed package versions, including packages without checks. Every result links to its check-history page and the AUR; packages without assessments show an empty check state until one is completed. Package names and commit hashes elsewhere link to their corresponding AUR pages. Every check stores the full PKGBUILD plus all changed files with `PKGBUILD` first; individual patches over 256 KiB, binary files, and content beyond the 1 MiB commit limit are represented by omission markers. The detail view uses Highlight.js with Bash and diff highlighting.

Topcoat generates Tailwind CSS from the classes in `web/src` during the web crate's build. `topcoat dev` also bundles and serves the generated stylesheet. For a production build, bundle assets before starting the binary:

```bash
cargo build --release -p aur_ai_security_web
topcoat asset bundle --release --package aur_ai_security_web
./target/release/aur_ai_security_web --database sqlite.db
```

## Development

Logging defaults to `INFO` for both binaries. Set `RUST_LOG` to change the filter globally or per crate:

```bash
RUST_LOG=debug cargo run -p aur_ai_security -- update-index
RUST_LOG=aur_ai_security_checker=debug,aur_ai_security=info cargo run -p aur_ai_security -- check \
  --provider openai \
  --model gpt-5.6-luna
RUST_LOG=debug topcoat dev --package aur_ai_security_web
```

Debug logging focuses on query results, candidate selection, repository cloning, diff sizes, provider execution, and `read_file` paths and byte counts. Routine argument parsing and startup plumbing are omitted. Prompt and file contents are not logged.

A broad `RUST_LOG=debug` keeps SQLx at `INFO` to avoid logging every SQL statement. Set `RUST_LOG=debug,sqlx=debug` when those statements are specifically needed.

```bash
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo fmt --all
```
