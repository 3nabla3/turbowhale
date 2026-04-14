# CI & Release Pipeline Design — turbowhale

## Overview

Two GitHub Actions workflows automate quality checks and binary releases for the `turbowhale` chess engine. Versioning is managed locally with `cargo-release`; CI is stateless and only reacts to what is pushed.

---

## Versioning

The canonical version lives in `Cargo.toml` (`version` field). Git tags mirror it exactly (e.g. `v0.2.0`).

**Release flow (run locally by the developer):**

```
cargo release minor    # or patch / major
```

`cargo-release` performs these steps automatically:
1. Bumps `Cargo.toml`: e.g. `0.1.0` → `0.2.0`
2. Creates a commit: `"chore: release v0.2.0"`
3. Creates an annotated git tag: `v0.2.0`
4. Pushes the commit and tag to `origin`

A `release.toml` file at the repo root configures `cargo-release`:
- `sign-tag = false` — no GPG required
- `push = true` — auto-push after tagging

GitHub Actions reacts to the pushed tag. The developer never edits `Cargo.toml` version by hand.

---

## Workflow 1: CI (`ci.yml`)

**Trigger:** push to any branch, pull requests targeting `master`

**Jobs (run in parallel):**

### `clippy`
- Runner: `ubuntu-latest`
- Command: `cargo clippy --all-targets --all-features -- -D warnings`
- Fails on any lint warning

### `test`
- Runner: `ubuntu-latest`
- Command: `cargo test`
- Runs the full test suite on the host (no cross-compilation needed)

Both jobs cache `~/.cargo` and `target/` keyed on `Cargo.lock`.

---

## Workflow 2: Release (`release.yml`)

**Trigger:** push of a tag matching `v*.*.*`

### `build` job (matrix)

| Target triple | Runner | Cross-compile |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | No |
| `aarch64-unknown-linux-gnu` | `ubuntu-latest` | Yes, via `cross` |
| `aarch64-apple-darwin` | `macos-latest` | No |
| `x86_64-pc-windows-msvc` | `windows-latest` | No |

Each matrix leg:
1. Adds the target with `rustup target add <target>`
2. For `aarch64-unknown-linux-gnu`: installs `cross` and runs `cross build --release --target <target>`
3. For all other targets: runs `cargo build --release --target <target>`
4. Uploads the binary as a workflow artifact named after the target

### `publish` job

Runs after all `build` legs complete (`needs: build`).

1. Downloads all 4 artifacts
2. Creates a GitHub Release using the pushed tag as the release name
3. Uploads all 4 binaries with descriptive names:
   - `turbowhale-<tag>-x86_64-linux`
   - `turbowhale-<tag>-aarch64-linux`
   - `turbowhale-<tag>-aarch64-macos`
   - `turbowhale-<tag>-x86_64-windows.exe`

Uses only the built-in `GITHUB_TOKEN` — no extra secrets required.

---

## Files to Create

```
.github/
  workflows/
    ci.yml
    release.yml
release.toml
```

---

## Lichess Deployment Context

`turbowhale` communicates via UCI over stdin/stdout. To run it as a lichess bot:

1. Create a dedicated lichess bot account and upgrade it via `POST /api/bot/account/upgrade` with a `bot:play` scoped token
2. Use a bridge program ([lichess-bot](https://github.com/lichess-bot-devs/lichess-bot) or [BotLi](https://github.com/Torom/BotLi)) on a server to connect the lichess API to the engine binary
3. Download the appropriate release artifact from GitHub Releases and configure it as the engine path in the bridge's `config.yml`

Server deployment is out of scope for this pipeline — the GitHub Release artifact is the handoff point.
