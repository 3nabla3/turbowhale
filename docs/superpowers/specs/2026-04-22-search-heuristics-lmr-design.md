# Search heuristics + LMR design

**Date:** 2026-04-22
**Status:** approved

## Goal

Add three well-understood search improvements — killer moves, history heuristic, late move reductions — in a single bundle, plus self-play infrastructure to measure the ELO gain. Expected improvement is roughly 150-200 ELO versus the current baseline at equal time control.

## Background

The current engine has PVS with alpha-beta, a transposition table (sharded, Lazy-SMP-ready), null move pruning, quiescence search, MVV-LVA capture ordering, and tapered PeSTO evaluation. The largest search-side gaps vs. a modern amateur engine are:

- No reduction of late quiet moves — every legal move is searched at full depth.
- Move ordering for quiet moves is arbitrary (stable sort with key 0).
- No memory of quiet moves that have historically caused cutoffs.

The three techniques in this spec plug those gaps. They are classic and well-studied; the design is conservative and follows standard formulations.

## Scope

**In scope**

1. **Killer moves** — two slots per ply, updated on quiet-move beta cutoffs.
2. **History heuristic** — `[color][from][to]` butterfly table, incremented by `depth²` on quiet-move cutoffs, saturated at 16384.
3. **Late move reductions** — precomputed `reduction[depth][move_index]` table using `round((ln(depth)·ln(move_index))/2.25)`, with null-window re-search at full depth on fail-high and full-window re-search on PV update. Skipped for: in-check nodes, captures, promotions, the first three moves, killers.
4. **Move ordering rewrite** — five-tier priority: TT move → captures (MVV-LVA) → killer 1 → killer 2 → quiets by history score.
5. **Self-play harness** — `scripts/selfplay.sh` using `fastchess`, downloading a released baseline binary by tag into a gitignored `./engines/` directory, playing it against the locally built dev binary, with a small shipped opening EPD and documented SPRT workflow.

**Out of scope**

- Evaluation changes (king safety, mobility, pawn structure, bishop pair, etc.).
- Other search pruning techniques (aspiration windows, SEE, futility, check extensions, IID, countermove history).
- Transposition table restructuring.
- Changes to Lazy SMP.

## Files touched

- `src/engine.rs` — all search changes and unit tests.
- `scripts/selfplay.sh` — new.
- `scripts/openings.epd` — new, small opening book.
- `.gitignore` — add `/engines/`.
- `README.md` — new "Measuring strength" section.

## Data structures

All three additions live in `SearchContext` (per-thread — not shared across Lazy SMP workers).

```rust
const MAX_SEARCH_PLY: usize = 128;

pub struct SearchContext {
    // ... existing fields
    pub killer_moves: [[Option<Move>; 2]; MAX_SEARCH_PLY],
    pub history_scores: [[[i32; 64]; 64]; 2],   // [color][from][to]
}
```

**Memory footprint per worker:** killers ~1.5 KB, history 32 KB. Comfortable.

**Lifetime:** allocated fresh at the start of every `select_move` call — no cross-search carryover. Matches how the existing `SearchContext` is constructed per search and keeps behaviour consistent position-to-position.

**Ply bound:** if `ply >= MAX_SEARCH_PLY`, killer updates and lookups are skipped (array bounds safety). The engine rarely exceeds depth 30, and quiescence adds at most ~20 more, so 128 is safe.

**Reduction table:** one global `OnceLock<[[u8; 64]; 64]>` computed once.

```rust
reduction[depth][move_index] = ((ln(depth as f64) * ln(move_index as f64)) / 2.25).round().max(0.0) as u8
```

Lookups clamp indices with `.min(63)`. `u8` is sufficient — reductions will be small.

## Algorithm changes

### Move ordering

Replace the current two-tier sort with a five-tier priority. Sort ascending on this key:

```
if move == tt_best_move                   : i32::MIN
else if is_capture(move)                  : -10_000_000 - mvv_lva_score(move)
else if move == killer_moves[ply][0]      : -1_000_000
else if move == killer_moves[ply][1]      :   -999_999
else                                      : -history_scores[color][from][to]
```

`order_moves` gains new parameters `ply`, `&killer_moves`, and `&history_scores`, passed through from `negamax_pvs`.

### Main search loop with LMR

Inside the `for chess_move in &ordered_moves` loop, the existing first-move-vs-rest logic is replaced with:

```rust
let is_quiet = !is_capture(chess_move, position)
            && chess_move.promotion_piece.is_none();
let is_killer = killer_moves[ply as usize][0] == Some(chess_move)
             || killer_moves[ply as usize][1] == Some(chess_move);

let score = if move_index == 0 {
    // First move: full-window, full-depth
    -negamax_pvs(&child, depth - 1, -beta, -alpha, ply + 1, ctx)
} else {
    let reduction = if depth >= 3
                    && move_index >= 3
                    && !is_in_check
                    && is_quiet
                    && !is_killer {
        reduction_table()[depth.min(63) as usize][move_index.min(63)]
    } else { 0 };

    // Null-window, possibly reduced
    let reduced_score = -negamax_pvs(
        &child, depth - 1 - reduction as u32,
        -alpha - 1, -alpha, ply + 1, ctx,
    );

    // Re-search at full depth if the reduction lied
    let null_window_score = if reduction > 0 && reduced_score > alpha {
        -negamax_pvs(&child, depth - 1, -alpha - 1, -alpha, ply + 1, ctx)
    } else { reduced_score };

    // Re-search with full window on PV update
    if null_window_score > alpha && null_window_score < beta && beta - alpha > 1 {
        -negamax_pvs(&child, depth - 1, -beta, -alpha, ply + 1, ctx)
    } else {
        null_window_score
    }
};
```

The current `first_move: bool` flag is replaced by `for (move_index, chess_move) in ordered_moves.iter().enumerate()`.

### Beta cutoff — killer and history update

When `score >= beta`, before the TT store and return, update heuristics for quiet moves only:

```rust
if score >= beta {
    if is_quiet && ply < MAX_SEARCH_PLY as u32 {
        if killer_moves[ply as usize][0] != Some(*chess_move) {
            killer_moves[ply as usize][1] = killer_moves[ply as usize][0];
            killer_moves[ply as usize][0] = Some(*chess_move);
        }
        let color_idx = position.side_to_move as usize;
        let bonus = (depth * depth) as i32;
        let entry = &mut history_scores[color_idx]
                              [chess_move.from_square as usize]
                              [chess_move.to_square as usize];
        *entry = (*entry + bonus).min(16384);
    }
    // ... existing TT store + return beta
}
```

Captures and promotions never touch killers or history — captures already sort by MVV-LVA.

### What is not changed

- Null move pruning logic — unchanged.
- Quiescence search — unchanged (no LMR, killers, or history there).
- Transposition table store / probe — unchanged.
- Evaluation — unchanged.

## Testing

### Unit tests in `src/engine.rs`

1. **killer move stored on quiet cutoff** — a position where a quiet move causes a cutoff at depth 2; assert `killer_moves[ply][0]` equals that move.
2. **killer not stored on capture cutoff** — a capture causes the cutoff; killer slot remains `None`.
3. **killer slot rotation** — two successive cutoffs at the same ply shift into slots `[0]` and `[1]`.
4. **history incremented on quiet cutoff** — `history_scores[color][from][to] > 0` after the cutoff.
5. **history saturates** — force many cutoffs on the same move; value clamps at 16384.
6. **LMR skipped in check** — no reduced search at a node where the side to move is in check.
7. **LMR re-search on fail-high** — a tactical position where the best move is ordered late; the engine must still find it (re-search restores correctness when the reduction lies).
8. **mate finding preserved** — existing `select_move_finds_mate_in_one`, `negamax_detects_checkmate`, and in-check / king-and-pawn regression tests continue to pass.

### Node-count sanity check

`search_nodes_reduced_by_lmr` — searches a small set of middlegame positions at a fixed depth and asserts total nodes searched is at least 30% lower than the baseline. The baseline number is captured once after implementation and hardcoded as a ceiling, protecting against regressions that silently disable pruning.

## Self-play infrastructure

### Prerequisites

- `fastchess` — installed from https://github.com/Disservin/fastchess. Install steps documented in the README.
- `curl` — used to download released binaries.
- `scripts/openings.epd` — a small opening book shipped in-tree, ~200 positions, to reduce variance from repeated openings. Source is documented in the file header.

### Engine storage

Downloaded baseline binaries live under `./engines/` at the repo root, one file per tag (e.g. `./engines/turbowhale-v1.4.0`). The directory is created on demand by the script and added to `.gitignore` (`/engines/`).

### Script — `scripts/selfplay.sh`

Contract:

- **Args:** `<baseline_tag> [games] [tc]`. The tag is passed without the `v` prefix (e.g. `1.4.0`) — the script prepends `v` to match the release tag format used by this project.
- **Baseline:** downloaded from the GitHub release for `v<tag>` into `./engines/turbowhale-v<tag>`. Skipped if the file is already present. Asset name is `turbowhale-v<tag>-<arch>-<os>` where `<arch>` is `uname -m` (`x86_64` or `aarch64`) and `<os>` is `linux` or `macos` (from `uname -s`). Windows is not supported by this script.
- **Challenger:** built in place by running `cargo build --release` in the current working directory; the binary used is `./target/release/turbowhale`.
- **Defaults:** `games=500`, `tc=10+0.1`.
- **Failure modes:** if the release asset does not exist (404) or the architecture is unsupported, the script exits non-zero with a clear message before invoking `fastchess`.

Sketch:

```bash
#!/usr/bin/env bash
# Usage: ./scripts/selfplay.sh <baseline_tag> [games] [tc]
#   baseline_tag: release version without leading "v" (e.g. 1.4.0)
#   games:        default 500
#   tc:           default "10+0.1"
set -euo pipefail

tag="${1:?need baseline tag, e.g. 1.4.0}"
games="${2:-500}"
tc="${3:-10+0.1}"

repo="3nabla3/turbowhale"
arch="$(uname -m)"   # x86_64 | aarch64
case "$(uname -s)" in
  Linux)  os="linux"  ;;
  Darwin) os="macos"  ;;
  *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

mkdir -p engines
baseline="engines/turbowhale-v${tag}"
if [[ ! -x "$baseline" ]]; then
    asset="turbowhale-v${tag}-${arch}-${os}"
    url="https://github.com/${repo}/releases/download/v${tag}/${asset}"
    echo "Downloading $asset ..."
    curl -fL -o "$baseline" "$url"
    chmod +x "$baseline"
fi

echo "Building challenger ..."
cargo build --release
challenger="$(pwd)/target/release/turbowhale"

fastchess \
  -engine cmd="$baseline"   name="v${tag}" \
  -engine cmd="$challenger" name="dev" \
  -each tc="$tc" proto=uci \
  -rounds "$((games / 2))" -games 2 -repeat \
  -openings file=scripts/openings.epd format=epd order=random \
  -sprt elo0=0 elo1=10 alpha=0.05 beta=0.05 \
  -concurrency "$(nproc)" \
  -pgnout selfplay.pgn
```

The SPRT line tests `H0: ≤0 ELO` vs `H1: ≥10 ELO` and stops early once either hypothesis is confirmed, so short runs suffice when the gain is real.

### README additions

A new short section **"Measuring strength"** documents: installing `fastchess`, running `./scripts/selfplay.sh <tag>`, and where the output PGN + SPRT log land.

## Success criteria

- All existing tests still pass.
- New unit tests pass.
- Node-count sanity test shows ≥30% reduction at fixed depth on the chosen positions.
- Self-play match vs. a downloaded released baseline (e.g. `./scripts/selfplay.sh 1.4.0`) shows a positive ELO with the SPRT H1 hypothesis accepted (or, if run fixed-length, a positive margin outside the 95% error bar).
