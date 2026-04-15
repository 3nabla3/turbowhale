use crate::board::{Color, Position};

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

use std::sync::LazyLock;

/// For each square, all squares on the same and adjacent files with strictly higher rank.
/// Used to determine if a White pawn is passed (no Black pawn in this mask).
static WHITE_FORWARD_SPAN_MASKS: LazyLock<[u64; 64]> = LazyLock::new(|| {
    let mut masks = [0u64; 64];
    for (square, mask) in masks.iter_mut().enumerate() {
        let file = square % 8;
        let rank = square / 8;
        let forward_ranks: u64 = if rank < 7 {
            0xFFFFFFFFFFFFFFFF << ((rank + 1) * 8)
        } else {
            0
        };
        let file_and_adjacent = FILE_MASKS[file] | ADJACENT_FILE_MASKS[file];
        *mask = forward_ranks & file_and_adjacent;
    }
    masks
});

/// For each square, all squares on the same and adjacent files with strictly lower rank.
/// Used to determine if a Black pawn is passed (no White pawn in this mask).
static BLACK_FORWARD_SPAN_MASKS: LazyLock<[u64; 64]> = LazyLock::new(|| {
    let mut masks = [0u64; 64];
    for (square, mask) in masks.iter_mut().enumerate() {
        let file = square % 8;
        let rank = square / 8;
        let backward_ranks: u64 = if rank > 0 {
            0xFFFFFFFFFFFFFFFF >> ((8 - rank) * 8)
        } else {
            0
        };
        let file_and_adjacent = FILE_MASKS[file] | ADJACENT_FILE_MASKS[file];
        *mask = backward_ranks & file_and_adjacent;
    }
    masks
});

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
    for (square, mask) in masks.iter_mut().enumerate() {
        let file = square % 8;
        let rank = square / 8;
        // Files covered: king file plus adjacent files
        let covered_files = FILE_MASKS[file] | ADJACENT_FILE_MASKS[file];
        // Two ranks immediately in front of the king (from White's perspective)
        let rank1_mask: u64 = if rank < 7 { 0xFFu64 << ((rank + 1) * 8) } else { 0 };
        let rank2_mask: u64 = if rank + 2 <= 7 { 0xFFu64 << ((rank + 2) * 8) } else { 0 };
        *mask = covered_files & (rank1_mask | rank2_mask);
    }
    masks
});

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

    if king_bitboard == 0 {
        return 0;
    }

    let king_square = king_bitboard.trailing_zeros() as usize;
    // For Black, mirror the king square to use White's perspective shield mask
    let shield_lookup_square = match color {
        Color::White => king_square,
        Color::Black => king_square ^ 56,
    };

    // KING_SHIELD_MASKS stores upward-facing zones in White's coordinate system.
    // For Black, mirror the resulting mask vertically (swap_bytes reverses rank order).
    let shield_zone = match color {
        Color::White => KING_SHIELD_MASKS[shield_lookup_square],
        Color::Black => KING_SHIELD_MASKS[shield_lookup_square].swap_bytes(),
    };
    let shield_pawns_present = (own_pawns & shield_zone).count_ones() as i32;
    let shield_pawns_possible = shield_zone.count_ones() as i32;
    let missing_shield_pawns = (shield_pawns_possible - shield_pawns_present).max(0);

    let mut score = -(missing_shield_pawns * PAWN_SHIELD_PENALTY);

    // Open and semi-open file penalties for the king file and adjacent files
    let king_file = king_square % 8;
    let minimum_file = king_file.saturating_sub(1);
    let maximum_file = (king_file + 1).min(7);
    for &file_mask in FILE_MASKS[minimum_file..=maximum_file].iter() {
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

/// Threshold (centipawns) above which the cheap tapered PST score is returned
/// without computing pawn structure, mobility, or king safety.
const LAZY_EVALUATION_THRESHOLD: i32 = 500;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{from_fen, start_position, PieceType};

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
    fn position_with_weak_pawn_structure_scores_worse_than_clean_structure() {
        // White has doubled isolated pawns — should evaluate worse than clean pawns
        let weak   = from_fen("4k3/8/8/8/4P3/4P3/8/4K3 w - - 0 1"); // doubled on e-file
        let strong = from_fen("4k3/8/8/8/3P4/4P3/8/4K3 w - - 0 1"); // connected d4+e3
        assert!(
            evaluate(&weak) <= evaluate(&strong),
            "weak pawn structure should not score better than clean structure"
        );
    }

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
        let open_file_king   = from_fen("4k3/8/8/8/8/8/3P1PPP/3P1K2 w - - 0 1");
        // White king on g1, g-file closed (pawn on g2)
        let closed_file_king = from_fen("4k3/8/8/8/8/8/3PPPPP/3P2K1 w - - 0 1");
        assert!(
            evaluate_king_safety(&open_file_king) < evaluate_king_safety(&closed_file_king),
            "king on open file should score worse than king on closed file"
        );
    }

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
