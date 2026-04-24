# Search Heuristics + LMR Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add killer moves, history heuristic, and late move reductions (LMR) to the alpha-beta search, plus a self-play harness that pits the local dev binary against a tagged release downloaded from GitHub. Expected gain ~150-200 ELO.

**Architecture:** All three search techniques live as per-thread state on `SearchContext`. LMR uses a precomputed `(ln(depth)·ln(move_index))/2.25` reduction table with re-search on fail-high. Move ordering is rewritten into a five-tier priority (TT → captures → killer 1 → killer 2 → quiets by history). Self-play uses `fastchess` via a shell script that downloads the baseline release binary into a gitignored `./engines/` directory.

**Tech Stack:** Rust 1.94 (pinned via `rust-toolchain.toml`), `cargo`, `fastchess` (external, installed by the user), `curl`, bash.

**Spec reference:** `docs/superpowers/specs/2026-04-22-search-heuristics-lmr-design.md`.

**Branch:** All commits land on `feat/lmr-killers-history` (already created; currently contains just the design spec).

**Git policy:** This project uses GitButler — all write operations use `but`, never raw `git`. See `CLAUDE.md`. Each "commit" step below is:

```bash
but status -fv                                      # get change IDs
but commit feat/lmr-killers-history \
    -m "<commit message>" \
    --changes <id1>,<id2>,... \
    --status-after
```

Replace `<id1>,<id2>,...` with the CLI IDs printed by `but status -fv` for the files listed in that task's "Files" block. Never include files that weren't changed by the task.

---

## File Structure

| File | Role | Action |
|---|---|---|
| `src/engine.rs` | All search changes (`SearchContext` fields, reduction table, move ordering, killer/history updates, LMR, tests) | Modify |
| `.gitignore` | Exclude downloaded engine binaries | Modify |
| `scripts/selfplay.sh` | Download baseline + run `fastchess` match | Create |
| `scripts/openings.epd` | ~30-position opening book for the match | Create |
| `README.md` | "Measuring strength" section | Modify |

All Rust code stays in `src/engine.rs` to keep the change tightly scoped — the module already holds search logic, move ordering, and the existing test suite. No new Rust modules.

---

## Task 1: Wire killer moves, history scores, and LMR reduction table into SearchContext

**Files:**
- Modify: `src/engine.rs:1-14` (imports + constants)
- Modify: `src/engine.rs:24-31` (SearchContext struct)
- Modify: every construction site of `SearchContext` in `src/engine.rs` (in `search_worker`, `select_move`, and all test helpers)
- Test: new unit tests in the `mod tests` block of `src/engine.rs`

This task adds the data only. No behaviour change — killer/history fields are initialized but not yet read or written. The reduction table is populated but not yet consulted.

- [ ] **Step 1: Write failing test for SearchContext default killer/history state**

Add to the `mod tests` block in `src/engine.rs`:

```rust
#[test]
fn new_search_context_has_empty_killers_and_zero_history() {
    let context = SearchContext {
        transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
        stop_flag: Arc::new(AtomicBool::new(false)),
        shared_nodes: Arc::new(AtomicU64::new(0)),
        limits: SearchLimits::Depth(1),
        start_time: Instant::now(),
        nodes_searched: 0,
        killer_moves: [[None; 2]; MAX_SEARCH_PLY],
        history_scores: [[[0i32; 64]; 64]; 2],
    };
    assert!(context.killer_moves.iter().all(|slots| slots[0].is_none() && slots[1].is_none()));
    assert!(context.history_scores.iter().flatten().flatten().all(|&v| v == 0));
}
```

- [ ] **Step 2: Run the test — it must fail to compile**

Run: `cargo test -p turbowhale new_search_context_has_empty_killers_and_zero_history`
Expected: compile error, "no field `killer_moves` on type `SearchContext`" (or similar for `history_scores` and `MAX_SEARCH_PLY`).

- [ ] **Step 3: Add the MAX_SEARCH_PLY constant and the two fields to SearchContext**

In `src/engine.rs`, just below the existing `NULL_MOVE_REDUCTION` constant (currently at line 14), add:

```rust
pub const MAX_SEARCH_PLY: usize = 128;
```

Update the `SearchContext` struct (currently at lines 24-31) to:

```rust
pub struct SearchContext {
    pub transposition_table: Arc<ShardedTranspositionTable>,
    pub stop_flag: Arc<AtomicBool>,
    pub shared_nodes: Arc<AtomicU64>,
    pub limits: SearchLimits,
    pub start_time: Instant,
    pub nodes_searched: u64,
    pub killer_moves: [[Option<Move>; 2]; MAX_SEARCH_PLY],
    pub history_scores: [[[i32; 64]; 64]; 2],
}
```

- [ ] **Step 4: Initialize the new fields at every SearchContext construction site**

There are five sites. For each, add these two lines inside the struct literal:

```rust
killer_moves: [[None; 2]; MAX_SEARCH_PLY],
history_scores: [[[0i32; 64]; 64]; 2],
```

Sites (confirm with `grep -n 'SearchContext {' src/engine.rs`):
1. `search_worker` (around line 41)
2. `select_move` (around line 94)
3. Test `negamax_returns_zero_for_stalemate` (around line 581)
4. Test `negamax_detects_checkmate` (around line 597)
5. Test `search_context_shared_nodes_accumulates_across_search` (around line 614)
6. Test `quiescence_search_in_check_skips_stand_pat` (around line 680)

Any test I may have miscounted: look for every `SearchContext {` literal in the file and update it.

- [ ] **Step 5: Run the new test**

Run: `cargo test -p turbowhale new_search_context_has_empty_killers_and_zero_history`
Expected: PASS.

- [ ] **Step 6: Run the full suite to make sure nothing regressed**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 7: Add the LMR reduction table — failing test first**

Add to the `mod tests` block:

```rust
#[test]
fn reduction_table_matches_log_formula() {
    // Hand-computed sanity checks:
    // At depth=1, move_index=1 → ln(1)*ln(1)=0 → reduction 0
    // At depth=3, move_index=3 → ln(3)*ln(3)/2.25 ≈ 0.5365 → rounds to 1
    // At depth=8, move_index=16 → ln(8)*ln(16)/2.25 ≈ 2.563 → rounds to 3
    let table = reduction_table();
    assert_eq!(table[1][1], 0);
    assert_eq!(table[3][3], 1);
    assert_eq!(table[8][16], 3);
    // Table must never produce a reduction larger than depth-1 at indices we'll use
    // (depth >= 3, move_index <= 63). Spot-check the corner.
    assert!(table[3][63] as u32 <= 3 - 1);
}
```

- [ ] **Step 8: Run the test — it must fail to compile**

Run: `cargo test -p turbowhale reduction_table_matches_log_formula`
Expected: compile error, "cannot find function `reduction_table`".

- [ ] **Step 9: Add the reduction_table() function**

At the top of `src/engine.rs` add the import:

```rust
use std::sync::OnceLock;
```

Add this function after the existing constants (after `NULL_MOVE_REDUCTION`):

```rust
fn reduction_table() -> &'static [[u8; 64]; 64] {
    static TABLE: OnceLock<[[u8; 64]; 64]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [[0u8; 64]; 64];
        for depth in 1..64 {
            for move_index in 1..64 {
                let value = ((depth as f64).ln() * (move_index as f64).ln()) / 2.25;
                table[depth][move_index] = value.round().max(0.0) as u8;
            }
        }
        table
    })
}
```

- [ ] **Step 10: Run the reduction-table test**

Run: `cargo test -p turbowhale reduction_table_matches_log_formula`
Expected: PASS.

- [ ] **Step 11: Run `cargo clippy` and fix any warnings you introduced**

Run: `cargo clippy -- -D warnings`
Expected: clean (ignore warnings that pre-existed on master; fix only ones you introduced).

- [ ] **Step 12: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "feat(engine): add killer/history fields and LMR reduction table

Plumbs the data structures needed for killer moves, history heuristic,
and late move reductions. No search behaviour change yet — fields are
initialized but not read or written during search." \
    --changes <id-for-src/engine.rs> \
    --status-after
```

---

## Task 2: Rewrite move ordering with five-tier priority

**Files:**
- Modify: `src/engine.rs` — `order_moves` function (currently around lines 482-493) and its callers
- Test: new unit tests in the `mod tests` block

- [ ] **Step 1: Write failing test for TT-move-first ordering**

Add to the `mod tests` block:

```rust
#[test]
fn order_moves_puts_tt_move_first_even_over_captures() {
    // Starting position. Pick some legal non-capture move; assert it sorts
    // ahead of captures when passed as the TT move.
    let position = start_position();
    let legal = generate_legal_moves(&position);
    let e2e4 = legal.iter().find(|m| m.from_square == 12 && m.to_square == 28).copied().unwrap();
    let killers = [[None; 2]; MAX_SEARCH_PLY];
    let history = [[[0i32; 64]; 64]; 2];
    let ordered = order_moves(legal, &position, Some(e2e4), 0, &killers, &history);
    assert_eq!(ordered[0], e2e4, "TT move must be first");
}

#[test]
fn order_moves_puts_killers_before_other_quiets() {
    let position = start_position();
    let legal = generate_legal_moves(&position);
    let b1c3 = legal.iter().find(|m| m.from_square == 1 && m.to_square == 18).copied().unwrap();
    let mut killers = [[None; 2]; MAX_SEARCH_PLY];
    killers[0][0] = Some(b1c3);
    let history = [[[0i32; 64]; 64]; 2];
    let ordered = order_moves(legal, &position, None, 0, &killers, &history);
    // Find index of the killer in the ordered list
    let killer_index = ordered.iter().position(|m| *m == b1c3).unwrap();
    // All moves before it must be captures (none in startpos) — in startpos there are
    // no captures, so the killer must be first.
    assert_eq!(killer_index, 0, "killer must be first among quiets when no captures exist");
}

#[test]
fn order_moves_sorts_quiets_by_history_descending() {
    let position = start_position();
    let legal = generate_legal_moves(&position);
    let e2e4 = legal.iter().find(|m| m.from_square == 12 && m.to_square == 28).copied().unwrap();
    let d2d4 = legal.iter().find(|m| m.from_square == 11 && m.to_square == 27).copied().unwrap();
    let killers = [[None; 2]; MAX_SEARCH_PLY];
    let mut history = [[[0i32; 64]; 64]; 2];
    // White side_to_move -> index 0.
    history[0][11][27] = 500;  // d2->d4 high history
    history[0][12][28] = 100;  // e2->e4 low history
    let ordered = order_moves(legal, &position, None, 0, &killers, &history);
    let d2d4_index = ordered.iter().position(|m| *m == d2d4).unwrap();
    let e2e4_index = ordered.iter().position(|m| *m == e2e4).unwrap();
    assert!(d2d4_index < e2e4_index, "higher-history quiet must come earlier");
}
```

- [ ] **Step 2: Run the tests — they must fail to compile**

Run: `cargo test -p turbowhale order_moves_`
Expected: compile errors complaining about the new `order_moves` signature.

- [ ] **Step 3: Rewrite `order_moves`**

Replace the existing `order_moves` function (currently around lines 482-493) with:

```rust
fn order_moves(
    mut moves: Vec<Move>,
    position: &Position,
    tt_best_move: Option<Move>,
    ply: u32,
    killer_moves: &[[Option<Move>; 2]; MAX_SEARCH_PLY],
    history_scores: &[[[i32; 64]; 64]; 2],
) -> Vec<Move> {
    let ply_index = (ply as usize).min(MAX_SEARCH_PLY - 1);
    let killer1 = killer_moves[ply_index][0];
    let killer2 = killer_moves[ply_index][1];
    let color_index = position.side_to_move as usize;

    moves.sort_by_cached_key(|&chess_move| {
        if Some(chess_move) == tt_best_move {
            return i32::MIN;
        }
        if is_capture(chess_move, position) {
            return -10_000_000 - mvv_lva_score(position, chess_move);
        }
        if Some(chess_move) == killer1 {
            return -1_000_000;
        }
        if Some(chess_move) == killer2 {
            return -999_999;
        }
        -history_scores[color_index][chess_move.from_square as usize][chess_move.to_square as usize]
    });
    moves
}
```

- [ ] **Step 4: Update the single current caller of `order_moves` in `negamax_pvs`**

Find the line (currently around line 290):

```rust
let ordered_moves = order_moves(legal_moves, position, tt_best_move);
```

Change it to:

```rust
let ordered_moves = order_moves(
    legal_moves, position, tt_best_move, ply, &context.killer_moves, &context.history_scores,
);
```

- [ ] **Step 5: Run the new ordering tests**

Run: `cargo test -p turbowhale order_moves_`
Expected: all three PASS.

- [ ] **Step 6: Run the full suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "refactor(engine): five-tier move ordering with killers and history

order_moves now takes ply, killers, and history_scores. Priority:
TT move → captures (MVV-LVA) → killer 1 → killer 2 → quiets by history.
No change to search logic yet — killers/history are still all empty, so
behaviour is unchanged vs baseline." \
    --changes <id-for-src/engine.rs> \
    --status-after
```

---

## Task 3: Update killers and history on quiet-move beta cutoffs

**Files:**
- Modify: `src/engine.rs` — the beta-cutoff branch of `negamax_pvs` (currently around lines 308-317)
- Test: new unit tests

- [ ] **Step 1: Write failing tests for killer/history updates**

Add to the `mod tests` block. These use a real search on a tactical position with known quiet-move cutoffs.

```rust
#[test]
fn quiet_beta_cutoff_stores_killer_at_ply() {
    // Run a shallow search from the start position; killer slots at some ply
    // must be populated (there are many fail-high events at depth >= 3).
    let position = start_position();
    let mut context = SearchContext {
        transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
        stop_flag: Arc::new(AtomicBool::new(false)),
        shared_nodes: Arc::new(AtomicU64::new(0)),
        limits: SearchLimits::Depth(4),
        start_time: Instant::now(),
        nodes_searched: 0,
        killer_moves: [[None; 2]; MAX_SEARCH_PLY],
        history_scores: [[[0i32; 64]; 64]; 2],
    };
    negamax_pvs(&position, 4, -INF, INF, 0, &mut context);
    let any_killer_set = context.killer_moves.iter()
        .any(|slots| slots[0].is_some() || slots[1].is_some());
    assert!(any_killer_set, "at depth 4 from startpos at least one killer must be stored");
}

#[test]
fn quiet_beta_cutoff_increments_history() {
    let position = start_position();
    let mut context = SearchContext {
        transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
        stop_flag: Arc::new(AtomicBool::new(false)),
        shared_nodes: Arc::new(AtomicU64::new(0)),
        limits: SearchLimits::Depth(4),
        start_time: Instant::now(),
        nodes_searched: 0,
        killer_moves: [[None; 2]; MAX_SEARCH_PLY],
        history_scores: [[[0i32; 64]; 64]; 2],
    };
    negamax_pvs(&position, 4, -INF, INF, 0, &mut context);
    let any_history_nonzero = context.history_scores.iter().flatten().flatten().any(|&v| v > 0);
    assert!(any_history_nonzero, "at depth 4 from startpos history must be written somewhere");
}

#[test]
fn history_saturates_at_ceiling() {
    // Direct test of saturation logic — craft a context with history near cap
    // and verify the update clamps at 16384.
    let mut history_scores = [[[0i32; 64]; 64]; 2];
    history_scores[0][12][28] = 16_380;
    // Simulate the increment that the engine performs at depth 10.
    let bonus = 10 * 10;
    let entry = &mut history_scores[0][12][28];
    *entry = (*entry + bonus).min(16384);
    assert_eq!(history_scores[0][12][28], 16384);
}
```

- [ ] **Step 2: Run the tests — the first two will fail (no killer/history updates yet)**

Run: `cargo test -p turbowhale quiet_beta_cutoff history_saturates`
Expected: `quiet_beta_cutoff_stores_killer_at_ply` and `quiet_beta_cutoff_increments_history` FAIL (killers empty, history all zero). `history_saturates_at_ceiling` passes because it tests pure arithmetic.

- [ ] **Step 3: Update the beta-cutoff branch of `negamax_pvs`**

In `src/engine.rs`, find the block (currently around lines 308-317):

```rust
if score >= beta {
    context.transposition_table.store(position_hash, TtEntry {
        hash: position_hash,
        depth: depth as u8,
        score: beta,
        best_move: *chess_move,
        node_type: NodeType::LowerBound,
    });
    return beta;
}
```

Replace it with:

```rust
if score >= beta {
    let chess_move_value = *chess_move;
    let is_quiet = !is_capture(chess_move_value, position)
                && chess_move_value.promotion_piece.is_none();
    if is_quiet && (ply as usize) < MAX_SEARCH_PLY {
        let ply_index = ply as usize;
        if context.killer_moves[ply_index][0] != Some(chess_move_value) {
            context.killer_moves[ply_index][1] = context.killer_moves[ply_index][0];
            context.killer_moves[ply_index][0] = Some(chess_move_value);
        }
        let color_index = position.side_to_move as usize;
        let from_index = chess_move_value.from_square as usize;
        let to_index = chess_move_value.to_square as usize;
        let bonus = (depth * depth) as i32;
        let entry = &mut context.history_scores[color_index][from_index][to_index];
        *entry = (*entry + bonus).min(16384);
    }
    context.transposition_table.store(position_hash, TtEntry {
        hash: position_hash,
        depth: depth as u8,
        score: beta,
        best_move: chess_move_value,
        node_type: NodeType::LowerBound,
    });
    return beta;
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p turbowhale quiet_beta_cutoff history_saturates`
Expected: all three PASS.

- [ ] **Step 5: Run the full suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 6: Add a test that captures do NOT populate killers**

```rust
#[test]
fn capture_beta_cutoff_leaves_killers_empty_at_that_ply() {
    // Position where the only sensible cutoff at ply 0 is a capture.
    // White rook on a5, free black queen on e5 (undefended). Search depth 2.
    let position = crate::board::from_fen("4k3/8/8/R3q3/8/8/8/4K3 w - - 0 1");
    let mut context = SearchContext {
        transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
        stop_flag: Arc::new(AtomicBool::new(false)),
        shared_nodes: Arc::new(AtomicU64::new(0)),
        limits: SearchLimits::Depth(2),
        start_time: Instant::now(),
        nodes_searched: 0,
        killer_moves: [[None; 2]; MAX_SEARCH_PLY],
        history_scores: [[[0i32; 64]; 64]; 2],
    };
    negamax_pvs(&position, 2, -INF, INF, 0, &mut context);
    // The capture Rxe5 is the best move and should cause the cutoff at ply 0.
    // It is a capture → killers[0] must remain empty.
    assert!(context.killer_moves[0][0].is_none(), "captures must not populate killers");
    assert!(context.killer_moves[0][1].is_none(), "captures must not populate killers");
}
```

- [ ] **Step 7: Run it**

Run: `cargo test -p turbowhale capture_beta_cutoff_leaves_killers_empty_at_that_ply`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "feat(engine): populate killers and history on quiet beta cutoffs

On a fail-high from a quiet (non-capture, non-promotion) move, shift
the existing killer into slot 2 and store the new move in slot 1, then
increment history[color][from][to] by depth² (saturated at 16384).
Captures and promotions are excluded — they already sort by MVV-LVA
and tainting killers with them is a well-known anti-pattern." \
    --changes <id-for-src/engine.rs> \
    --status-after
```

---

## Task 4: Integrate late move reductions into the search loop

**Files:**
- Modify: `src/engine.rs` — the main move loop in `negamax_pvs` (currently around lines 290-322)
- Test: new unit tests

- [ ] **Step 1: Write failing tests for LMR correctness**

Add to `mod tests`:

```rust
#[test]
fn lmr_preserves_mate_in_one() {
    // Same position as select_move_finds_mate_in_one — but via the LMR-aware search.
    let position = crate::board::from_fen("6k1/8/6KQ/8/8/8/8/8 w - - 0 1");
    let tt = Arc::new(ShardedTranspositionTable::new(4));
    let stop = Arc::new(AtomicBool::new(false));
    let params = GoParameters { depth: Some(3), ..Default::default() };
    let chosen = select_move(&position, &params, tt, stop, 1);
    let after = crate::board::apply_move(&position, chosen);
    let opponent_moves = generate_legal_moves(&after);
    assert!(opponent_moves.is_empty(), "LMR must not mask mate-in-one");
}

#[test]
fn lmr_preserves_hanging_queen_capture() {
    let position = crate::board::from_fen("4k3/8/8/R3q3/8/8/8/4K3 w - - 0 1");
    let tt = Arc::new(ShardedTranspositionTable::new(4));
    let stop = Arc::new(AtomicBool::new(false));
    let params = GoParameters { depth: Some(3), ..Default::default() };
    let chosen = select_move(&position, &params, tt, stop, 1);
    assert_eq!(chosen.to_square, 36, "LMR must not hide the queen capture on e5 (sq 36)");
}

#[test]
fn lmr_in_check_node_still_finds_evasion() {
    // White in check — LMR must be disabled at this node. A mis-implemented LMR
    // can miss the evasion.
    let position = crate::board::from_fen("4q3/7k/8/8/8/8/8/4K3 w - - 0 1");
    let tt = Arc::new(ShardedTranspositionTable::new(4));
    let stop = Arc::new(AtomicBool::new(false));
    let params = GoParameters { depth: Some(4), ..Default::default() };
    let chosen = select_move(&position, &params, tt, stop, 1);
    let legal = generate_legal_moves(&position);
    assert!(legal.contains(&chosen), "LMR must return a legal evasion when in check");
}
```

- [ ] **Step 2: Run the tests — they currently pass (LMR not yet active) but will be the regression guard after the change**

Run: `cargo test -p turbowhale lmr_`
Expected: all PASS (acts as a canary: these tests prove *equivalence* once LMR is added).

- [ ] **Step 3: Rewrite the main move loop to use LMR**

In `src/engine.rs`, find the block starting at `let mut best_move = ordered_moves[0];` (around line 291) and ending at the post-loop TT store (around line 322). Replace the entire loop body — everything from `for chess_move in &ordered_moves {` through the bottom of the for-loop (i.e. the line before `let node_type = if alpha > alpha_original { ... };`).

Replace the loop with:

```rust
let mut best_move = ordered_moves[0];

for (move_index, chess_move) in ordered_moves.iter().enumerate() {
    let chess_move_value = *chess_move;
    let child_position = crate::board::apply_move(position, chess_move_value);

    let is_quiet = !is_capture(chess_move_value, position)
                && chess_move_value.promotion_piece.is_none();
    let ply_index_for_killer = (ply as usize).min(MAX_SEARCH_PLY - 1);
    let is_killer = context.killer_moves[ply_index_for_killer][0] == Some(chess_move_value)
                 || context.killer_moves[ply_index_for_killer][1] == Some(chess_move_value);

    let score = if move_index == 0 {
        -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
    } else {
        let reduction: u32 = if depth >= 3
                             && move_index >= 3
                             && !is_in_check
                             && is_quiet
                             && !is_killer {
            let depth_index = (depth as usize).min(63);
            let move_index_clamped = move_index.min(63);
            reduction_table()[depth_index][move_index_clamped] as u32
        } else {
            0
        };

        let reduced_depth = (depth - 1).saturating_sub(reduction);
        let reduced_score = -negamax_pvs(
            &child_position, reduced_depth, -alpha - 1, -alpha, ply + 1, context,
        );

        let null_window_score = if reduction > 0 && reduced_score > alpha {
            -negamax_pvs(&child_position, depth - 1, -alpha - 1, -alpha, ply + 1, context)
        } else {
            reduced_score
        };

        if null_window_score > alpha && null_window_score < beta && beta - alpha > 1 {
            -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
        } else {
            null_window_score
        }
    };

    if score >= beta {
        let is_quiet_cutoff = !is_capture(chess_move_value, position)
                           && chess_move_value.promotion_piece.is_none();
        if is_quiet_cutoff && (ply as usize) < MAX_SEARCH_PLY {
            let ply_index = ply as usize;
            if context.killer_moves[ply_index][0] != Some(chess_move_value) {
                context.killer_moves[ply_index][1] = context.killer_moves[ply_index][0];
                context.killer_moves[ply_index][0] = Some(chess_move_value);
            }
            let color_index = position.side_to_move as usize;
            let from_index = chess_move_value.from_square as usize;
            let to_index = chess_move_value.to_square as usize;
            let bonus = (depth * depth) as i32;
            let entry = &mut context.history_scores[color_index][from_index][to_index];
            *entry = (*entry + bonus).min(16384);
        }
        context.transposition_table.store(position_hash, TtEntry {
            hash: position_hash,
            depth: depth as u8,
            score: beta,
            best_move: chess_move_value,
            node_type: NodeType::LowerBound,
        });
        return beta;
    }
    if score > alpha {
        alpha = score;
        best_move = chess_move_value;
    }
}
```

Note: the beta-cutoff block from Task 3 is absorbed into the loop above (same logic, plus `is_quiet` is recomputed here as `is_quiet_cutoff` for clarity — it matches `is_quiet` computed at the top of each iteration but we recompute because the borrow situation differs). Also delete the old `let mut first_move = true;` line — it's no longer used.

- [ ] **Step 4: Run the LMR tests**

Run: `cargo test -p turbowhale lmr_`
Expected: all PASS.

- [ ] **Step 5: Run the full suite**

Run: `cargo test`
Expected: all tests pass, including mate-finding, check evasion, stalemate, and null-move Zugzwang regression tests.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: clean (fix any warnings you introduced).

- [ ] **Step 7: Smoke-test the UCI binary at a realistic depth**

Run:

```bash
cargo build --release
echo -e "position startpos\ngo depth 8\nquit" | ./target/release/turbowhale
```

Expected: engine responds with `info depth 1` ... `info depth 8` and a final `bestmove <move>` line. Verify it completes in under ~5 seconds (LMR must reduce the node count substantially vs baseline).

- [ ] **Step 8: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "feat(engine): late move reductions with re-search on fail-high

Reduce null-window depth for moves after the first three when the node
is not in check, the move is quiet (not a capture or promotion), and
the move is not a killer. Reduction is looked up in the precomputed
(ln(d)*ln(i))/2.25 table. If a reduced search fails high, re-search
at full depth with a null window; if that result is inside the aspiration
window, re-search with a full window for PV correctness." \
    --changes <id-for-src/engine.rs> \
    --status-after
```

---

## Task 5: Node-count regression guard

**Files:**
- Modify: `src/engine.rs` — add one integration-style test

- [ ] **Step 1: Measure the current node count on a fixed benchmark**

Run the engine on three well-known positions at depth 7 (release build) and record the node counts from the last `info` line. Positions (FENs):

- **startpos:** `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1`
- **kiwipete:** `r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1`
- **position 3 (endgame):** `8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1`

Command for each:

```bash
cargo build --release
echo -e "position fen <FEN>\ngo depth 7\nquit" | ./target/release/turbowhale | tail -3
```

Record the `nodes` value from the final `info depth 7 ...` line for each position. Sum them. Note this number — call it `TOTAL_NODES_DEPTH_7`.

- [ ] **Step 2: Write the regression guard test**

Add to `mod tests` (replace `<total>` with the value measured above):

```rust
#[test]
fn search_node_budget_regression_at_depth_7() {
    // Captured after LMR+killers+history were wired in. If this test starts
    // failing, investigate whether pruning has regressed (e.g. an off-by-one
    // in LMR guards disabling reductions) before bumping the ceiling.
    const CEILING: u64 = <total> * 115 / 100; // allow 15% headroom for noise
    let fens = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    ];
    let mut total_nodes: u64 = 0;
    for fen in fens.iter() {
        let position = crate::board::from_fen(fen);
        let mut context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::new(AtomicU64::new(0)),
            limits: SearchLimits::Depth(7),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        for depth in 1..=7 {
            negamax_pvs(&position, depth, -INF, INF, 0, &mut context);
        }
        total_nodes += context.shared_nodes.load(Ordering::Relaxed) + context.nodes_searched;
    }
    assert!(
        total_nodes <= CEILING,
        "search explored {} nodes vs ceiling {} — pruning regression?",
        total_nodes, CEILING,
    );
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p turbowhale search_node_budget_regression_at_depth_7 --release`
Expected: PASS. (Use `--release` — depth 7 on three positions is slow in debug.)

- [ ] **Step 4: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "test(engine): add node-count regression guard at depth 7

Captures the current node budget across three benchmark positions
(startpos, kiwipete, endgame position 3) at depth 7. Fails if any
future change silently disables LMR pruning." \
    --changes <id-for-src/engine.rs> \
    --status-after
```

---

## Task 6: Gitignore downloaded engines

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Add the engines directory to .gitignore**

Current contents of `.gitignore`:

```
/target
.env
```

Append `/engines` so the file reads:

```
/target
.env
/engines
```

- [ ] **Step 2: Create the directory with a `.gitkeep`-free sanity check**

Run:

```bash
mkdir -p engines
touch engines/.test-probe && rm engines/.test-probe
```

Confirm `but status -fv` does **not** show anything inside `engines/` as a change.

- [ ] **Step 3: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "chore: gitignore /engines/ — self-play baseline binaries" \
    --changes <id-for-.gitignore> \
    --status-after
```

---

## Task 7: Ship a small opening book

**Files:**
- Create: `scripts/openings.epd`

- [ ] **Step 1: Create the opening book file**

```bash
mkdir -p scripts
```

Create `scripts/openings.epd` with the content below. Each line is an EPD (FEN without the move counters, followed by a semicolon-delimited comment). fastchess parses the FEN portion and plays both sides from that position.

```
# turbowhale opening book — 30 well-known positions used to reduce
# self-play variance from repeated openings.
# Each line: <FEN> ; <opening name>
rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - ; King's Pawn
rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - ; Sicilian
rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - ; Scandinavian
rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - ; King's Knight
rnbqkb1r/pppp1ppp/5n2/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - ; Petroff
r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - ; Italian/Spanish base
r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - ; Ruy Lopez
r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R b KQkq - ; Italian Game
rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - ; Sicilian Open
rnbqkb1r/pp2pppp/3p1n2/8/3NP3/8/PPP2PPP/RNBQKB1R w KQkq - ; Najdorf setup
rnbqkbnr/pp2pppp/3p4/8/3NP3/8/PPP2PPP/RNBQKB1R b KQkq - ; Sicilian Open Najdorf
rnbqkbnr/pppp1ppp/8/4p3/2B1P3/8/PPPP1PPP/RNBQK1NR b KQkq - ; Bishop's Opening
rnbqkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - ; Queen's Pawn
rnbqkbnr/pp2pppp/8/2pp4/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - ; Slav Declined
rnbqkb1r/pppppppp/5n2/8/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - ; Indian Defence
rnbqkb1r/pppppp1p/5np1/8/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - ; King's Indian
rnbqkb1r/pppppp1p/5np1/8/2PP4/8/PP2PPPP/RNBQKBNR w KQkq - ; King's Indian (c4)
rnbqkb1r/pp1ppppp/5n2/2p5/2P5/8/PP1PPPPP/RNBQKBNR w KQkq - ; English Symmetric
rnbqkb1r/pppppppp/5n2/8/2P5/8/PP1PPPPP/RNBQKBNR b KQkq - ; English
rnbqkbnr/ppp2ppp/4p3/3p4/2PP4/8/PP2PPPP/RNBQKBNR w KQkq - ; QGD
rnbqkb1r/ppp1pppp/5n2/3p4/2PP4/8/PP2PPPP/RNBQKBNR w KQkq - ; Semi-Slav base
rnbqkbnr/pp1ppppp/8/8/3pP3/8/PPP2PPP/RNBQKBNR w KQkq - ; Centre Counter
rnbqkbnr/pppp1ppp/8/4p3/3PP3/8/PPP2PPP/RNBQKBNR b KQkq - ; Centre Game
rnbqkbnr/pp1ppppp/8/2p5/2P5/8/PP1PPPPP/RNBQKBNR w KQkq - ; English/Symmetric
rnbqkbnr/pppp1ppp/8/4p3/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - ; Englund
rnbqkb1r/pppppppp/5n2/8/3P4/5N2/PPP1PPPP/RNBQKB1R b KQkq - ; Queen's Pawn Knight
rnbqkb1r/ppp1pppp/5n2/3p4/3P4/5N2/PPP1PPPP/RNBQKB1R w KQkq - ; London base
rnbqk1nr/pppp1ppp/8/2b1p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - ; Italian defence
r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/2N2N2/PPPP1PPP/R1BQK2R b KQkq - ; Italian four knights
rnbqkbnr/pp1p1ppp/4p3/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - ; Sicilian Kan setup
```

- [ ] **Step 2: Confirm fastchess can parse it**

Only run this step if `fastchess` is already installed locally; otherwise skip and rely on the selfplay script's own error reporting. If installed:

```bash
fastchess --version
```

Expected: prints a version string.

- [ ] **Step 3: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "chore: ship 30-position opening book for self-play" \
    --changes <id-for-scripts/openings.epd> \
    --status-after
```

---

## Task 8: Self-play script

**Files:**
- Create: `scripts/selfplay.sh`

- [ ] **Step 1: Write the script**

Create `scripts/selfplay.sh`:

```bash
#!/usr/bin/env bash
# Usage: ./scripts/selfplay.sh <baseline_tag> [games] [tc]
#   baseline_tag: release version without leading "v" (e.g. 1.4.0)
#   games:        default 500 (total, split into rounds of 2 with color swap)
#   tc:           default "10+0.1"
set -euo pipefail

tag="${1:?need baseline tag, e.g. 1.4.0}"
games="${2:-500}"
tc="${3:-10+0.1}"

repo="3nabla3/turbowhale"

arch="$(uname -m)"
case "$arch" in
    x86_64|aarch64) ;;
    *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
esac

case "$(uname -s)" in
    Linux)  os="linux"  ;;
    Darwin) os="macos"  ;;
    *) echo "Unsupported OS: $(uname -s) — script supports Linux and macOS" >&2; exit 1 ;;
esac

mkdir -p engines
baseline="engines/turbowhale-v${tag}"
if [[ ! -x "$baseline" ]]; then
    asset="turbowhale-v${tag}-${arch}-${os}"
    url="https://github.com/${repo}/releases/download/v${tag}/${asset}"
    echo "Downloading $asset from $url ..."
    if ! curl -fL -o "$baseline" "$url"; then
        echo "Failed to download $url — check that the release exists for this platform." >&2
        rm -f "$baseline"
        exit 1
    fi
    chmod +x "$baseline"
fi

echo "Building challenger from working tree ..."
cargo build --release
challenger="$(pwd)/target/release/turbowhale"

if ! command -v fastchess >/dev/null 2>&1; then
    echo "fastchess not found on PATH — install from https://github.com/Disservin/fastchess" >&2
    exit 1
fi

rounds=$(( games / 2 ))
concurrency="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)"

fastchess \
    -engine cmd="$baseline"   name="v${tag}" \
    -engine cmd="$challenger" name="dev" \
    -each tc="$tc" proto=uci \
    -rounds "$rounds" -games 2 -repeat \
    -openings file=scripts/openings.epd format=epd order=random \
    -sprt elo0=0 elo1=10 alpha=0.05 beta=0.05 \
    -concurrency "$concurrency" \
    -pgnout selfplay.pgn
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x scripts/selfplay.sh`

- [ ] **Step 3: Smoke-test the argument parsing and error paths**

Run: `./scripts/selfplay.sh` (no args)
Expected: non-zero exit with a message about `need baseline tag`.

Run: `./scripts/selfplay.sh 999.999.999 2 1+0`
Expected: prints "Downloading ..." then fails with the curl 404 diagnostic and non-zero exit. `engines/turbowhale-v999.999.999` must not be left behind.

- [ ] **Step 4: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "feat(scripts): selfplay.sh — local dev vs tagged release binary

Downloads the baseline from https://github.com/3nabla3/turbowhale/releases
into ./engines/, builds the challenger via cargo build --release, and
runs a fastchess SPRT match using scripts/openings.epd. Arch/OS autodetected
via uname; Linux and macOS only. Windows users should run fastchess manually." \
    --changes <id-for-scripts/selfplay.sh> \
    --status-after
```

---

## Task 9: README update

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace README contents**

Replace `README.md` with:

```markdown
# Turbowhale

Turbowhale is a UCI compatible chess engine developed with Claude AI. This project aims to evaluate the performance of AI-generated code in the context of chess engine development.

## Features
- UCI (Universal Chess Interface) compatibility
- Alpha-beta search with PVS, transposition table, null move pruning, late move reductions
- Killer-move and history-heuristic quiet move ordering
- Tapered PeSTO evaluation
- Lazy SMP multi-threading
- Built entirely with Claude AI code generation

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
```

- [ ] **Step 2: Commit**

```bash
but status -fv
but commit feat/lmr-killers-history \
    -m "docs(readme): document self-play harness and updated feature list" \
    --changes <id-for-README.md> \
    --status-after
```

---

## Task 10: Final verification

- [ ] **Step 1: Clean build + all tests**

Run:

```bash
cargo clean
cargo build --release
cargo test
cargo clippy -- -D warnings
```

Expected: clean build, all tests pass, no clippy warnings in files you touched.

- [ ] **Step 2: End-to-end UCI smoke test**

Run:

```bash
echo -e "uci\nposition startpos\ngo depth 10\nquit" | ./target/release/turbowhale
```

Expected: `uciok`, `info` lines up to `depth 10`, and a `bestmove` line.

- [ ] **Step 3: If fastchess is installed, run a short self-play match against v1.4.0**

Only if `fastchess` is installed:

```bash
./scripts/selfplay.sh 1.4.0 20 2+0.05
```

Expected: a short match completes (20 games at 2s+0.05s) and reports an Elo estimate in the console output. The SPRT result may be inconclusive at only 20 games — that is fine; we are smoke-testing the pipeline, not the result.

If `fastchess` is not installed, skip this step and document in the handoff that the user should run the match themselves.

- [ ] **Step 4: Push the branch**

```bash
but push feat/lmr-killers-history
```

Expected: branch is pushed to origin. The branch is ready for review / merge.

---

## Summary

After all tasks complete, the branch contains:

- **Search:** killer moves (2 per ply), color-indexed butterfly history, LMR with re-search, five-tier move ordering.
- **Tests:** 10+ new unit tests covering ordering, cutoff heuristic updates, LMR correctness, and a node-count regression guard at depth 7.
- **Infra:** `scripts/selfplay.sh`, `scripts/openings.epd`, updated `.gitignore`, updated `README.md`.

The self-play script is the verification tool — run it against the latest release (currently `1.4.0`) with a reasonable game count to measure the ELO gain.
