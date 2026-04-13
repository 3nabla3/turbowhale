# Engine Search Logging — Design Spec

**Date:** 2026-04-13  
**Status:** Approved

## Goal

Emit standard UCI `info` lines after each depth of iterative deepening so that chess GUIs and humans watching the engine can see search progress: depth, score, node count, speed, time elapsed, and the principal variation.

## Scope

Changes are confined to `src/engine.rs`. The `movegen.rs` tracing instrument change (already in the diff) is out of scope — it is an unrelated correctness fix and is kept as-is.

## Changes

### 1. Remove debug `eprintln!` calls

Remove all 8 `eprintln!` calls currently in the working tree:

- `engine.rs` `select_move`: "Initial stop_flag", "Starting depth", "Completed depth", "Breaking due to stop flag"
- `engine.rs` `negamax_pvs`: "Stop flag already set", "Setting stop flag due to time limit"
- `engine.rs` `quiescence_search`: "quiescence_search: Stop flag already set", "quiescence_search: Setting stop flag due to time limit"

### 2. Add `extract_pv_from_tt`

A pure function in `engine.rs`:

```rust
fn extract_pv_from_tt(root: &Position, tt: &TranspositionTable, max_depth: u32) -> Vec<Move>
```

**Algorithm:**
1. Start with a mutable copy of `root`.
2. Maintain a `HashSet<u64>` of visited position hashes to detect cycles.
3. For each step up to `max_depth`:
   - Compute the position hash via `compute_hash`.
   - If the hash is already in the visited set, stop (cycle).
   - Insert the hash into the visited set.
   - Probe the TT; if no entry or no best move, stop.
   - Push the best move onto the result vec.
   - Apply the move to advance the position.
4. Return the collected moves.

### 3. Emit UCI `info` after each complete depth

In `select_move`, after updating `best_move` from the TT, call `extract_pv_from_tt` and print:

```
info depth <d> score cp <n> nodes <n> nps <n> time <ms> pv <move1> <move2> ...
```

Mate score handling:
- If `score.abs() > MATE_SCORE / 2`, emit `score mate <N>` where `N = (MATE_SCORE - score.abs() + 1) / 2`.
- Sign: positive N if the engine is giving mate (`score > 0`), negative N if being mated (`score < 0`).

If the TT has no entry after a depth (uncommon but possible), emit the `info` line without a `pv` field rather than panicking.

`move_to_uci_string` (already in `uci.rs`) is used to format each move in the PV.

## What is not in scope

- Triangular PV table (more accurate but more complex; can be added later).
- Logging to a file.
- UCI `debug on` gating.
- Seldepth, hashfull, or other optional `info` fields.
