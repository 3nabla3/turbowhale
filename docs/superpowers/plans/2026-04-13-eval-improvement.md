# Static Evaluation Improvement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the material-only `evaluate` function with a classical hand-crafted evaluation using PeSTO piece-square tables (maintained incrementally on `Position`), pawn structure penalties/bonuses, mobility, and king safety.

**Architecture:** `Position` stores `middlegame_score`, `endgame_score`, and `game_phase` fields maintained incrementally in `apply_move`. `evaluate` computes a tapered blend of these, then applies lazy evaluation: if the PST score is already decisive, skip the more expensive pawn structure, mobility, and king safety sub-functions.

**Tech Stack:** Rust 1.94, no new dependencies. Reuses existing `movegen` attack functions for mobility.

**Spec:** `docs/superpowers/specs/2026-04-13-eval-design.md`

---

## Table Indexing Convention

All PST tables in this plan are stored in **a1=0 order** (matching the codebase's square indexing: a1=0, b1=1, …, h1=7, a2=8, …, h8=63).

- **White** piece on square `s` → `table[s]`
- **Black** piece on square `s` → `table[s ^ 56]` (vertical mirror)

`middlegame_score` and `endgame_score` are always **White minus Black** (positive = good for White).

---

## File Map

| File | Changes |
|------|---------|
| `src/eval.rs` | Complete rewrite: PST tables, material constants, precomputed masks, all evaluation sub-functions |
| `src/board.rs` | Add 3 fields to `Position`; add `update_piece_square_scores` method; update `apply_move`, `from_fen`, `start_position`, `empty` |

---

## Task 1: Add PeSTO tables and material constants to eval.rs

**Files:**
- Modify: `src/eval.rs`

These are pure data — no logic changes yet. The tables are PeSTO positional bonuses (centipawns, a1=0 order). Material values are separate constants added alongside the positional bonus during score updates.

- [ ] **Step 1: Write a failing test that will pass once the tables exist**

Add to `src/eval.rs` `#[cfg(test)]` block:

```rust
#[test]
fn knight_positional_bonus_prefers_center_over_corner_in_middlegame() {
    // d4=27, a1=0
    assert!(
        MIDDLEGAME_PIECE_SQUARE_TABLES[PieceType::Knight as usize][27]
            > MIDDLEGAME_PIECE_SQUARE_TABLES[PieceType::Knight as usize][0],
        "knight on d4 should have higher middlegame bonus than knight on a1"
    );
}
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cargo test -p turbowhale -- eval::tests::knight_positional_bonus_prefers_center_over_corner_in_middlegame
```

Expected: compile error (`MIDDLEGAME_PIECE_SQUARE_TABLES` not defined).

- [ ] **Step 3: Replace the existing material constants and add PST tables**

Replace the entire contents of `src/eval.rs` up to the `evaluate` function with the following. Keep existing `evaluate` function and tests intact for now.

```rust
use crate::board::{Color, PieceType, Position};

// ---------------------------------------------------------------------------
// Material values (centipawns) — separate from positional bonuses.
// Source: PeSTO by Ronald Friederich (public domain).
// ---------------------------------------------------------------------------

pub const MIDDLEGAME_MATERIAL_VALUES: [i32; 6] = [
    82,    // Pawn
    337,   // Knight
    365,   // Bishop
    477,   // Rook
    1025,  // Queen
    0,     // King (king has no material value)
];

pub const ENDGAME_MATERIAL_VALUES: [i32; 6] = [
    94,    // Pawn
    281,   // Knight
    297,   // Bishop
    512,   // Rook
    936,   // Queen
    0,     // King
];

// Phase weights: how much each piece contributes to the game phase counter.
// Max phase = 24 (full opening material on the board).
pub const PIECE_PHASE_VALUES: [i32; 6] = [
    0,  // Pawn
    1,  // Knight
    1,  // Bishop
    2,  // Rook
    4,  // Queen
    0,  // King
];

pub const MAX_GAME_PHASE: i32 = 24;

// ---------------------------------------------------------------------------
// Piece-square tables (positional bonuses only, centipawns).
// Stored in a1=0 order (a1=index 0, h8=index 63).
// White uses table[square]; Black uses table[square ^ 56].
// Source: PeSTO by Ronald Friederich (public domain).
// ---------------------------------------------------------------------------

pub const MIDDLEGAME_PIECE_SQUARE_TABLES: [[i32; 64]; 6] = [
    // Pawn
    [
          0,   0,   0,   0,   0,   0,   0,   0,
        -35,  -1, -20, -23, -15,  24,  38, -22,
        -26,  -4,  -4, -10,   3,   3,  33, -12,
        -27,  -2,  -5,  12,  17,   6,  10, -25,
        -14,  13,   6,  21,  23,  12,  17, -23,
         -6,   7,  26,  31,  65,  56,  25, -20,
         98, 134,  61,  95,  68, 126,  34, -11,
          0,   0,   0,   0,   0,   0,   0,   0,
    ],
    // Knight
    [
       -105, -21, -58, -33, -17, -28, -19, -23,
        -29, -53, -12,  -3,  -1,  18, -14, -19,
        -23,  -9,  12,  10,  19,  17,  25, -16,
        -13,   4,  16,  13,  28,  19,  21,  -8,
         -9,  17,  19,  53,  37,  69,  18,  22,
        -47,  60,  37,  65,  84, 129,  73,  44,
        -73, -41,  72,  36,  23,  62,   7, -17,
       -167, -89, -34, -49,  61, -97, -15,-107,
    ],
    // Bishop
    [
        -33,  -3, -14, -21, -13, -12, -39, -21,
          4,  15,  16,   0,   7,  21,  33,   1,
          0,  15,  15,  15,  14,  27,  18,  10,
         -6,  13,  13,  26,  34,  12,  10,   4,
         -4,   5,  19,  50,  37,  37,   7,  -2,
        -16,  37,  43,  40,  35,  50,  37,  -2,
        -26,  16, -18, -13,  30,  59,  18, -47,
        -29,   4, -82, -37, -25, -42,   7,  -8,
    ],
    // Rook
    [
        -19, -13,   1,  17,  16,   7, -37, -26,
        -44, -16, -20,  -9,  -1,  11,  -6, -71,
        -45, -25, -16, -17,   3,   0,  -5, -33,
        -36, -26, -12,  -1,   9,  -7,   6, -23,
        -24, -11,   7,  26,  24,  35,  -8, -20,
         -5,  19,  26,  36,  17,  45,  61,  16,
         27,  32,  58,  62,  80,  67,  26,  44,
         32,  42,  32,  51,  63,   9,  31,  43,
    ],
    // Queen
    [
         -1, -18,  -9,  10, -15, -25, -31, -50,
        -35,  -8,  11,   2,   8,  15,  -3,   1,
        -14,   2, -11,  -2,  -5,   2,  14,   5,
         -9, -26,  -9, -10,  -2,  -4,   3,  -3,
        -27, -27, -16, -16,  -1,  17,  -2,   1,
        -13, -17,   7,   8,  29,  56,  47,  57,
        -24, -39,  -5,   1, -16,  57,  28,  54,
        -28,   0,  29,  12,  59,  44,  43,  45,
    ],
    // King
    [
        -15,  36,  12, -54,   8, -28,  24,  14,
          1,   7,  -8, -64, -43, -16,   9,   8,
        -14, -14, -22, -46, -44, -30, -15, -27,
        -49,  -1, -27, -39, -46, -44, -33, -51,
        -17, -20, -12, -27, -30, -25, -14, -36,
         -9,  24,   2, -16, -20,   6,  22, -22,
         29,  -1, -20,  -7,  -8,  -4, -38, -29,
        -65,  23,  16, -15, -56, -34,   2,  13,
    ],
];

pub const ENDGAME_PIECE_SQUARE_TABLES: [[i32; 64]; 6] = [
    // Pawn
    [
          0,   0,   0,   0,   0,   0,   0,   0,
         13,   8,   8,  10,  13,   0,   2,  -7,
          4,   7,  -6,   1,   0,  -5,  -1,  -8,
         13,   9,  -3,  -7,  -7,  -8,   3,  -1,
         32,  24,  13,   5,  -2,   4,  17,  17,
         94, 100,  85,  67,  56,  53,  82,  84,
        178, 173, 158, 134, 147, 132, 165, 187,
          0,   0,   0,   0,   0,   0,   0,   0,
    ],
    // Knight
    [
        -29, -51, -23, -15, -22, -18, -50, -64,
        -42, -20, -10,  -5,  -2, -20, -23, -44,
        -23,  -3,  -1,  15,  10,  -3, -20, -22,
        -18,  -6,  16,  25,  16,  17,   4, -18,
        -17,   3,  22,  22,  22,  11,   8, -18,
        -24, -20,  10,   9,  -1,  -9, -19, -41,
        -25,  -8, -25,  -2,  -9, -25, -24, -52,
        -58, -38, -13, -28, -31, -27, -63, -99,
    ],
    // Bishop
    [
        -23,  -9, -23,  -5,  -9, -16,  -5, -17,
        -14, -18,  -7,  -1,   4,  -9, -15, -27,
        -12,  -3,   8,  10,  13,   3,  -7, -15,
         -6,   3,  13,  19,   7,  10,  -3,  -9,
         -3,   9,  12,   9,  14,  10,   3,   2,
          2,  -8,   0,  -1,  -2,   6,   0,   4,
         -8,  -4,   7, -12,  -3, -13,  -4, -14,
        -14, -21, -11,  -8,  -7,  -9, -17, -24,
    ],
    // Rook
    [
         -9,   2,   3,  -1,  -5, -13,   4, -20,
         -6,  -6,   0,   2,  -9,  -9, -11,  -3,
         -4,   0,  -5,  -1,  -7, -12,  -8, -16,
          3,   5,   8,   4,  -5,  -6,  -8, -11,
          4,   3,  13,   1,   2,   1,  -1,   2,
          7,   7,   7,   5,   4,  -3,  -5,  -3,
         11,  13,  13,  11,  -3,   3,   8,   3,
         13,  10,  18,  15,  12,  12,   8,   5,
    ],
    // Queen
    [
        -33, -28, -22, -43,  -5, -32, -20, -41,
        -22, -23, -30, -16, -16, -23, -36, -32,
        -16, -27,  15,   6,   9,  17,  10,   5,
        -18,  28,  19,  47,  31,  34,  39,  23,
          3,  22,  24,  45,  57,  40,  57,  36,
        -20,   6,   9,  49,  47,  35,  19,   9,
        -17,  20,  32,  41,  58,  25,  30,   0,
         -9,  22,  22,  27,  27,  19,  10,  20,
    ],
    // King
    [
        -53, -34, -21, -11, -28, -14, -24, -43,
        -27, -11,   4,  13,  14,   4,  -5, -17,
        -19,  -3,  11,  21,  23,  16,   7,  -9,
        -18,  -4,  21,  24,  27,  23,   9, -11,
         -8,  22,  24,  27,  26,  33,  26,   3,
         10,  17,  23,  15,  20,  45,  44,  13,
        -12,  17,  14,  17,  17,  38,  23,  11,
        -74, -35, -18, -18, -11,  15,   4, -17,
    ],
];
```

- [ ] **Step 4: Run the test to confirm it passes**

```bash
cargo test -p turbowhale -- eval::tests::knight_positional_bonus_prefers_center_over_corner_in_middlegame
```

Expected: PASS.

- [ ] **Step 5: Run all existing tests to confirm nothing is broken**

```bash
cargo test -p turbowhale
```

Expected: all pass (no logic changed yet).

- [ ] **Step 6: Commit**

```
docs/data: add PeSTO piece-square tables and material constants to eval.rs
```

---

## Task 2: Add incremental PST fields to Position

**Files:**
- Modify: `src/board.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` block in `src/board.rs`:

```rust
#[test]
fn start_position_incremental_scores_are_zero() {
    let position = start_position();
    // Start position is perfectly symmetric — both sides have identical material and placement
    assert_eq!(position.middlegame_score, 0);
    assert_eq!(position.endgame_score, 0);
}

#[test]
fn start_position_game_phase_is_max() {
    let position = start_position();
    // Full set of pieces: 2 queens × 4 + 4 rooks × 2 + 4 bishops × 1 + 4 knights × 1 = 24
    assert_eq!(position.game_phase, 24);
}

#[test]
fn lone_white_knight_on_d4_has_positive_middlegame_score() {
    // White knight on d4 (square 27), no other pieces except kings
    let position = from_fen("4k3/8/8/8/3N4/8/8/4K3 w - - 0 1");
    // Knight on d4 contributes material (337) + positional bonus (positive for center)
    assert!(position.middlegame_score > 0, "white knight on d4 should give positive middlegame score");
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p turbowhale -- board::tests::start_position_incremental_scores_are_zero board::tests::start_position_game_phase_is_max board::tests::lone_white_knight_on_d4_has_positive_middlegame_score
```

Expected: compile errors (fields don't exist yet).

- [ ] **Step 3: Add the three fields to Position**

In `src/board.rs`, add to the `Position` struct after `fullmove_number`:

```rust
/// Incrementally-maintained middlegame PST score (White minus Black, centipawns).
/// Updated by apply_move on every move; initialised by from_fen and start_position.
pub middlegame_score: i32,
/// Incrementally-maintained endgame PST score (White minus Black, centipawns).
pub endgame_score: i32,
/// Current game phase (0 = full endgame, 24 = full opening material on the board).
pub game_phase: i32,
```

- [ ] **Step 4: Initialise fields to 0 in Position::empty()**

In the `empty()` method, add after `fullmove_number: 1`:

```rust
middlegame_score: 0,
endgame_score: 0,
game_phase: 0,
```

- [ ] **Step 5: Add a helper that computes scores from scratch by scanning all pieces**

Add after the `recompute_occupancy` method in the `impl Position` block:

```rust
/// Recomputes `middlegame_score`, `endgame_score`, and `game_phase` from scratch
/// by scanning all occupied squares. Called once by `from_fen` and `start_position`.
pub fn recompute_incremental_scores(&mut self) {
    use crate::eval::{
        ENDGAME_MATERIAL_VALUES, ENDGAME_PIECE_SQUARE_TABLES,
        MIDDLEGAME_MATERIAL_VALUES, MIDDLEGAME_PIECE_SQUARE_TABLES, PIECE_PHASE_VALUES,
    };

    self.middlegame_score = 0;
    self.endgame_score = 0;
    self.game_phase = 0;

    let piece_bitboards: [(PieceType, u64, Color); 12] = [
        (PieceType::Pawn,   self.white_pawns,   Color::White),
        (PieceType::Knight, self.white_knights, Color::White),
        (PieceType::Bishop, self.white_bishops, Color::White),
        (PieceType::Rook,   self.white_rooks,   Color::White),
        (PieceType::Queen,  self.white_queens,  Color::White),
        (PieceType::King,   self.white_king,    Color::White),
        (PieceType::Pawn,   self.black_pawns,   Color::Black),
        (PieceType::Knight, self.black_knights, Color::Black),
        (PieceType::Bishop, self.black_bishops, Color::Black),
        (PieceType::Rook,   self.black_rooks,   Color::Black),
        (PieceType::Queen,  self.black_queens,  Color::Black),
        (PieceType::King,   self.black_king,    Color::Black),
    ];

    for (piece_type, mut bitboard, color) in piece_bitboards {
        while bitboard != 0 {
            let square = bitboard.trailing_zeros() as usize;
            bitboard &= bitboard - 1;

            let piece_index = piece_type as usize;
            let lookup_square = match color {
                Color::White => square,
                Color::Black => square ^ 56,
            };
            let direction = match color {
                Color::White => 1,
                Color::Black => -1,
            };

            let middlegame_value = MIDDLEGAME_MATERIAL_VALUES[piece_index]
                + MIDDLEGAME_PIECE_SQUARE_TABLES[piece_index][lookup_square];
            let endgame_value = ENDGAME_MATERIAL_VALUES[piece_index]
                + ENDGAME_PIECE_SQUARE_TABLES[piece_index][lookup_square];

            self.middlegame_score += direction * middlegame_value;
            self.endgame_score += direction * endgame_value;
            self.game_phase += PIECE_PHASE_VALUES[piece_index];
        }
    }

    // Clamp game_phase to [0, MAX_GAME_PHASE] in case of unusual positions
    self.game_phase = self.game_phase.min(crate::eval::MAX_GAME_PHASE);
}
```

- [ ] **Step 6: Call recompute_incremental_scores at the end of from_fen**

In `from_fen`, after `position.recompute_occupancy();` and before `position`:

```rust
position.recompute_incremental_scores();
```

- [ ] **Step 7: Run tests to confirm they pass**

```bash
cargo test -p turbowhale -- board::tests::start_position_incremental_scores_are_zero board::tests::start_position_game_phase_is_max board::tests::lone_white_knight_on_d4_has_positive_middlegame_score
```

Expected: all PASS.

- [ ] **Step 8: Run all tests**

```bash
cargo test -p turbowhale
```

Expected: all pass.

- [ ] **Step 9: Commit**

```
feat: add incremental PST fields to Position, initialised from FEN
```

---

## Task 3: Maintain incremental scores through apply_move

**Files:**
- Modify: `src/board.rs`

- [ ] **Step 1: Write failing test**

Add to `#[cfg(test)]` in `src/board.rs`:

```rust
#[test]
fn apply_move_keeps_incremental_scores_consistent_with_recompute() {
    use crate::movegen::generate_legal_moves; // generate_legal_moves is not in scope via super::*
    let start = start_position();
    for chess_move in generate_legal_moves(&start) {
        let after_move = apply_move(&start, chess_move);
        let mut recomputed = after_move.clone();
        recomputed.recompute_incremental_scores();
        assert_eq!(
            after_move.middlegame_score, recomputed.middlegame_score,
            "middlegame_score inconsistent after move {:?}", chess_move
        );
        assert_eq!(
            after_move.endgame_score, recomputed.endgame_score,
            "endgame_score inconsistent after move {:?}", chess_move
        );
        assert_eq!(
            after_move.game_phase, recomputed.game_phase,
            "game_phase inconsistent after move {:?}", chess_move
        );
    }
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p turbowhale -- board::tests::apply_move_keeps_incremental_scores_consistent_with_recompute
```

Expected: FAIL (incremental scores are stale after apply_move — `empty()` initialises them to 0 and `clone()` copies the stale values).

- [ ] **Step 3: Add update_piece_square_scores method to Position**

Add to the `impl Position` block in `src/board.rs`:

```rust
/// Updates `middlegame_score`, `endgame_score`, and `game_phase` for a single piece
/// placement or removal.
///
/// `sign` must be `+1` when placing a piece on the board and `-1` when removing one.
/// `middlegame_score` and `endgame_score` are White-minus-Black: White placements
/// increase the score, Black placements decrease it.
fn update_piece_square_scores(
    &mut self,
    piece_type: PieceType,
    color: Color,
    square: usize,
    sign: i32,
) {
    use crate::eval::{
        ENDGAME_MATERIAL_VALUES, ENDGAME_PIECE_SQUARE_TABLES,
        MIDDLEGAME_MATERIAL_VALUES, MIDDLEGAME_PIECE_SQUARE_TABLES, PIECE_PHASE_VALUES,
    };

    let piece_index = piece_type as usize;
    let lookup_square = match color {
        Color::White => square,
        Color::Black => square ^ 56,
    };
    let direction = match color {
        Color::White => sign,
        Color::Black => -sign,
    };

    self.middlegame_score += direction
        * (MIDDLEGAME_MATERIAL_VALUES[piece_index]
            + MIDDLEGAME_PIECE_SQUARE_TABLES[piece_index][lookup_square]);
    self.endgame_score += direction
        * (ENDGAME_MATERIAL_VALUES[piece_index]
            + ENDGAME_PIECE_SQUARE_TABLES[piece_index][lookup_square]);
    self.game_phase += sign * PIECE_PHASE_VALUES[piece_index];
}
```

- [ ] **Step 4: Determine captured piece type before clearing it in apply_move**

In `apply_move`, before the block that clears enemy pieces from the destination square (the `if !chess_move.move_flags.contains(MoveFlags::EN_PASSANT)` block), add logic to detect what piece is being captured:

```rust
// Detect any captured piece at the destination before clearing it,
// so we can update the incremental PST scores.
let captured_piece_type: Option<PieceType> =
    if chess_move.move_flags.contains(MoveFlags::EN_PASSANT) {
        Some(PieceType::Pawn)
    } else {
        let opponent_pieces: [(PieceType, u64); 5] = match position.side_to_move {
            Color::White => [
                (PieceType::Pawn,   position.black_pawns),
                (PieceType::Knight, position.black_knights),
                (PieceType::Bishop, position.black_bishops),
                (PieceType::Rook,   position.black_rooks),
                (PieceType::Queen,  position.black_queens),
            ],
            Color::Black => [
                (PieceType::Pawn,   position.white_pawns),
                (PieceType::Knight, position.white_knights),
                (PieceType::Bishop, position.white_bishops),
                (PieceType::Rook,   position.white_rooks),
                (PieceType::Queen,  position.white_queens),
            ],
        };
        opponent_pieces
            .iter()
            .find(|(_, bitboard)| bitboard & to_bit != 0)
            .map(|(piece, _)| *piece)
    };
```

- [ ] **Step 5: Add PST update calls throughout apply_move**

After each of the four structural changes in `apply_move`, add the corresponding `update_piece_square_scores` call on `new_position`.

**a) After removing the moving piece from its source square** (after the first `match (position.side_to_move, moving_piece_type)` block):

```rust
new_position.update_piece_square_scores(
    moving_piece_type,
    position.side_to_move,
    chess_move.from_square as usize,
    -1,
);
```

**b) After clearing captured enemy pieces** (after the `if !chess_move.move_flags.contains(MoveFlags::EN_PASSANT)` block), if a capture occurred:

```rust
if let Some(piece_type) = captured_piece_type {
    if !chess_move.move_flags.contains(MoveFlags::EN_PASSANT) {
        new_position.update_piece_square_scores(
            piece_type,
            position.side_to_move.opponent(),
            chess_move.to_square as usize,
            -1,
        );
    }
}
```

**c) After placing the destination piece** (after the second `match (position.side_to_move, destination_piece_type)` block):

```rust
new_position.update_piece_square_scores(
    destination_piece_type,
    position.side_to_move,
    chess_move.to_square as usize,
    1,
);
```

**d) After the en passant pawn removal** (inside the `if chess_move.move_flags.contains(MoveFlags::EN_PASSANT)` block, after clearing the captured pawn):

```rust
new_position.update_piece_square_scores(
    PieceType::Pawn,
    position.side_to_move.opponent(),
    captured_pawn_square,
    -1,
);
```

**e) After each rook move in the castling block** — two calls, one removal and one placement:

```rust
// Inside the castling block, after moving the rook:
new_position.update_piece_square_scores(
    PieceType::Rook,
    position.side_to_move,
    rook_from_square as usize,
    -1,
);
new_position.update_piece_square_scores(
    PieceType::Rook,
    position.side_to_move,
    rook_to_square as usize,
    1,
);
```

- [ ] **Step 6: Clamp game_phase after all updates**

At the end of `apply_move`, before `new_position.recompute_occupancy()`:

```rust
new_position.game_phase = new_position.game_phase.min(crate::eval::MAX_GAME_PHASE).max(0);
```

- [ ] **Step 7: Run the consistency test**

```bash
cargo test -p turbowhale -- board::tests::apply_move_keeps_incremental_scores_consistent_with_recompute
```

Expected: PASS.

- [ ] **Step 8: Run all tests**

```bash
cargo test -p turbowhale
```

Expected: all pass.

- [ ] **Step 9: Commit**

```
feat: maintain incremental PST scores in apply_move
```

---

## Task 4: Replace evaluate with tapered PST evaluation

**Files:**
- Modify: `src/eval.rs`

- [ ] **Step 1: Add the new test cases**

Add to the `#[cfg(test)]` block in `src/eval.rs`:

```rust
#[test]
fn knight_on_d4_evaluates_higher_than_knight_on_a1_for_white() {
    // White knight on d4, kings only otherwise
    let knight_on_d4 = from_fen("4k3/8/8/8/3N4/8/8/4K3 w - - 0 1");
    let knight_on_a1 = from_fen("4k3/8/8/8/8/8/8/N3K3 w - - 0 1");
    assert!(
        evaluate(&knight_on_d4) > evaluate(&knight_on_a1),
        "knight on d4 should score better than knight on a1 for white"
    );
}

#[test]
fn tapered_eval_blends_toward_endgame_when_phase_is_zero() {
    // Kings only → phase=0 → pure endgame score
    let position = from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    let tapered = ((position.middlegame_score * position.game_phase)
        + (position.endgame_score * (crate::eval::MAX_GAME_PHASE - position.game_phase)))
        / crate::eval::MAX_GAME_PHASE;
    assert_eq!(evaluate(&position), tapered);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p turbowhale -- eval::tests::knight_on_d4_evaluates_higher_than_knight_on_a1_for_white eval::tests::tapered_eval_blends_toward_endgame_when_phase_is_zero
```

Expected: FAIL (evaluate still uses the old material-count logic).

- [ ] **Step 3: Replace the evaluate function and add the lazy eval constant**

Replace the `evaluate` function and the `count_material` function in `src/eval.rs` with:

```rust
/// Threshold (centipawns) above which the cheap tapered PST score is returned
/// without computing pawn structure, mobility, or king safety.
const LAZY_EVALUATION_THRESHOLD: i32 = 50;

/// Returns a score in centipawns from the perspective of the side to move:
/// positive values favour the side to move.
/// This is the only function that performs the perspective flip.
pub fn evaluate(position: &Position) -> i32 {
    // 1. Cheap tapered score from the incrementally-maintained PST fields.
    //    Blends from pure middlegame (game_phase=MAX) to pure endgame (game_phase=0).
    let tapered_score = ((position.middlegame_score * position.game_phase)
        + (position.endgame_score * (MAX_GAME_PHASE - position.game_phase)))
        / MAX_GAME_PHASE;

    // 2. Lazy evaluation: skip expensive sub-functions when the score is already
    //    outside a margin where positional features could change the outcome.
    if tapered_score.abs() > LAZY_EVALUATION_THRESHOLD {
        return match position.side_to_move {
            Color::White => tapered_score,
            Color::Black => -tapered_score,
        };
    }

    // 3. Expensive positional features — only reached when the position is close.
    //    (Implemented in later tasks; for now, return the tapered score.)
    let absolute_score = tapered_score;

    match position.side_to_move {
        Color::White => absolute_score,
        Color::Black => -absolute_score,
    }
}
```

Also remove the now-unused `PAWN_VALUE`, `KNIGHT_VALUE`, `BISHOP_VALUE`, `ROOK_VALUE`, `QUEEN_VALUE` constants (they are replaced by `MIDDLEGAME_MATERIAL_VALUES` and `ENDGAME_MATERIAL_VALUES`).

- [ ] **Step 4: Update the existing tests that import board helpers**

The `use` statement at the top of `eval.rs` tests only imports `from_fen` and `start_position`. Make sure it includes `Color` too if needed:

```rust
use crate::board::{from_fen, start_position};
```

The tests `white_up_a_queen_is_positive` and `black_up_a_rook_is_negative_from_whites_perspective` should still pass because material advantage is large enough to exceed `LAZY_EVALUATION_THRESHOLD`.

- [ ] **Step 5: Run all eval tests**

```bash
cargo test -p turbowhale -- eval::tests
```

Expected: all pass, including old ones.

- [ ] **Step 6: Run all tests**

```bash
cargo test -p turbowhale
```

Expected: all pass.

- [ ] **Step 7: Commit**

```
feat: replace material-only evaluate with tapered PST evaluation
```

---

## Task 5: Pawn structure masks and evaluate_pawn_structure

**Files:**
- Modify: `src/eval.rs`

- [ ] **Step 1: Write failing tests**

Add to `#[cfg(test)]` in `src/eval.rs`:

```rust
#[test]
fn doubled_pawn_scores_worse_than_single_pawn() {
    // Two white pawns on the e-file vs one white pawn on e-file
    let doubled = from_fen("4k3/8/8/8/4P3/4P3/8/4K3 w - - 0 1");
    let single  = from_fen("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1");
    // Doubled pawns should receive a penalty relative to single
    assert!(
        evaluate_pawn_structure(&doubled) < evaluate_pawn_structure(&single),
        "doubled pawns should score worse than a single pawn"
    );
}

#[test]
fn isolated_pawn_scores_worse_than_connected_pawn() {
    // White pawn on e4, no adjacent file pawns (isolated)
    let isolated  = from_fen("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1");
    // White pawns on d4 and e4 (e4 is connected, d4 is connected)
    let connected = from_fen("4k3/8/8/8/3PP3/8/8/4K3 w - - 0 1");
    assert!(
        evaluate_pawn_structure(&isolated) < evaluate_pawn_structure(&connected),
        "isolated pawn should score worse than connected pawns"
    );
}

#[test]
fn passed_pawn_on_rank_7_scores_better_than_rank_3() {
    let rank_7 = from_fen("4k3/4P3/8/8/8/8/8/4K3 w - - 0 1");
    let rank_3 = from_fen("4k3/8/8/8/8/4P3/8/4K3 w - - 0 1");
    assert!(
        evaluate_pawn_structure(&rank_7) > evaluate_pawn_structure(&rank_3),
        "passed pawn on rank 7 should score higher than passed pawn on rank 3"
    );
}

#[test]
fn pawn_structure_is_symmetric_at_start_position() {
    let position = start_position();
    assert_eq!(evaluate_pawn_structure(&position), 0);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p turbowhale -- eval::tests::doubled_pawn_scores_worse eval::tests::isolated_pawn_scores_worse eval::tests::passed_pawn_on_rank_7 eval::tests::pawn_structure_is_symmetric
```

Expected: compile error (`evaluate_pawn_structure` not defined).

- [ ] **Step 3: Add file masks and pawn structure constants**

Add to `src/eval.rs` after the PST tables:

```rust
// ---------------------------------------------------------------------------
// Pawn structure penalties and bonuses (centipawns).
// ---------------------------------------------------------------------------

const DOUBLED_PAWN_PENALTY: i32 = 10;
const ISOLATED_PAWN_PENALTY: i32 = 15;
/// Bonus per passed pawn indexed by rank (rank 0 unused; rank 7 = promotion rank).
const PASSED_PAWN_RANK_BONUS: [i32; 8] = [0, 0, 10, 20, 35, 55, 80, 110];

// ---------------------------------------------------------------------------
// Precomputed bitboard masks for pawn structure evaluation.
// ---------------------------------------------------------------------------

/// One bit per square on each of the 8 files. FILE_MASKS[0] = a-file.
const FILE_MASKS: [u64; 8] = [
    0x0101010101010101, // a-file
    0x0202020202020202, // b-file
    0x0404040404040404, // c-file
    0x0808080808080808, // d-file
    0x1010101010101010, // e-file
    0x2020202020202020, // f-file
    0x4040404040404040, // g-file
    0x8080808080808080, // h-file
];

/// For each file, the mask of all squares on adjacent files.
/// ADJACENT_FILE_MASKS[0] = b-file only; ADJACENT_FILE_MASKS[7] = g-file only.
const ADJACENT_FILE_MASKS: [u64; 8] = [
    0x0202020202020202, // a: only b-file
    0x0505050505050505, // b: a + c
    0x0a0a0a0a0a0a0a0a, // c: b + d
    0x1414141414141414, // d: c + e
    0x2828282828282828, // e: d + f
    0x5050505050505050, // f: e + g
    0xa0a0a0a0a0a0a0a0, // g: f + h
    0x4040404040404040, // h: only g-file
];
```

- [ ] **Step 4: Add forward span masks using LazyLock**

Add to `src/eval.rs` after the file mask constants:

```rust
use std::sync::LazyLock;

/// For each square, all squares on the same and adjacent files with strictly higher rank.
/// Used to determine if a White pawn is passed (no Black pawn in this mask).
static WHITE_FORWARD_SPAN_MASKS: LazyLock<[u64; 64]> = LazyLock::new(|| {
    let mut masks = [0u64; 64];
    for square in 0..64usize {
        let file = square % 8;
        let rank = square / 8;
        // All squares with rank > current rank
        let forward_ranks: u64 = if rank < 7 {
            0xFFFFFFFFFFFFFFFF << ((rank + 1) * 8)
        } else {
            0
        };
        let file_and_adjacent = FILE_MASKS[file] | ADJACENT_FILE_MASKS[file];
        masks[square] = forward_ranks & file_and_adjacent;
    }
    masks
});

/// For each square, all squares on the same and adjacent files with strictly lower rank.
/// Used to determine if a Black pawn is passed (no White pawn in this mask).
static BLACK_FORWARD_SPAN_MASKS: LazyLock<[u64; 64]> = LazyLock::new(|| {
    let mut masks = [0u64; 64];
    for square in 0..64usize {
        let file = square % 8;
        let rank = square / 8;
        // All squares with rank < current rank
        let backward_ranks: u64 = if rank > 0 {
            0xFFFFFFFFFFFFFFFF >> ((8 - rank) * 8)
        } else {
            0
        };
        let file_and_adjacent = FILE_MASKS[file] | ADJACENT_FILE_MASKS[file];
        masks[square] = backward_ranks & file_and_adjacent;
    }
    masks
});
```

- [ ] **Step 5: Implement evaluate_pawn_structure**

Add to `src/eval.rs`:

```rust
/// Returns a score in centipawns from White's perspective:
/// positive values favour White, negative values favour Black.
///
/// Evaluates three pawn structure features for both sides and returns
/// white_score - black_score.
fn evaluate_pawn_structure(position: &Position) -> i32 {
    evaluate_pawn_structure_for_color(position, Color::White)
        - evaluate_pawn_structure_for_color(position, Color::Black)
}

/// Returns the pawn structure score for one side (always positive = good for that side).
fn evaluate_pawn_structure_for_color(position: &Position, color: Color) -> i32 {
    let (own_pawns, enemy_pawns) = match color {
        Color::White => (position.white_pawns, position.black_pawns),
        Color::Black => (position.black_pawns, position.white_pawns),
    };

    let mut score = 0i32;
    let mut pawns_remaining = own_pawns;

    while pawns_remaining != 0 {
        let square = pawns_remaining.trailing_zeros() as usize;
        pawns_remaining &= pawns_remaining - 1;

        let file = square % 8;
        let rank = square / 8;

        // Doubled pawn penalty: penalise each extra pawn beyond the first on this file
        let pawns_on_file = (own_pawns & FILE_MASKS[file]).count_ones() as i32;
        if pawns_on_file > 1 {
            score -= DOUBLED_PAWN_PENALTY;
        }

        // Isolated pawn penalty: penalise if no friendly pawn on adjacent files
        if own_pawns & ADJACENT_FILE_MASKS[file] == 0 {
            score -= ISOLATED_PAWN_PENALTY;
        }

        // Passed pawn bonus: bonus if no enemy pawn can block or capture this pawn
        let forward_span = match color {
            Color::White => WHITE_FORWARD_SPAN_MASKS[square],
            Color::Black => BLACK_FORWARD_SPAN_MASKS[square],
        };
        if forward_span & enemy_pawns == 0 {
            // Bonus scaled by how advanced the pawn is (rank 1–6 for White, mirrored for Black)
            let advancement_rank = match color {
                Color::White => rank,
                Color::Black => 7 - rank,
            };
            score += PASSED_PAWN_RANK_BONUS[advancement_rank];
        }
    }

    score
}
```

- [ ] **Step 6: Run pawn structure tests**

```bash
cargo test -p turbowhale -- eval::tests::doubled_pawn_scores_worse eval::tests::isolated_pawn_scores_worse eval::tests::passed_pawn_on_rank_7 eval::tests::pawn_structure_is_symmetric
```

Expected: all PASS.

- [ ] **Step 7: Run all tests**

```bash
cargo test -p turbowhale
```

Expected: all pass.

- [ ] **Step 8: Commit**

```
feat: add evaluate_pawn_structure with doubled, isolated, and passed pawn detection
```

---

## Task 6: Implement evaluate_mobility

**Files:**
- Modify: `src/eval.rs`

- [ ] **Step 1: Write failing tests**

Add to `#[cfg(test)]` in `src/eval.rs`:

```rust
#[test]
fn mobility_is_symmetric_at_start_position() {
    let position = start_position();
    let (white_middlegame, white_endgame) = mobility_for_color(&position, Color::White);
    let (black_middlegame, black_endgame) = mobility_for_color(&position, Color::Black);
    assert_eq!(white_middlegame, black_middlegame, "start position middlegame mobility should be equal");
    assert_eq!(white_endgame,    black_endgame,    "start position endgame mobility should be equal");
}

#[test]
fn knight_on_d4_has_more_mobility_than_knight_on_a1() {
    let knight_on_d4 = from_fen("4k3/8/8/8/3N4/8/8/4K3 w - - 0 1");
    let knight_on_a1 = from_fen("4k3/8/8/8/8/8/8/N3K3 w - - 0 1");
    let (d4_mg, _) = mobility_for_color(&knight_on_d4, Color::White);
    let (a1_mg, _) = mobility_for_color(&knight_on_a1, Color::White);
    assert!(d4_mg > a1_mg, "knight on d4 should have higher mobility than knight on a1");
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p turbowhale -- eval::tests::mobility_is_symmetric eval::tests::knight_on_d4_has_more_mobility
```

Expected: compile error.

- [ ] **Step 3: Add mobility constants and implement the functions**

Add to `src/eval.rs`:

```rust
// ---------------------------------------------------------------------------
// Mobility bonuses (centipawns per reachable square, by piece type).
// ---------------------------------------------------------------------------

const KNIGHT_MOBILITY_MIDDLEGAME_BONUS: i32 = 4;
const KNIGHT_MOBILITY_ENDGAME_BONUS: i32 = 2;
const BISHOP_MOBILITY_MIDDLEGAME_BONUS: i32 = 3;
const BISHOP_MOBILITY_ENDGAME_BONUS: i32 = 3;
const ROOK_MOBILITY_MIDDLEGAME_BONUS: i32 = 2;
const ROOK_MOBILITY_ENDGAME_BONUS: i32 = 4;
const QUEEN_MOBILITY_MIDDLEGAME_BONUS: i32 = 1;
const QUEEN_MOBILITY_ENDGAME_BONUS: i32 = 2;
```

Then add the functions:

```rust
/// Returns (middlegame_bonus, endgame_bonus) in centipawns from White's perspective:
/// positive values favour White, negative values favour Black.
fn evaluate_mobility(position: &Position) -> (i32, i32) {
    let (white_mg, white_eg) = mobility_for_color(position, Color::White);
    let (black_mg, black_eg) = mobility_for_color(position, Color::Black);
    (white_mg - black_mg, white_eg - black_eg)
}

/// Returns (middlegame_bonus, endgame_bonus) for one side.
/// Counts reachable squares for each non-pawn, non-king piece (squares not
/// occupied by a friendly piece), using the precomputed attack tables.
fn mobility_for_color(position: &Position, color: Color) -> (i32, i32) {
    use crate::movegen::{bishop_attacks, knight_attacks_for_square, queen_attacks, rook_attacks};

    let (own_occupancy, knights, bishops, rooks, queens) = match color {
        Color::White => (
            position.white_occupancy,
            position.white_knights,
            position.white_bishops,
            position.white_rooks,
            position.white_queens,
        ),
        Color::Black => (
            position.black_occupancy,
            position.black_knights,
            position.black_bishops,
            position.black_rooks,
            position.black_queens,
        ),
    };

    let mut middlegame_bonus = 0i32;
    let mut endgame_bonus = 0i32;

    let mut knight_bits = knights;
    while knight_bits != 0 {
        let square = knight_bits.trailing_zeros() as usize;
        knight_bits &= knight_bits - 1;
        let reachable_squares = (knight_attacks_for_square(square) & !own_occupancy).count_ones() as i32;
        middlegame_bonus += reachable_squares * KNIGHT_MOBILITY_MIDDLEGAME_BONUS;
        endgame_bonus    += reachable_squares * KNIGHT_MOBILITY_ENDGAME_BONUS;
    }

    let mut bishop_bits = bishops;
    while bishop_bits != 0 {
        let square = bishop_bits.trailing_zeros() as usize;
        bishop_bits &= bishop_bits - 1;
        let reachable_squares = (bishop_attacks(square, position.all_occupancy) & !own_occupancy).count_ones() as i32;
        middlegame_bonus += reachable_squares * BISHOP_MOBILITY_MIDDLEGAME_BONUS;
        endgame_bonus    += reachable_squares * BISHOP_MOBILITY_ENDGAME_BONUS;
    }

    let mut rook_bits = rooks;
    while rook_bits != 0 {
        let square = rook_bits.trailing_zeros() as usize;
        rook_bits &= rook_bits - 1;
        let reachable_squares = (rook_attacks(square, position.all_occupancy) & !own_occupancy).count_ones() as i32;
        middlegame_bonus += reachable_squares * ROOK_MOBILITY_MIDDLEGAME_BONUS;
        endgame_bonus    += reachable_squares * ROOK_MOBILITY_ENDGAME_BONUS;
    }

    let mut queen_bits = queens;
    while queen_bits != 0 {
        let square = queen_bits.trailing_zeros() as usize;
        queen_bits &= queen_bits - 1;
        let reachable_squares = (queen_attacks(square, position.all_occupancy) & !own_occupancy).count_ones() as i32;
        middlegame_bonus += reachable_squares * QUEEN_MOBILITY_MIDDLEGAME_BONUS;
        endgame_bonus    += reachable_squares * QUEEN_MOBILITY_ENDGAME_BONUS;
    }

    (middlegame_bonus, endgame_bonus)
}
```

- [ ] **Step 4: Run mobility tests**

```bash
cargo test -p turbowhale -- eval::tests::mobility_is_symmetric eval::tests::knight_on_d4_has_more_mobility
```

Expected: all PASS.

- [ ] **Step 5: Run all tests**

```bash
cargo test -p turbowhale
```

Expected: all pass.

- [ ] **Step 6: Commit**

```
feat: add evaluate_mobility using precomputed attack tables
```

---

## Task 7: King shield masks and evaluate_king_safety

**Files:**
- Modify: `src/eval.rs`

- [ ] **Step 1: Write failing tests**

Add to `#[cfg(test)]` in `src/eval.rs`:

```rust
#[test]
fn king_safety_is_symmetric_at_start_position() {
    let position = start_position();
    assert_eq!(evaluate_king_safety(&position), 0);
}

#[test]
fn castled_king_with_intact_pawn_shield_scores_better_than_broken_shield() {
    // White king castled kingside, full pawn shield
    let intact_shield = from_fen("4k3/8/8/8/8/8/5PPP/5RK1 w - - 0 1");
    // White king castled kingside, pawn on f2 missing (broken shield)
    let broken_shield = from_fen("4k3/8/8/8/8/8/6PP/5RK1 w - - 0 1");
    assert!(
        evaluate_king_safety(&intact_shield) > evaluate_king_safety(&broken_shield),
        "intact pawn shield should score better than broken shield"
    );
}

#[test]
fn king_on_open_file_scores_worse_than_king_on_closed_file() {
    // White king on e1, open e-file (no pawns on e-file)
    let open_file_king  = from_fen("4k3/8/8/8/8/8/3P1PPP/3P1K2 w - - 0 1");
    // White king on g1, g-file closed (pawn on g2)
    let closed_file_king = from_fen("4k3/8/8/8/8/8/3PPPPP/3P2K1 w - - 0 1");
    assert!(
        evaluate_king_safety(&open_file_king) < evaluate_king_safety(&closed_file_king),
        "king on open file should score worse than king on closed file"
    );
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p turbowhale -- eval::tests::king_safety_is_symmetric eval::tests::castled_king_with_intact eval::tests::king_on_open_file
```

Expected: compile error.

- [ ] **Step 3: Add king safety constants and shield masks**

Add to `src/eval.rs`:

```rust
// ---------------------------------------------------------------------------
// King safety penalties (centipawns). Applied to the middlegame score only,
// scaled by game_phase so they fade toward zero in the endgame.
// ---------------------------------------------------------------------------

const PAWN_SHIELD_PENALTY: i32 = 15;
const KING_OPEN_FILE_PENALTY: i32 = 25;
const KING_SEMI_OPEN_FILE_PENALTY: i32 = 10;

/// For each king square, the pawn shield zone (3 squares directly in front
/// and 3 squares one rank further), for White.
/// For Black, use KING_SHIELD_MASKS[square ^ 56].
static KING_SHIELD_MASKS: LazyLock<[u64; 64]> = LazyLock::new(|| {
    let mut masks = [0u64; 64];
    for square in 0..64usize {
        let file = square % 8;
        let rank = square / 8;
        // Files covered: king file plus adjacent files
        let covered_files = FILE_MASKS[file] | ADJACENT_FILE_MASKS[file];
        // Two ranks immediately in front of the king (from White's perspective)
        let rank1_mask: u64 = if rank + 1 <= 7 { 0xFFu64 << ((rank + 1) * 8) } else { 0 };
        let rank2_mask: u64 = if rank + 2 <= 7 { 0xFFu64 << ((rank + 2) * 8) } else { 0 };
        masks[square] = covered_files & (rank1_mask | rank2_mask);
    }
    masks
});
```

- [ ] **Step 4: Implement evaluate_king_safety**

Add to `src/eval.rs`:

```rust
/// Returns a score in centipawns from White's perspective:
/// positive values favour White, negative values favour Black.
/// King safety is a middlegame concern only; the caller scales by game_phase.
fn evaluate_king_safety(position: &Position) -> i32 {
    evaluate_king_safety_for_color(position, Color::White)
        - evaluate_king_safety_for_color(position, Color::Black)
}

/// Returns the king safety score for one side (positive = good for that side).
fn evaluate_king_safety_for_color(position: &Position, color: Color) -> i32 {
    let (own_pawns, enemy_pawns, king_bitboard) = match color {
        Color::White => (position.white_pawns, position.black_pawns, position.white_king),
        Color::Black => (position.black_pawns, position.white_pawns, position.black_king),
    };

    let king_square = king_bitboard.trailing_zeros() as usize;
    // For Black, mirror the king square to use White's perspective shield mask
    let shield_lookup_square = match color {
        Color::White => king_square,
        Color::Black => king_square ^ 56,
    };

    let shield_zone = KING_SHIELD_MASKS[shield_lookup_square];
    let shield_pawns_present = (own_pawns & shield_zone).count_ones() as i32;
    let shield_pawns_possible = (shield_zone).count_ones() as i32;
    let missing_shield_pawns = (shield_pawns_possible - shield_pawns_present).max(0);

    let mut score = -(missing_shield_pawns * PAWN_SHIELD_PENALTY);

    // Open and semi-open file penalties for the king file and adjacent files
    let king_file = king_square % 8;
    let files_to_check: &[usize] = match king_file {
        0 => &[0, 1],
        7 => &[6, 7],
        f => &[f - 1, *f, f + 1],
    };

    for &file in files_to_check {
        let file_mask = FILE_MASKS[file];
        if own_pawns & file_mask == 0 {
            if enemy_pawns & file_mask == 0 {
                score -= KING_OPEN_FILE_PENALTY;
            } else {
                score -= KING_SEMI_OPEN_FILE_PENALTY;
            }
        }
    }

    score
}
```

```rust
    let min_file = king_file.saturating_sub(1);
    let max_file = (king_file + 1).min(7);
    for file in min_file..=max_file {
        let file_mask = FILE_MASKS[file];
        if own_pawns & file_mask == 0 {
            if enemy_pawns & file_mask == 0 {
                score -= KING_OPEN_FILE_PENALTY;
            } else {
                score -= KING_SEMI_OPEN_FILE_PENALTY;
            }
        }
    }

- [ ] **Step 5: Run king safety tests**

```bash
cargo test -p turbowhale -- eval::tests::king_safety_is_symmetric eval::tests::castled_king_with_intact eval::tests::king_on_open_file
```

Expected: all PASS.

- [ ] **Step 6: Run all tests**

```bash
cargo test -p turbowhale
```

Expected: all pass.

- [ ] **Step 7: Commit**

```
feat: add evaluate_king_safety with pawn shield and open file detection
```

---

## Task 8: Wire all features into evaluate with lazy eval

**Files:**
- Modify: `src/eval.rs`

- [ ] **Step 1: Write integration tests**

Add to `#[cfg(test)]` in `src/eval.rs`:

```rust
#[test]
fn position_with_weak_pawn_structure_scores_worse_than_clean_structure() {
    // White has doubled isolated pawns — should evaluate worse than clean pawns
    let weak   = from_fen("4k3/8/8/8/4P3/4P3/8/4K3 w - - 0 1"); // doubled on e-file
    let strong = from_fen("4k3/8/8/8/3P4/4P3/8/4K3 w - - 0 1"); // connected d4+e3
    // Force full evaluation by giving a near-balanced position (within lazy threshold)
    assert!(
        evaluate(&weak) <= evaluate(&strong),
        "weak pawn structure should not score better than clean structure"
    );
}
```

- [ ] **Step 2: Run test to confirm it fails or is already correct**

```bash
cargo test -p turbowhale -- eval::tests::position_with_weak_pawn_structure_scores_worse
```

This test may already pass if pawn penalties push scores outside the lazy threshold window. The key check is that the full eval path is reachable.

- [ ] **Step 3: Replace the placeholder evaluate body with the full wired version**

Replace the `evaluate` function in `src/eval.rs`:

```rust
/// Returns a score in centipawns from the perspective of the side to move:
/// positive values favour the side to move.
/// This is the only function that performs the perspective flip.
pub fn evaluate(position: &Position) -> i32 {
    // 1. Cheap tapered score from the incrementally-maintained PST fields.
    //    Blends from pure middlegame (game_phase=MAX_GAME_PHASE) to pure endgame (game_phase=0).
    let tapered_score = ((position.middlegame_score * position.game_phase)
        + (position.endgame_score * (MAX_GAME_PHASE - position.game_phase)))
        / MAX_GAME_PHASE;

    // 2. Lazy evaluation: skip expensive sub-functions when the score is already
    //    outside a margin where positional features could change the outcome.
    if tapered_score.abs() > LAZY_EVALUATION_THRESHOLD {
        return match position.side_to_move {
            Color::White => tapered_score,
            Color::Black => -tapered_score,
        };
    }

    // 3. Expensive positional features — only reached in near-balanced positions.

    // Pawn structure: doubled pawns, isolated pawns, passed pawns.
    let pawn_score = evaluate_pawn_structure(position);

    // Mobility: reachable squares per piece type, blended by game phase.
    let (mobility_middlegame, mobility_endgame) = evaluate_mobility(position);
    let mobility_score = ((mobility_middlegame * position.game_phase)
        + (mobility_endgame * (MAX_GAME_PHASE - position.game_phase)))
        / MAX_GAME_PHASE;

    // King safety: pawn shield and open files. Middlegame-only, scaled by phase.
    let king_safety_score = (evaluate_king_safety(position) * position.game_phase) / MAX_GAME_PHASE;

    // 4. Combine all terms (all White-minus-Black) and orient to side to move.
    let absolute_score = tapered_score + pawn_score + mobility_score + king_safety_score;

    match position.side_to_move {
        Color::White => absolute_score,
        Color::Black => -absolute_score,
    }
}
```

- [ ] **Step 4: Run all tests**

```bash
cargo test -p turbowhale
```

Expected: all pass.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy -p turbowhale
```

Fix any warnings before committing.

- [ ] **Step 6: Run a release build to verify it compiles cleanly**

```bash
cargo build --release -p turbowhale
```

Expected: compiles without warnings.

- [ ] **Step 7: Commit**

```
feat: wire pawn structure, mobility, and king safety into evaluate with lazy eval
```
