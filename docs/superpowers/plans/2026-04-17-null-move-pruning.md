# Null Move Pruning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add null move pruning to `negamax_pvs` to prune subtrees where even giving the opponent a free move fails to save them, and restructure the node entry order so legal move generation happens after the TT probe.

**Architecture:** Add a cheap `is_in_check` detection step at the top of `negamax_pvs` using `is_square_attacked`, move legal move generation to after the TT probe, pass `is_in_check` into `quiescence_search` to avoid recomputation, and insert a null move attempt before move generation that prunes when the reduced-depth null-window search exceeds beta.

**Tech Stack:** Rust, `src/engine.rs` only.

---

## File Map

| File | Change |
|------|--------|
| `src/engine.rs` | Add `NULL_MOVE_REDUCTION` constant; restructure `negamax_pvs` node entry order; add `is_in_check: bool` parameter to `quiescence_search`; add null move pruning block |

---

### Task 1: Write failing tests

**Files:**
- Modify: `src/engine.rs` — add tests inside `mod tests`

The test `quiescence_search_in_check_skips_stand_pat` calls `quiescence_search` with the new 6-argument signature. It will **fail to compile** until Task 2 adds the `is_in_check` parameter — that is the intentional failing state.

- [ ] **Step 1: Add three tests inside `mod tests` in `src/engine.rs`**

Add the following three tests at the bottom of the `mod tests` block (before the closing `}`):

```rust
#[test]
fn quiescence_search_in_check_skips_stand_pat() {
    // White king on e1, black queen on e8 — white is in check on the e-file.
    // With is_in_check=true the stand-pat branch is skipped.
    // The king has legal evasions so the score must not be a mate value.
    let position = from_fen("4q3/8/8/8/8/8/8/4K3 w - - 0 1");
    let mut context = SearchContext {
        transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
        stop_flag: Arc::new(AtomicBool::new(false)),
        shared_nodes: Arc::new(AtomicU64::new(0)),
        limits: SearchLimits::Depth(1),
        start_time: Instant::now(),
        nodes_searched: 0,
    };
    let score = quiescence_search(&position, -INF, INF, 0, true, &mut context);
    assert!(score > -MATE_SCORE / 2, "king has evasions — score must not be a mate loss, got {}", score);
}

#[test]
fn select_move_returns_legal_move_when_in_check() {
    // White king on e1, black queen on e8 — white is in check, must find an evasion.
    // Null move must not fire here.
    let position = from_fen("4q3/8/8/8/8/8/8/4K3 w - - 0 1");
    let tt = make_tt();
    let stop = make_stop();
    let params = GoParameters { depth: Some(3), ..Default::default() };
    let chosen = select_move(&position, &params, tt, stop, 1);
    let legal_moves = generate_legal_moves(&position);
    assert!(legal_moves.contains(&chosen), "must return a legal evasion when in check");
}

#[test]
fn select_move_returns_legal_move_in_king_and_pawn_endgame() {
    // Only kings and pawns — null move must not fire (Zugzwang guard).
    let position = from_fen("4k3/4p3/8/8/8/8/4P3/4K3 w - - 0 1");
    let tt = make_tt();
    let stop = make_stop();
    let params = GoParameters { depth: Some(4), ..Default::default() };
    let chosen = select_move(&position, &params, tt, stop, 1);
    let legal_moves = generate_legal_moves(&position);
    assert!(legal_moves.contains(&chosen), "must return a legal move in king-and-pawn endgame");
}
```

- [ ] **Step 2: Run tests to verify the compile failure**

```bash
cargo test quiescence_search_in_check_skips_stand_pat 2>&1 | head -20
```

Expected: compile error — `quiescence_search` takes 5 arguments but 6 were supplied (or similar).

---

### Task 2: Restructure `negamax_pvs` and update `quiescence_search` signature

**Files:**
- Modify: `src/engine.rs:188-386`

This task:
1. Adds `is_in_check: bool` parameter to `quiescence_search`.
2. Removes the duplicate `is_square_attacked` call inside `quiescence_search` (lines 334–335 currently).
3. Makes recursive `quiescence_search` calls compute `child_in_check` before recursing.
4. In `negamax_pvs`: adds cheap `is_in_check` at the top, moves legal move generation to after the TT probe, updates the `depth == 0` call to pass `is_in_check`.

- [ ] **Step 1: Replace `quiescence_search` with the new signature**

Replace the entire `quiescence_search` function (currently lines 308–386) with:

```rust
fn quiescence_search(
    position: &Position,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    is_in_check: bool,
    context: &mut SearchContext,
) -> i32 {
    if context.stop_flag.load(Ordering::Relaxed) {
        return 0;
    }

    context.nodes_searched += 1;
    if context.nodes_searched.is_multiple_of(1024) {
        context.shared_nodes.fetch_add(1024, Ordering::Relaxed);
        context.nodes_searched = 0;
        let over_time = match &context.limits {
            SearchLimits::MoveTime(duration) => context.start_time.elapsed() >= *duration,
            SearchLimits::Clock { budget }   => context.start_time.elapsed() >= *budget,
            SearchLimits::Depth(_) | SearchLimits::Infinite => false,
        };
        if over_time {
            context.stop_flag.store(true, Ordering::Release);
            return 0;
        }
    }

    if !is_in_check {
        let stand_pat = evaluate(position);
        if stand_pat >= beta {
            return beta;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }
    }

    let pseudo_legal = generate_pseudo_legal_moves(position);
    let mut candidate_moves: Vec<Move> = pseudo_legal
        .into_iter()
        .filter(|&chess_move| {
            if is_in_check { true } else { is_capture(chess_move, position) }
        })
        .collect();

    candidate_moves.sort_by_cached_key(|&chess_move| {
        if is_capture(chess_move, position) {
            -mvv_lva_score(position, chess_move)
        } else {
            0
        }
    });

    let mut legal_move_count = 0;
    for chess_move in candidate_moves {
        let child_position = crate::board::apply_move(position, chess_move);
        let moving_king_square = child_position.king_square(position.side_to_move);
        if is_square_attacked(moving_king_square, position.side_to_move.opponent(), &child_position) {
            continue;
        }
        legal_move_count += 1;

        let child_king_square = child_position.king_square(child_position.side_to_move);
        let child_in_check = is_square_attacked(
            child_king_square,
            child_position.side_to_move.opponent(),
            &child_position,
        );
        let score = -quiescence_search(&child_position, -beta, -alpha, ply + 1, child_in_check, context);
        if score >= beta {
            return beta;
        }
        if score > alpha {
            alpha = score;
        }
    }

    if is_in_check && legal_move_count == 0 {
        return -(MATE_SCORE - ply as i32);
    }

    alpha
}
```

- [ ] **Step 2: Restructure `negamax_pvs`**

Replace the entire `negamax_pvs` function (currently lines 188–306) with:

```rust
fn negamax_pvs(
    position: &Position,
    depth: u32,
    mut alpha: i32,
    mut beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
    if context.stop_flag.load(Ordering::Relaxed) {
        return 0;
    }

    context.nodes_searched += 1;
    if context.nodes_searched.is_multiple_of(1024) {
        context.shared_nodes.fetch_add(1024, Ordering::Relaxed);
        context.nodes_searched = 0;
        let over_time = match &context.limits {
            SearchLimits::MoveTime(duration) => context.start_time.elapsed() >= *duration,
            SearchLimits::Clock { budget }   => context.start_time.elapsed() >= *budget,
            SearchLimits::Depth(_) | SearchLimits::Infinite => false,
        };
        if over_time {
            context.stop_flag.store(true, Ordering::Release);
            return 0;
        }
    }

    if position.halfmove_clock >= 100 {
        return 0;
    }

    let king_square = position.king_square(position.side_to_move);
    let is_in_check = is_square_attacked(king_square, position.side_to_move.opponent(), position);

    if depth == 0 {
        return quiescence_search(position, alpha, beta, ply, is_in_check, context);
    }

    let position_hash = compute_hash(position);
    let alpha_original = alpha;
    let mut tt_best_move: Option<Move> = None;

    if let Some(tt_entry) = context.transposition_table.probe(position_hash) {
        tt_best_move = Some(tt_entry.best_move);
        if tt_entry.depth >= depth as u8 {
            match tt_entry.node_type {
                NodeType::Exact => return tt_entry.score,
                NodeType::LowerBound => {
                    if tt_entry.score > alpha {
                        alpha = tt_entry.score;
                    }
                }
                NodeType::UpperBound => {
                    if tt_entry.score < beta {
                        beta = tt_entry.score;
                    }
                }
            }
            if alpha >= beta {
                return tt_entry.score;
            }
        }
    }

    let legal_moves = generate_legal_moves(position);
    if legal_moves.is_empty() {
        return if is_in_check {
            -(MATE_SCORE - ply as i32)
        } else {
            0
        };
    }

    let ordered_moves = order_moves(legal_moves, position, tt_best_move);
    let mut best_move = ordered_moves[0];
    let mut first_move = true;

    for chess_move in &ordered_moves {
        let child_position = crate::board::apply_move(position, *chess_move);
        let score = if first_move {
            first_move = false;
            -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
        } else {
            let null_window_score = -negamax_pvs(&child_position, depth - 1, -alpha - 1, -alpha, ply + 1, context);
            if null_window_score > alpha && null_window_score < beta && beta - alpha > 1 {
                -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
            } else {
                null_window_score
            }
        };

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
        if score > alpha {
            alpha = score;
            best_move = *chess_move;
        }
    }

    let node_type = if alpha > alpha_original { NodeType::Exact } else { NodeType::UpperBound };
    context.transposition_table.store(position_hash, TtEntry {
        hash: position_hash,
        depth: depth as u8,
        score: alpha,
        best_move,
        node_type,
    });

    alpha
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: all tests pass (the three new tests now compile). `quiescence_search_in_check_skips_stand_pat` should pass.

- [ ] **Step 4: Commit**

Use GitButler (`but`) to commit the changes to `src/engine.rs` with message:
`refactor(engine): add is_in_check to quiescence_search, move legal move gen after TT probe`

---

### Task 3: Add `NULL_MOVE_REDUCTION` constant and null move pruning block

**Files:**
- Modify: `src/engine.rs` — add constant near top, insert pruning block in `negamax_pvs`

- [ ] **Step 1: Add `NULL_MOVE_REDUCTION` constant**

After the existing constants at the top of `src/engine.rs`:

```rust
pub const MATE_SCORE: i32 = 100_000;
const INF: i32 = 200_000;
```

Add:

```rust
const NULL_MOVE_REDUCTION: u32 = 2;
```

- [ ] **Step 2: Insert null move pruning block in `negamax_pvs`**

In the restructured `negamax_pvs` from Task 2, insert the null move block between the TT probe and the `generate_legal_moves` call. The insertion point is after the `if alpha >= beta { return tt_entry.score; }` line and before `let legal_moves = generate_legal_moves(position);`.

Add this block:

```rust
    // Null move pruning: if the position is so good that even passing our turn
    // fails to let the opponent recover, prune this subtree.
    if !is_in_check && ply > 0 && depth >= NULL_MOVE_REDUCTION + 1 {
        let has_non_pawn_non_king_piece = match position.side_to_move {
            Color::White => (position.white_knights | position.white_bishops
                           | position.white_rooks  | position.white_queens) != 0,
            Color::Black => (position.black_knights | position.black_bishops
                           | position.black_rooks  | position.black_queens) != 0,
        };
        if has_non_pawn_non_king_piece {
            let mut null_position = position.clone();
            null_position.side_to_move = position.side_to_move.opponent();
            null_position.en_passant_square = None;
            null_position.halfmove_clock += 1;
            let null_score = -negamax_pvs(
                &null_position,
                depth - 1 - NULL_MOVE_REDUCTION,
                -beta,
                -beta + 1,
                ply + 1,
                context,
            );
            if null_score >= beta {
                return beta;
            }
        }
    }
```

- [ ] **Step 3: Run all tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: all tests pass, including `select_move_finds_mate_in_one`, `select_move_captures_hanging_queen`, `select_move_returns_legal_move_when_in_check`, `select_move_returns_legal_move_in_king_and_pawn_endgame`.

- [ ] **Step 4: Run clippy**

```bash
cargo clippy 2>&1 | grep -E "^error|^warning" | head -20
```

Expected: no errors. Address any warnings before committing.

- [ ] **Step 5: Commit**

Use GitButler (`but`) to commit the changes to `src/engine.rs` with message:
`feat(engine): add null move pruning with Zugzwang guard (R=2)`
