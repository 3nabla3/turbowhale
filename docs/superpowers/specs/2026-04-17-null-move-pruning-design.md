# Null Move Pruning Design

**Date:** 2026-04-17
**Status:** Approved

## Overview

Add null move pruning to `negamax_pvs` in `src/engine.rs`. Null move pruning skips subtrees where even giving the opponent a free move fails to save them — if a reduced-depth search with a null move still exceeds beta, the position is too good for the opponent and the node can be pruned.

## Changes to `negamax_pvs` Node Entry Order

The current order generates legal moves before the TT probe, which is wasteful. This change restructures the node entry to move legal move generation later and adds a cheap in-check detection step.

**New order:**

1. Stop flag / node count check
2. Halfmove clock draw detection (`halfmove_clock >= 100`)
3. Cheap in-check detection via `is_square_attacked` — result passed forward to avoid recomputation
4. Depth == 0 → `quiescence_search` (receives `is_in_check` to skip the stand-pat when in check)
5. TT probe — may return early
6. **Null move pruning** ← new
7. Legal move generation (`generate_legal_moves`)
8. Checkmate / stalemate detection (`legal_moves.is_empty()`)
9. PVS loop (unchanged)

## Null Move Pruning

### Tunable Constant

```rust
const NULL_MOVE_REDUCTION: u32 = 2;
```

Defined at the top of `engine.rs` alongside `MATE_SCORE` and `INF`. Changing this one value adjusts the aggressiveness of null move pruning.

### Preconditions (all must hold)

- `depth >= NULL_MOVE_REDUCTION + 1` — ensures remaining depth after reduction is at least 1
- `!is_in_check` — null move while in check is illegal and unsound
- `ply > 0` — skip at the root to avoid distorting root move selection
- Side to move has at least one non-pawn, non-king piece — guards against Zugzwang in pure pawn/king endings where passing might genuinely be the worst move

The Zugzwang guard is a bitboard check:

```rust
let has_non_pawn_non_king_piece = match position.side_to_move {
    Color::White => position.white_knights | position.white_bishops
                  | position.white_rooks  | position.white_queens != 0,
    Color::Black => position.black_knights | position.black_bishops
                  | position.black_rooks  | position.black_queens != 0,
};
```

### Making the Null Move

A null move flips the side to move without moving any piece:

```rust
let mut null_position = position.clone();
null_position.side_to_move = position.side_to_move.opponent();
null_position.en_passant_square = None;
null_position.halfmove_clock += 1;
```

No call to `recompute_occupancy` is needed (no pieces changed). The null position is **never stored in the TT**.

### Search and Pruning

```rust
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
```

A null window `[-beta, -beta+1]` is used — we only need to know whether the score exceeds beta, not what the exact score is.

## What Does Not Change

- `quiescence_search` — the `is_in_check` flag is passed in as a parameter to avoid recomputing it, but the logic is otherwise unchanged.
- Move generation, board representation, TT, evaluation — untouched.
- `search_worker` — calls `negamax_pvs` directly; benefits automatically.

## Testing Strategy

- All existing engine tests must continue to pass.
- Add a test: a position where null move pruning is expected to fire (side has major pieces, clearly winning position) returns the same best move as without null move pruning at depth 4.
- Add a test: null move pruning does not fire when in check (verify by constructing a check position and confirming the search completes without panicking or returning 0 spuriously).
- Add a test: null move pruning does not fire in a king-and-pawns-only endgame (Zugzwang guard).
- Manual verification: `go depth 10` from startpos should be measurably faster with null move enabled vs disabled (compare node counts).
