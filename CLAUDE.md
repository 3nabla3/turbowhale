# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Naming

NEVER abbreviate variable names or function names. Always use full, descriptive names. Code is read far more than it is written.

## Git Workflow

Always use the `/gitbutler` skill for git write operations (commits, branches, pushes). Never commit directly to `master` — use feature branches.

NEVER use git worktrees. GitButler allows multiple branches to be checked out simultaneously, so use that instead.

When working on multiple concerns at once, keep them on separate branches:
- Two features → two feature branches
- A feature plus an unrelated bug fix → a feature branch and a separate bug fix branch

Always ensure each commit lands on the correct branch.

## Build and Test Commands

```bash
cargo build                    # debug build
cargo build --release          # optimized release build
cargo test                     # run all tests
cargo test <test_name>         # run a single test by name
cargo test -p turbowhale -- board::tests  # run tests in a specific module
cargo clippy                   # lint
```

The engine binary speaks UCI over stdin/stdout. To run it interactively:
```bash
cargo run --release
```

Rust toolchain is pinned to `1.94` via `rust-toolchain.toml`.

## Architecture

**turbowhale** is a UCI chess engine. Data flows: UCI input → position + search params → move generation → alpha-beta search → UCI output.

### Module Responsibilities

| Module | Role |
|--------|------|
| `board` | Core data types (`Position`, `Move`, `Color`, `PieceType`, `MoveFlags`), FEN parsing/serialization, `apply_move` |
| `movegen` | Pseudo-legal and legal move generation; `is_square_attacked`; precomputed attack tables via `OnceLock` |
| `engine` | Iterative deepening PVS (principal variation search) with quiescence search, move ordering (TT move first, then MVV-LVA), time management |
| `tt` | Zobrist hashing (`compute_hash`) and `TranspositionTable` (fixed-size, power-of-two, always-replace) |
| `eval` | Static evaluation — currently material count only, returned relative to side to move |
| `perft` | Move correctness testing via perft node counts; `perft_divide` for debugging |
| `uci` | UCI protocol parser (`parse_uci_command`), `run_uci_loop`, search runs on a background thread with `stop_flag` |
| `telemetry` | OpenTelemetry tracing init (optional, requires `OTEL_BACKEND_URL` env var) |

### Key Design Decisions

**Square indexing**: a1=0, b1=1, …, h1=7, a2=8, …, h8=63 (little-endian rank-file).

**Bitboards**: `Position` stores one `u64` per piece type per color (12 total), plus derived `white_occupancy`, `black_occupancy`, `all_occupancy` kept in sync via `recompute_occupancy()`.

**`apply_move` is pure**: returns a new `Position`, never mutates the input. Always calls `recompute_occupancy()` before returning.

**Capture flag semantics**: `MoveFlags::CAPTURE` is only set for pawn captures and en passant. Sliding-piece and knight captures do not set it. `apply_move` unconditionally clears enemy pieces from the destination square (except en passant), so this is correct.

**Castling rights**: stored as a 4-bit `u8` (bit 0=white kingside, 1=white queenside, 2=black kingside, 3=black queenside). `castling_rights_mask_after_move` computes the AND-mask to apply after each move.

**Search**: `negamax_pvs` with alpha-beta, PV search (null-window re-search), and a transposition table. Stop condition is checked every 1024 nodes via an `AtomicBool`. The `SearchContext` struct bundles all mutable search state.

**Evaluation scores** are always from the perspective of the side to move (positive = good for the current player).

**Perft tests** in `src/perft.rs` are the correctness ground truth for move generation. If movegen changes, verify perft counts against the standard positions (startpos, Kiwipete, Position 3, Position 5).
