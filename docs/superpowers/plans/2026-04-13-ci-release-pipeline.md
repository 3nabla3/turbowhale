# CI & Release Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two GitHub Actions workflows (CI and release) plus a `release.toml` config so that every push gets linted and tested, and every `v*.*.*` tag produces a 4-platform GitHub Release with named binaries.

**Architecture:** Two stateless workflows — `ci.yml` runs clippy and tests in parallel on every push/PR; `release.yml` runs a 4-target build matrix on tag pushes and a final publish job that assembles a GitHub Release. `cargo-release` manages version bumps and tagging locally. No secrets beyond the built-in `GITHUB_TOKEN` are required.

**Tech Stack:** GitHub Actions, Rust stable toolchain via `dtolnay/rust-toolchain`, `cross` crate for ARM Linux cross-compilation, `actions/cache@v4`, `actions/upload-artifact@v4`, `actions/download-artifact@v4`, `softprops/action-gh-release@v2`, `cargo-release`.

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `release.toml` | Create | Configure `cargo-release` (no GPG, auto-push) |
| `.github/workflows/ci.yml` | Create | Clippy + test jobs on every push/PR |
| `.github/workflows/release.yml` | Create | 4-platform build matrix + GitHub Release publish |

---

### Task 1: Create the feature branch

**Files:**
- (no file changes — git operation only)

- [ ] **Step 1: Create and switch to the feature branch**

```bash
git checkout -b ci/github-pipeline
```

Expected output:
```
Switched to a new branch 'ci/github-pipeline'
```

- [ ] **Step 2: Verify you are on the correct branch**

```bash
git branch --show-current
```

Expected output:
```
ci/github-pipeline
```

---

### Task 2: Add `release.toml`

**Files:**
- Create: `release.toml`

`cargo-release` looks for this file at the repo root. It controls whether it signs tags (we disable GPG) and whether it pushes automatically.

- [ ] **Step 1: Create `release.toml`**

Create the file at the repo root with this exact content:

```toml
sign-tag = false
push = true
```

- [ ] **Step 2: Verify the file exists and is correct**

```bash
cat release.toml
```

Expected output:
```
sign-tag = false
push = true
```

- [ ] **Step 3: Commit**

```bash
git add release.toml
git commit -m "chore: add release.toml for cargo-release config"
```

---

### Task 3: Create `.github/workflows/ci.yml`

**Files:**
- Create: `.github/workflows/ci.yml`

This workflow runs two jobs in parallel on every push to any branch and on every PR targeting `master`. Both jobs cache Cargo's registry and the `target/` directory keyed on `Cargo.lock` to avoid redundant downloads and recompilation.

- [ ] **Step 1: Create the workflows directory**

```bash
mkdir -p .github/workflows
```

- [ ] **Step 2: Create `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
    branches: ["**"]
  pull_request:
    branches: [master]

jobs:
  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy

      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Run clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Run tests
        run: cargo test
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add clippy and test workflow"
```

---

### Task 4: Create `.github/workflows/release.yml`

**Files:**
- Create: `.github/workflows/release.yml`

This workflow triggers only on `v*.*.*` tag pushes. The `build` job runs a 4-target matrix. Each leg produces a renamed binary and uploads it as a workflow artifact. The `publish` job waits for all legs, downloads all artifacts, and creates a GitHub Release.

The `aarch64-unknown-linux-gnu` leg uses `cross` (a cross-compilation tool that runs inside Docker) instead of `cargo` directly, because the GitHub-hosted Ubuntu runner is x86-64 and cannot natively run ARM binaries during linking.

The `publish` job needs `permissions: contents: write` to create releases via `GITHUB_TOKEN`.

- [ ] **Step 1: Create `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags:
      - "v*.*.*"

jobs:
  build:
    name: Build ${{ matrix.artifact_suffix }}
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            runner: ubuntu-latest
            binary_name: turbowhale
            artifact_suffix: x86_64-linux
            use_cross: false

          - target: aarch64-unknown-linux-gnu
            runner: ubuntu-latest
            binary_name: turbowhale
            artifact_suffix: aarch64-linux
            use_cross: true

          - target: aarch64-apple-darwin
            runner: macos-latest
            binary_name: turbowhale
            artifact_suffix: aarch64-macos
            use_cross: false

          - target: x86_64-pc-windows-msvc
            runner: windows-latest
            binary_name: turbowhale.exe
            artifact_suffix: x86_64-windows
            use_cross: false

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-${{ matrix.target }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Install cross
        if: ${{ matrix.use_cross }}
        run: cargo install cross --locked

      - name: Build with cross
        if: ${{ matrix.use_cross }}
        run: cross build --release --target ${{ matrix.target }}

      - name: Build natively
        if: ${{ !matrix.use_cross }}
        run: cargo build --release --target ${{ matrix.target }}

      - name: Rename binary for release
        shell: bash
        run: |
          TAG="${{ github.ref_name }}"
          SRC="target/${{ matrix.target }}/release/${{ matrix.binary_name }}"
          if [[ "${{ matrix.artifact_suffix }}" == "x86_64-windows" ]]; then
            DST="turbowhale-${TAG}-${{ matrix.artifact_suffix }}.exe"
          else
            DST="turbowhale-${TAG}-${{ matrix.artifact_suffix }}"
          fi
          cp "$SRC" "$DST"
          echo "ARTIFACT_FILE=$DST" >> "$GITHUB_ENV"

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact_suffix }}
          path: ${{ env.ARTIFACT_FILE }}

  publish:
    name: Publish GitHub Release
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: artifacts/*
          generate_release_notes: true
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add 4-platform release workflow"
```

---

### Task 5: Verify and push the branch

- [ ] **Step 1: Confirm all three files are present**

```bash
ls release.toml .github/workflows/ci.yml .github/workflows/release.yml
```

Expected output:
```
release.toml  .github/workflows/ci.yml  .github/workflows/release.yml
```

- [ ] **Step 2: Confirm commit history on the branch**

```bash
git log --oneline master..HEAD
```

Expected output (3 commits, newest first):
```
<sha> ci: add 4-platform release workflow
<sha> ci: add clippy and test workflow
<sha> chore: add release.toml for cargo-release config
```

- [ ] **Step 3: Push branch to origin**

```bash
git push -u origin ci/github-pipeline
```

Expected output ends with:
```
Branch 'ci/github-pipeline' set up to track remote branch 'ci/github-pipeline' from 'origin'.
```

---

## Post-Implementation Notes

**To install cargo-release locally (one-time setup):**
```bash
cargo install cargo-release
```

**To perform a release after merging the PR:**
```bash
# from master, after merging
cargo release patch    # 0.1.0 → 0.1.1
# or
cargo release minor    # 0.1.0 → 0.2.0
# or
cargo release major    # 0.1.0 → 1.0.0
```

**To use the released binary as a lichess bot:**
1. Create a dedicated bot account at lichess.org (cannot be an account that has played games)
2. Generate a token at https://lichess.org/account/oauth/token with scope `bot:play`
3. Upgrade the account: `curl -d '' https://lichess.org/api/bot/account/upgrade -H "Authorization: Bearer <token>"`
4. Download the appropriate binary from the GitHub Release
5. Configure [lichess-bot](https://github.com/lichess-bot-devs/lichess-bot) or [BotLi](https://github.com/Torom/BotLi) to use the binary as the engine
