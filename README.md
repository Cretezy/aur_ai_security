# AUR Security

The [Arch User Repository](https://aur.archlinux.org/) makes an enormous range
of software easy to install, but it comes with a specific trust model: a
`PKGBUILD` is shell code written by another user, and installing a package
means running it.

In June 2026, Arch published an
[official notice about an active malicious-packages
incident](https://archlinux.org/news/active-aur-malicious-packages-incident/).
A [second wave reported in
July](https://www.phoronix.com/news/Arch-Linux-AUR-Adoptions-Halted) led Arch
to temporarily halt package adoptions. In both cases, users were asked to
review every `PKGBUILD` and install-script change.

Reading every update by hand does not scale, but reducing the answer to “an AI
said it was safe” is not useful either. AUR Security adds another review layer:
it indexes package versions, examines AUR Git repositories for malware and
supply-chain risk, and preserves the exact evidence behind every verdict.

[Read the article](https://cretezy.com/2026/aur-security/) ·
[Browse the live results](https://aur-security.cretezy.com/)

[![A suspicious AUR package assessment with its highlighted commit diff](docs/aur-security-demo.png)](https://aur-security.cretezy.com/)

## The problem with reviewing only a PKGBUILD

Looking at the current `PKGBUILD` is a useful start, but it loses the most
important context: what changed?

A package downloading a binary from its established upstream GitHub release is
normal. The same package suddenly downloading from an unrelated domain is much
more interesting. A new checksum is expected when the version changes; a new
repository owner or delivery mechanism deserves attention even if the shell
code still looks clean.

The AUR stores each package base in Git, so the useful unit of review is a
package version together with its repository commit, complete `PKGBUILD`, and
the diff that introduced it.

## How it works

The core is a reusable AI checker. A CLI uses it to review packages collected
by the indexer, while a web application catalogs the resulting assessments and
their evidence.

### [Checker library](checker/)

For each update, the checker:

1. opens the AUR Git repository;
2. records the current commit;
3. reads the complete `PKGBUILD`;
4. builds a commit diff with `PKGBUILD` first, followed by the other changed
   files;
5. sends that context to the selected AI provider; and
6. returns the verdict and explanation with the source, diff, and commit it
   reviewed.

The checker supports OpenAI, Anthropic, and OpenRouter through
[Rig](https://github.com/0xPlaygrounds/rig), as well as the Claude Code and
Codex CLIs. API-backed checks can use a restricted `read_file` tool to inspect
other repository files when the diff needs more context. CLI-backed checks run
in isolated sessions with read-only repository access.

#### Scoring

Checks return one of three verdicts: **safe** for ordinary packaging behavior,
**suspicious** when a concrete concern needs human review, or **dangerous**
when there is strong evidence of malicious behavior.

The prompt treats routine version and checksum bumps as normal while giving
more scrutiny to changed sources, obfuscated commands, credential access,
persistence, and privilege escalation.

Verdicts are signals, not proof: models can be wrong, and downloaded binaries
are not analyzed. Review the evidence before installing a package.

### [CLI](cli/) and [web application](web/)

`update-index` downloads current AUR metadata and appends newly seen package
versions to SQLite. `check` selects current versions that have not already been
reviewed with the chosen provider and model, then passes them through the
checker concurrently.

The [Topcoat](https://github.com/tokio-rs/topcoat) web application provides a
searchable catalog of packages and checks. Each result includes its check
history, full `PKGBUILD`, and highlighted update diff so the verdict can be
reviewed instead of treated as a black box.

## paru integration

The experimental
[`aur-security` branch](https://github.com/Cretezy/paru/tree/aur-security)
brings these checks into the package installation workflow. It can use the
hosted API, the local checker library, or both. In remote-only mode, paru looks
up assessments for every commit since the last accepted version. In local-only
mode, it uses the checker library to review the cumulative diff locally.

With both enabled, paru checks the hosted API first. It only runs a local
assessment when the remote results do not cover the complete update. The
results are combined, and the package is safe only when the full update is
covered and safe.

These `paru.conf` options enable remote lookups with a local Codex fallback:

```ini
[options]
AurSecurityRemote
# AurSecurityRemoteUrl = https://aur-security.cretezy.com
AurSecurityProvider = codex
AurSecurityModel = gpt-5.6-luna
# SkipSafeReviews
# SkipAurSecurity
```

The remote URL is optional and defaults to the hosted service. To use remote
assessments alone, omit `AurSecurityProvider` and `AurSecurityModel`. To run
only local assessments, omit `AurSecurityRemote` and set both the provider and
model. Supported providers are described in [AI providers](#ai-providers).

`SkipSafeReviews` makes skipping manual review for safely assessed packages the
default and remembers them as the baseline for the next update, while
`SkipAurSecurity` disables both remote and local assessments.

The checks run after paru downloads the AUR repositories and before it executes
pre-build commands or starts the package build.

![paru displaying remote and local AUR security assessments before an upgrade](docs/paru-example.png)

## Quick start

You need Rust, Cargo, Git access to `aur.archlinux.org`, and either API credentials or an installed and authenticated Claude Code or Codex CLI.

Download the current AUR index:

```bash
cargo run -p aur_security -- update-index
```

Preview packages modified during the last day without cloning repositories or calling a provider:

```bash
cargo run -p aur_security -- check --dry-run \
  --provider openai \
  --model gpt-5.6-luna \
  --since 24h
```

Run checks with OpenAI:

```bash
export OPENAI_API_KEY="your-api-key"

cargo run -p aur_security -- check \
  --provider openai \
  --model gpt-5.6-luna \
  --filter 010editor spotify
```

The default database is `sqlite.db`. Override it with the global option:

```bash
cargo run -p aur_security -- --database data/aur-security.sqlite update-index
```

For release binaries, run `cargo build --release`. This produces `target/release/aur_security` and `target/release/aur_security_web`.

## CLI

### `update-index`

`update-index` downloads `packages-meta-v1.json.gz` and records the current version of every AUR package. New versions are appended; older versions remain as history but are marked non-current. Seeing the same package and version again refreshes its metadata.

Stored metadata includes package and package-base identifiers, version, submitter, last-modified time, popularity, snapshot path, and first-seen, last-seen, and current-version state.

### `check`

`check` reviews current package versions that have not already been checked with the selected provider and model:

```bash
cargo run -p aur_security -- check --provider <PROVIDER> --model <MODEL> [OPTIONS]
```

Supported providers are `openai`, `anthropic`, `openrouter`, `claude`, and `codex`. Useful filters include:

```bash
# Exact package names
--filter 010editor spotify visual-studio-code-bin

# Relative time
--since 30m
--since 12h
--since 7d

# Unix timestamp, relative seconds, RFC 3339, or date
--since 1773703418
--since -30
--since 2026-03-17T12:00:00Z
--since 2026-03-17
```

`--since` uses the package's AUR `LastModified` timestamp and composes with `--filter` and `--dry-run`. Timestamps without an explicit timezone are interpreted as UTC.

Checks run concurrently across the available CPU cores by default. Use `--parallelism N` (or its `--jobs N` alias) to override the number of concurrent checks, or set `AUR_SECURITY_CHECK_PARALLELISM`.

## AI providers

| Provider | Authentication | Model argument |
| --- | --- | --- |
| OpenAI | `OPENAI_API_KEY` | OpenAI model name |
| Anthropic | `ANTHROPIC_API_KEY` | Anthropic model name |
| OpenRouter | `OPENROUTER_API_KEY` | Usually `provider/model` |
| Claude CLI | Installed and authenticated Claude Code CLI | Claude model name or alias |
| Codex CLI | Installed and authenticated Codex CLI | Codex model name |

The API providers use [Rig](https://github.com/0xPlaygrounds/rig). Their only repository tool, `read_file`, can inspect UTF-8 text files up to 128 KiB inside the clone; absolute paths, parent traversal, and symlinks resolving outside the clone are rejected. Structured-output support through OpenRouter depends on the upstream model.

The Claude provider runs `claude` from the clone in a non-persistent safe-mode session. It ignores project and user customizations, denies interactive permission requests, limits repository access to the built-in `Read`, `Glob`, and `Grep` tools, and supplies the assessment JSON schema. The Codex provider runs `codex exec` from the clone in an ephemeral session. It ignores user configuration, disables shell tools and web search, and supplies the same schema.

## Results and storage

Each successful check stores the verdict and explanation, provider and model, checked Git commit, complete `PKGBUILD`, diff from the commit's first parent, and check timestamp. Checks are unique per package version, provider, and model.

The shared database crate runs migrations automatically when either application starts. Its main tables are:

- `package_versions`: historical AUR metadata keyed by package name and version
- `checks`: AI assessments linked to package-version rows

After changing the schema during development, delete the local database and run `update-index` to create a fresh one.

## Web interface

The [Topcoat](https://github.com/tokio-rs/topcoat) web application reads the same SQLite database as the CLI. Install the development CLI and start it from the workspace root:

```bash
cargo install topcoat-cli
topcoat dev --package aur_security_web
```

It listens on `http://127.0.0.1:3000` by default. Use `HOST`, `PORT`, and `AUR_SECURITY_DATABASE` to customize the server. The interface provides package search, verdict filters, check history, complete PKGBUILDs, and highlighted diffs.

The JSON API includes `POST /api/v1/checks/lookup` for batch assessment lookup by package base and ordered commit list.

For production, bundle the generated assets before starting the server:

```bash
cargo build --release -p aur_security_web
topcoat asset bundle --release --package aur_security_web
./target/release/aur_security_web --database sqlite.db
```

### Cloudflare Workers and D1

The `web` crate also builds the Worker target and serves the same UI from Cloudflare Workers with D1 storage. Follow [web/README.md](web/README.md) to apply migrations, configure the API secret, and deploy. The CLI connects through authenticated discrete API operations with `AUR_SECURITY_REMOTE_URL` and `AUR_SECURITY_API_TOKEN`; it never connects to D1 directly.

## Workspace

- `checker` (`aur_security_checker`): repository cloning, evidence collection, and AI assessment
- `db` (`aur_security_db`): SQLite connection, migrations, and queries
- `cli` (`aur_security`): indexing, candidate selection, and result persistence
- `web` (`aur_security_web`): native package browsing server and Cloudflare Worker/D1 target
- `protocol` (`aur_security_protocol`): shared wire types for local and remote database operations

The CLI and web application share the database crate; the checker is independent of clap, SQLx, and the index database.

## Development

```bash
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo fmt --all
```

Logging defaults to `INFO`. Use `RUST_LOG=debug` or a crate-specific filter such as `RUST_LOG=aur_security_checker=debug,aur_security=info`. SQLx remains at `INFO` and Rig at `WARN` under the broad debug filter; opt in with `sqlx=debug` or `rig_core=debug` when needed. Prompts and file contents are not logged.

## License

Licensed under the [GNU General Public License v3.0](LICENSE).
