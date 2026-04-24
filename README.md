# Turbowhale

Turbowhale is a UCI compatible chess engine developed with Claude AI. This project aims to evaluate the performance of AI-generated code in the context of chess engine development.

## Features
- UCI (Universal Chess Interface) compatibility
- Alpha-beta search with PVS, transposition table, null move pruning, late move reductions
- Killer-move and history-heuristic quiet move ordering
- Lazy SMP multi-threading
- Built entirely with Claude AI code generation
- Performance evaluation of AI-generated implementations

## About
This chess engine was created to assess how well AI can generate functional, competitive code for complex algorithmic problems like chess engines.

## Usage
Compile and run with any UCI-compatible chess interface (e.g., Chess.com, Lichess, Arena).

```bash
cargo build --release
./target/release/turbowhale
```

## Measuring strength

A self-play harness is included for A/B testing changes against a published release.

**Prerequisites:**
- [`fastchess`](https://github.com/Disservin/fastchess) on your `PATH`
- `curl`
- Linux or macOS (x86_64 or aarch64)

**Run a match against a released version:**

```bash
./scripts/selfplay.sh 1.4.0              # default: 500 games at 10s+0.1s
./scripts/selfplay.sh 1.4.0 200 5+0.05   # 200 games at 5s+0.05s
```

The script downloads the baseline binary from GitHub Releases into `./engines/` (gitignored), builds the current working tree with `cargo build --release`, and runs an SPRT match (`H0: ≤0 ELO`, `H1: ≥10 ELO`) with the two engines alternating colors on each of the 30 openings in `scripts/openings.epd`. Output is written to `selfplay.pgn`.

## License
MIT or your preferred license
