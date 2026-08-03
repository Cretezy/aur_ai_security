# AUR AI Security web app

The web app serves the package and security-check data stored by the CLI in a
SQLite database. Its Docker image contains the release web binary and the
matching Topcoat asset bundle.

## Build the image

Run the build from the repository root so Docker can access all workspace
crates:

```bash
docker build -f web/Dockerfile -t aur-ai-security-web:latest .
```

## Run the container

Create the data directory before starting the container:

```bash
mkdir -p docker-data
```

The web app creates and migrates `/data/sqlite.db` when it does not exist. To
serve data already populated by the CLI, copy that database into the directory
before starting the container:

```bash
cp sqlite.db docker-data/sqlite.db
```

The whole directory is mounted so the created database and its SQLite journal
files persist. Running with the host user's UID and GID gives the process write
access to the directory:

```bash
docker run --detach \
  --name aur-ai-security-web \
  --restart unless-stopped \
  --user "$(id -u):$(id -g)" \
  --publish 3000:3000 \
  --volume "$PWD/docker-data:/data" \
  aur-ai-security-web:latest
```

Open <http://127.0.0.1:3000>.

The container listens on port `3000` and reads
`/data/sqlite.db` by default. Override those defaults with the `PORT` and
`AUR_AI_SECURITY_DATABASE` environment variables if needed.

## Lookup API

The lookup API returns the latest stored security assessment for every commit
in one or more AUR package-base commit lists. It is intended for clients that
already know the ordered commits in the update range they want to check.

The examples below assume the web application is running locally at
`http://127.0.0.1:3000`. The endpoint does not currently require
authentication.

### Look up checks

```text
POST /api/v1/checks/lookup
Content-Type: application/json
```

The request body contains a `packages` array with at most 1,000 commits in
total:

```json
{
  "packages": [
    {
      "package_base": "paru",
      "commits": [
        "0123456789abcdef0123456789abcdef01234567",
        "89abcdef0123456789abcdef0123456789abcdef"
      ]
    },
    {
      "package_base": "yay",
      "commits": ["ffffffffffffffffffffffffffffffffffffffff"]
    }
  ]
}
```

Each entry has these required fields:

| Field | Type | Description |
| --- | --- | --- |
| `package_base` | string | Exact AUR package-base name. It must use lowercase ASCII letters, digits, `@`, `.`, `_`, `+`, or `-`, and cannot begin with `.` or `-`. |
| `commits` | non-empty array of strings | Ordered full 40-character hexadecimal Git commits. Hexadecimal matching is case-insensitive. |

An empty `packages` array is valid. Empty `commits` arrays are rejected.
Duplicate packages and commits are valid.

#### Example request

```bash
curl --fail-with-body \
  --request POST \
  --header 'Content-Type: application/json' \
  --data '{
    "packages": [
      {
        "package_base": "paru",
        "commits": ["0123456789abcdef0123456789abcdef01234567"]
      },
      {
        "package_base": "yay",
        "commits": ["ffffffffffffffffffffffffffffffffffffffff"]
      }
    ]
  }' \
  http://127.0.0.1:3000/api/v1/checks/lookup
```

#### Successful response

The endpoint returns `200 OK` with `Content-Type: application/json`:

```json
{
  "results": [
    {
      "package_base": "paru",
      "commits": [{
        "commit": "0123456789abcdef0123456789abcdef01234567",
        "assessment": {
          "verdict": "safe",
          "explanation": null,
          "provider": "openai",
          "model": "gpt-5.6-luna",
          "checked_at": 1785686400,
          "version": "2.1.0-1",
          "details_path": "/checks/paru/0123456789abcdef0123456789abcdef01234567"
        }
      }]
    },
    {
      "package_base": "yay",
      "commits": [{
        "commit": "ffffffffffffffffffffffffffffffffffffffff",
        "assessment": null
      }]
    }
  ]
}
```

The response contains one result for every requested package and one nested
status for every requested commit, preserving both levels of input order.
`package_base` and `commit` are echoed exactly as supplied. An unmatched pair
is not an error; its `assessment` is `null`.

When several stored checks match a pair, the most recently checked assessment
is returned. If checks have the same `checked_at` value, the most recently
stored one wins.

An assessment contains:

| Field | Type | Description |
| --- | --- | --- |
| `verdict` | string | Assessment verdict: `safe`, `suspicious`, or `dangerous`. |
| `explanation` | string or null | Provider explanation, when one was recorded. |
| `provider` | string | Provider used for the check, such as `openai`, `anthropic`, `openrouter`, or `codex`. |
| `model` | string | Model identifier used for the check. |
| `checked_at` | integer | Time the check completed, as Unix seconds. |
| `version` | string | AUR package version associated with the check. |
| `details_path` | string | Relative path to the human-readable check page. Resolve it against the API server's origin. |

### Errors

Invalid requests return `400 Bad Request` with a
`Content-Type: text/plain; charset=utf-8` body. For example:

```text
bad request: packages[0].commits[0] must be a 40-character hexadecimal commit
```

Validation errors include:

- More than 1,000 total commits: `packages must contain at most 1000 commits`
- An invalid package base: `packages[<index>].package_base is not a valid package-base name`
- An empty commit list: `packages[<index>].commits must contain at least one commit`
- A commit that is not exactly 40 hexadecimal characters: `packages[<index>].commits[<commit-index>] must be a 40-character hexadecimal commit`
- A missing or non-JSON `Content-Type`, malformed JSON, or a JSON body that does not match the request shape

Unexpected server or database failures return a 5xx response.
