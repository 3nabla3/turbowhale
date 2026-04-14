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

/// Threshold (centipawns) above which the cheap tapered PST score is returned
/// without computing pawn structure, mobility, or king safety.
const LAZY_EVALUATION_THRESHOLD: i32 = 500;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{from_fen, start_position};

    #[test]
    fn knight_positional_bonus_prefers_center_over_corner_in_middlegame() {
        // d4=27, a1=0
        assert!(
            MIDDLEGAME_PIECE_SQUARE_TABLES[PieceType::Knight as usize][27]
                > MIDDLEGAME_PIECE_SQUARE_TABLES[PieceType::Knight as usize][0],
            "knight on d4 should have higher middlegame bonus than knight on a1"
        );
    }

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

    #[test]
    fn start_position_evaluates_to_zero() {
        // Start position is perfectly symmetric — material balance is 0
        let position = start_position();
        assert_eq!(evaluate(&position), 0);
    }

    #[test]
    fn white_up_a_queen_is_positive() {
        // White has queen + all pieces, black missing queen
        let position = from_fen("rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert!(evaluate(&position) > 0, "white up a queen should be positive");
    }

    #[test]
    fn black_up_a_rook_is_negative_from_whites_perspective() {
        // White to move, black has extra rook
        let position = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/1NBQKBNR w Kkq - 0 1");
        assert!(evaluate(&position) < 0, "black up a rook should be negative when white to move");
    }

    #[test]
    fn evaluate_is_negated_for_opposite_side() {
        // Same position, different side to move — scores should be negations
        let white_to_move = from_fen("8/8/8/8/8/8/4P3/4K3 w - - 0 1");
        let black_to_move = from_fen("8/8/8/8/8/8/4P3/4K3 b - - 0 1");
        assert_eq!(evaluate(&white_to_move), -evaluate(&black_to_move));
    }
}
