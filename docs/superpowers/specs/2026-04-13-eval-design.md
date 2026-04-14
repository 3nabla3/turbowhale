# Static Evaluation Improvement Design

**Date:** 2026-04-13  
**Status:** Approved

## Overview

Replace the current material-only `evaluate` function with a classical hand-crafted evaluation combining:

- Piece-square tables (PST) with tapered middlegame/endgame blending, maintained incrementally on `Position`
- Pawn structure: doubled, isolated, and passed pawn detection
- Mobility: reachable squares per piece type, weighted by game phase
- King safety: pawn shield and open files near the king

All evaluation sub-functions return scores from **White's perspective** (positive = good for White, negative = good for Black). The single perspective flip to the side-to-move perspective happens once, at the end of the public `evaluate` function.

---

## Architecture

### Chosen Approach

**Incremental PST + Lazy Full Eval**

- `Position` stores three incrementally-maintained fields updated on every `apply_move`: `middlegame_score`, `endgame_score`, `game_phase`
- `evaluate` computes a cheap tapered score from these fields first
- If that score is already outside the search window by more than `LAZY_EVALUATION_THRESHOLD`, it returns early without computing the expensive features
- Otherwise it computes pawn structure, mobility, and king safety and adds them

This avoids recomputing PST scores at every node while keeping the expensive features correct for positions that matter.

---

## Section 1: Position Struct Changes (`board.rs`)

Three new fields added to `Position`:

```rust
pub middlegame_score: i32,  // PST score, white minus black, middlegame weights
pub endgame_score: i32,     // PST score, white minus black, endgame weights
pub game_phase: i32,        // 0 = full endgame, 24 = full opening material
```

**Phase weights per piece (standard PeSTO values):**

| Piece  | Phase weight |
|--------|-------------|
| Pawn   | 0           |
| Knight | 1           |
| Bishop | 1           |
| Rook   | 2           |
| Queen  | 4           |

Maximum phase = 24 (2 queens × 4 + 4 rooks × 2 + 4 bishops × 1 + 4 knights × 1).

**`apply_move` changes:**

A private helper `update_piece_square_scores(piece: PieceType, color: Color, square: usize, sign: i32)` is called with `sign = +1` on piece placement and `sign = -1` on piece removal. It updates `middlegame_score`, `endgame_score`, and `game_phase` by the appropriate table values. All existing move cases (captures, en passant, castling, promotion) already touch specific squares — each gets the corresponding `update_piece_square_scores` calls inserted.

**`from_fen` and `start_position`:** Compute `middlegame_score`, `endgame_score`, and `game_phase` from scratch by iterating all occupied squares.

---

## Section 2: Piece-Square Tables (`eval.rs`)

Twelve `[i32; 64]` constants — one middlegame and one endgame table per piece type (pawn, knight, bishop, rook, queen, king). Values use the **PeSTO tables**, which are well-tested and widely used in open-source engines.

Square indexing matches the codebase: a1=0, h1=7, a8=56, h8=63. Tables are defined from White's perspective. Black's value for square `s` uses the vertical mirror: `table[s ^ 56]`.

**Tapered score formula:**

```
tapered_score = ((middlegame_score * game_phase) + (endgame_score * (24 - game_phase))) / 24
```

**Lazy evaluation:**

```rust
const LAZY_EVALUATION_THRESHOLD: i32 = 50; // centipawns
```

If `tapered_score.abs() > LAZY_EVALUATION_THRESHOLD`, return early.

---

## Section 3: Pawn Structure (`eval.rs`)

Function signature:

```rust
/// Returns a score in centipawns from White's perspective:
/// positive values favour White, negative values favour Black.
fn evaluate_pawn_structure(position: &Position) -> i32
```

**Doubled pawns** — for each of 8 file masks, count friendly pawns on that file. If more than one, penalise `(count - 1) * DOUBLED_PAWN_PENALTY`. Computed for both sides; result is white penalty minus black penalty.

**Isolated pawns** — for each pawn, check the two adjacent-file masks. If both are empty of friendly pawns, the pawn is isolated and incurs `ISOLATED_PAWN_PENALTY`.

**Passed pawns** — for a white pawn on square `s`, the forward span mask covers all squares on the same and adjacent files with rank > rank(s). If `forward_span_mask & black_pawns == 0`, the pawn is passed and earns a `PASSED_PAWN_RANK_BONUS[rank]` bonus (7-entry table, higher ranks earn more).

**Constants:**

```rust
const DOUBLED_PAWN_PENALTY: i32 = 10;   // centipawns per extra pawn on a file
const ISOLATED_PAWN_PENALTY: i32 = 15;  // centipawns per isolated pawn
// PASSED_PAWN_RANK_BONUS[0..7]: [0, 10, 20, 35, 55, 80, 110] (rank 0 unused)
```

**Precomputed masks** (64-entry or 8-entry arrays of `u64` constants in `eval.rs`):
- `FILE_MASKS: [u64; 8]`
- `ADJACENT_FILE_MASKS: [u64; 8]`
- `WHITE_FORWARD_SPAN_MASKS: [u64; 64]`
- `BLACK_FORWARD_SPAN_MASKS: [u64; 64]`

---

## Section 4: Mobility (`eval.rs`)

Function signature:

```rust
/// Returns (middlegame_bonus, endgame_bonus) in centipawns from White's perspective:
/// positive values favour White, negative values favour Black.
fn evaluate_mobility(position: &Position) -> (i32, i32)
```

For each non-pawn, non-king piece, count reachable squares not occupied by a friendly piece using the existing `movegen` attack functions:

- **Knights**: `knight_attacks_for_square(square) & !own_occupancy`
- **Bishops**: `bishop_attacks(square, all_occupancy) & !own_occupancy`
- **Rooks**: `rook_attacks(square, all_occupancy) & !own_occupancy`
- **Queens**: `queen_attacks(square, all_occupancy) & !own_occupancy`

**Per-square bonuses:**

| Piece  | Middlegame bonus | Endgame bonus |
|--------|-----------------|---------------|
| Knight | 4               | 2             |
| Bishop | 3               | 3             |
| Rook   | 2               | 4             |
| Queen  | 1               | 2             |

**Constants:**

```rust
const KNIGHT_MOBILITY_MIDDLEGAME_BONUS: i32 = 4;
const KNIGHT_MOBILITY_ENDGAME_BONUS: i32 = 2;
const BISHOP_MOBILITY_MIDDLEGAME_BONUS: i32 = 3;
const BISHOP_MOBILITY_ENDGAME_BONUS: i32 = 3;
const ROOK_MOBILITY_MIDDLEGAME_BONUS: i32 = 2;
const ROOK_MOBILITY_ENDGAME_BONUS: i32 = 4;
const QUEEN_MOBILITY_MIDDLEGAME_BONUS: i32 = 1;
const QUEEN_MOBILITY_ENDGAME_BONUS: i32 = 2;
```

---

## Section 5: King Safety (`eval.rs`)

Function signature:

```rust
/// Returns a score in centipawns from White's perspective:
/// positive values favour White, negative values favour Black.
/// King safety is a middlegame concern only; the returned score is scaled
/// by game_phase / 24 so it fades to zero as the endgame approaches.
fn evaluate_king_safety(position: &Position) -> i32
```

**Pawn shield** — the shield zone for a king on square `s` is `KING_SHIELD_MASKS[s]`, a precomputed bitmask of the 3 squares directly in front plus the 3 squares one rank further. Each missing friendly pawn in the shield zone incurs `PAWN_SHIELD_PENALTY`.

**Open files near king** — for each file the king occupies or is adjacent to:
- If no friendly pawns on that file: `KING_OPEN_FILE_PENALTY`
- If friendly pawns but no enemy pawns: `KING_SEMI_OPEN_FILE_PENALTY`

**Constants:**

```rust
const PAWN_SHIELD_PENALTY: i32 = 15;         // per missing shield pawn
const KING_OPEN_FILE_PENALTY: i32 = 25;      // per open file adjacent to king
const KING_SEMI_OPEN_FILE_PENALTY: i32 = 10; // per semi-open file adjacent to king
```

**Precomputed mask:**

```rust
const KING_SHIELD_MASKS: [u64; 64]  // shield zone per king square, White's perspective
                                     // mirrored with ^ 56 for Black
```

---

## Section 6: The `evaluate` Function

```rust
/// Returns a score in centipawns from the perspective of the side to move:
/// positive values favour the side to move.
/// This is the only function that performs the perspective flip.
pub fn evaluate(position: &Position) -> i32 {
    // 1. Cheap tapered score from incrementally-maintained PST fields
    let tapered_score = ((position.middlegame_score * position.game_phase)
        + (position.endgame_score * (24 - position.game_phase)))
        / 24;

    // 2. Lazy evaluation: skip expensive features if score is already decisive
    if tapered_score.abs() > LAZY_EVALUATION_THRESHOLD {
        return match position.side_to_move {
            Color::White => tapered_score,
            Color::Black => -tapered_score,
        };
    }

    // 3. Expensive features (pawn structure, mobility, king safety)
    let pawn_score = evaluate_pawn_structure(position);
    let (mobility_middlegame, mobility_endgame) = evaluate_mobility(position);
    let mobility_score = ((mobility_middlegame * position.game_phase)
        + (mobility_endgame * (24 - position.game_phase)))
        / 24;
    let king_safety_score = evaluate_king_safety(position);

    // 4. Combine and orient to side to move
    let absolute_score = tapered_score + pawn_score + mobility_score + king_safety_score;
    match position.side_to_move {
        Color::White => absolute_score,
        Color::Black => -absolute_score,
    }
}
```

---

## Files Changed

| File | Change |
|------|--------|
| `src/board.rs` | Add `middlegame_score`, `endgame_score`, `game_phase` to `Position`; add `update_piece_square_scores` calls in `apply_move`; initialise fields in `from_fen` and `start_position` |
| `src/eval.rs` | Replace material-only eval with PST tables, tapered eval, `evaluate_pawn_structure`, `evaluate_mobility`, `evaluate_king_safety`, precomputed masks, all constants |

No new modules. No changes to `movegen`, `engine`, `tt`, or `uci`.

---

## Testing

Existing eval tests remain valid (material balance, start position = 0, perspective flip). New tests:

- Passed pawn on rank 7 scores higher than passed pawn on rank 3
- Knight on d4 scores higher than knight on a1
- King with broken pawn shield scores lower than king with intact shield
- Start position mobility is symmetric (score = 0)
- Tapered eval blends correctly at phase 0, 12, and 24
