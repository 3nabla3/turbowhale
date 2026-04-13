use crate::board::Color;

// File masks: FILE_MASKS[0] = file A (squares 0,8,16,...,56)
pub const FILE_MASKS: [u64; 8] = {
    const FILE_A: u64 = 0x0101010101010101u64;
    [
        FILE_A,       FILE_A << 1, FILE_A << 2, FILE_A << 3,
        FILE_A << 4,  FILE_A << 5, FILE_A << 6, FILE_A << 7,
    ]
};

// Rank masks: RANK_MASKS[0] = rank 1 (squares 0-7)
pub const RANK_MASKS: [u64; 8] = {
    const RANK_1: u64 = 0x00000000000000FFu64;
    [
        RANK_1,        RANK_1 << 8,  RANK_1 << 16, RANK_1 << 24,
        RANK_1 << 32,  RANK_1 << 40, RANK_1 << 48, RANK_1 << 56,
    ]
};

/// Returns the knight attack bitboard for a given square.
pub fn knight_attacks_for_square(square: usize) -> u64 {
    let rank = (square / 8) as i32;
    let file = (square % 8) as i32;
    let offsets: [(i32, i32); 8] = [
        (2, 1), (2, -1), (-2, 1), (-2, -1),
        (1, 2), (1, -2), (-1, 2), (-1, -2),
    ];
    offsets.iter().fold(0u64, |attacks, &(rank_offset, file_offset)| {
        let target_rank = rank + rank_offset;
        let target_file = file + file_offset;
        if target_rank >= 0 && target_rank < 8 && target_file >= 0 && target_file < 8 {
            attacks | (1u64 << (target_rank as usize * 8 + target_file as usize))
        } else {
            attacks
        }
    })
}

/// Returns the king attack bitboard for a given square.
pub fn king_attacks_for_square(square: usize) -> u64 {
    let rank = (square / 8) as i32;
    let file = (square % 8) as i32;
    let offsets: [(i32, i32); 8] = [
        (1, 0), (-1, 0), (0, 1), (0, -1),
        (1, 1), (1, -1), (-1, 1), (-1, -1),
    ];
    offsets.iter().fold(0u64, |attacks, &(rank_offset, file_offset)| {
        let target_rank = rank + rank_offset;
        let target_file = file + file_offset;
        if target_rank >= 0 && target_rank < 8 && target_file >= 0 && target_file < 8 {
            attacks | (1u64 << (target_rank as usize * 8 + target_file as usize))
        } else {
            attacks
        }
    })
}

/// Returns the pawn attack bitboard for a given square and color.
/// This represents the squares the pawn *attacks* (diagonals), not where it can push.
pub fn pawn_attacks_for_square(square: usize, color: Color) -> u64 {
    let pawn_bit = 1u64 << square;
    match color {
        Color::White => {
            let left_attack  = (pawn_bit & !FILE_MASKS[0]) << 7;
            let right_attack = (pawn_bit & !FILE_MASKS[7]) << 9;
            left_attack | right_attack
        }
        Color::Black => {
            let left_attack  = (pawn_bit & !FILE_MASKS[7]) >> 7;
            let right_attack = (pawn_bit & !FILE_MASKS[0]) >> 9;
            left_attack | right_attack
        }
    }
}

use std::sync::OnceLock;

// Per-square diagonal masks (NE-SW direction)
static SQUARE_DIAGONAL_MASKS: OnceLock<[u64; 64]> = OnceLock::new();
// Per-square anti-diagonal masks (NW-SE direction)
static SQUARE_ANTI_DIAGONAL_MASKS: OnceLock<[u64; 64]> = OnceLock::new();

fn get_diagonal_masks() -> &'static [u64; 64] {
    SQUARE_DIAGONAL_MASKS.get_or_init(|| {
        std::array::from_fn(|square| {
            let rank = (square / 8) as i32;
            let file = (square % 8) as i32;
            (0..8i32).fold(0u64, |mask, target_rank| {
                let target_file = file + (target_rank - rank);
                if target_file >= 0 && target_file < 8 {
                    mask | (1u64 << (target_rank as usize * 8 + target_file as usize))
                } else {
                    mask
                }
            })
        })
    })
}

fn get_anti_diagonal_masks() -> &'static [u64; 64] {
    SQUARE_ANTI_DIAGONAL_MASKS.get_or_init(|| {
        std::array::from_fn(|square| {
            let rank = (square / 8) as i32;
            let file = (square % 8) as i32;
            (0..8i32).fold(0u64, |mask, target_rank| {
                let target_file = file - (target_rank - rank);
                if target_file >= 0 && target_file < 8 {
                    mask | (1u64 << (target_rank as usize * 8 + target_file as usize))
                } else {
                    mask
                }
            })
        })
    })
}

/// Flips a bitboard along the a1-h8 diagonal (transposes the 8x8 board).
/// Used as the "reverse" function for diagonal ray attacks.
fn flip_diagonal_a1h8(mut board: u64) -> u64 {
    const K1: u64 = 0x5500550055005500;
    const K2: u64 = 0x3333000033330000;
    const K4: u64 = 0x0f0f0f0f00000000;
    let mut t = K4 & (board ^ (board << 28));
    board ^= t ^ (t >> 28);
    t = K2 & (board ^ (board << 14));
    board ^= t ^ (t >> 14);
    t = K1 & (board ^ (board << 7));
    board ^= t ^ (t >> 7);
    board
}

/// Flips a bitboard along the a8-h1 anti-diagonal.
/// Used as the "reverse" function for anti-diagonal ray attacks.
fn flip_anti_diagonal_a8h1(board: u64) -> u64 {
    flip_diagonal_a1h8(board.swap_bytes())
}

/// Computes rook attacks along the file (N/S) using hyperbola quintessence.
/// Reverse function: swap_bytes (reverses rank order).
fn compute_file_attacks(square: usize, occupancy: u64) -> u64 {
    let piece_bit = 1u64 << square;
    let file_mask = FILE_MASKS[square % 8];
    let masked_occupancy = occupancy & file_mask;
    let forward = masked_occupancy.wrapping_sub(piece_bit.wrapping_mul(2));
    let reversed_occupancy = masked_occupancy.swap_bytes();
    let reversed_piece = piece_bit.swap_bytes();
    let backward = reversed_occupancy
        .wrapping_sub(reversed_piece.wrapping_mul(2))
        .swap_bytes();
    (forward ^ backward) & file_mask
}

/// Computes rook attacks along the rank (E/W) using hyperbola quintessence.
/// Operates on the 8-bit rank byte; reverse function: u8::reverse_bits.
fn compute_rank_attacks(square: usize, occupancy: u64) -> u64 {
    let file = square % 8;
    let rank_shift = (square / 8) * 8;
    let piece_bit = 1u8 << file;
    let occupancy_byte = ((occupancy >> rank_shift) & 0xFF) as u8;
    let forward = occupancy_byte.wrapping_sub(piece_bit.wrapping_mul(2));
    let reversed_piece = piece_bit.reverse_bits();
    let reversed_occupancy = occupancy_byte.reverse_bits();
    let backward = reversed_occupancy
        .wrapping_sub(reversed_piece.wrapping_mul(2))
        .reverse_bits();
    let attacks_byte = (forward ^ backward) & !piece_bit;
    (attacks_byte as u64) << rank_shift
}

/// Computes bishop attacks along the diagonal (NE/SW) using hyperbola quintessence.
/// Uses reverse_bits as the reversal function since diagonal bits are spaced 9 apart.
fn compute_diagonal_attacks(square: usize, occupancy: u64) -> u64 {
    let piece_bit = 1u64 << square;
    let diagonal_mask = get_diagonal_masks()[square];
    let masked_occupancy = occupancy & diagonal_mask;
    let forward = masked_occupancy.wrapping_sub(piece_bit.wrapping_mul(2));
    let reversed_piece = piece_bit.reverse_bits();
    let reversed_occupancy = masked_occupancy.reverse_bits();
    let backward = reversed_occupancy
        .wrapping_sub(reversed_piece.wrapping_mul(2))
        .reverse_bits();
    (forward ^ backward) & diagonal_mask
}

/// Computes bishop attacks along the anti-diagonal (NW/SE) using hyperbola quintessence.
/// Uses reverse_bits as the reversal function since anti-diagonal bits are spaced 7 apart.
fn compute_anti_diagonal_attacks(square: usize, occupancy: u64) -> u64 {
    let piece_bit = 1u64 << square;
    let anti_diagonal_mask = get_anti_diagonal_masks()[square];
    let masked_occupancy = occupancy & anti_diagonal_mask;
    let forward = masked_occupancy.wrapping_sub(piece_bit.wrapping_mul(2));
    let reversed_piece = piece_bit.reverse_bits();
    let reversed_occupancy = masked_occupancy.reverse_bits();
    let backward = reversed_occupancy
        .wrapping_sub(reversed_piece.wrapping_mul(2))
        .reverse_bits();
    (forward ^ backward) & anti_diagonal_mask
}

/// Returns all squares attacked by a rook on the given square given the board occupancy.
pub fn rook_attacks(square: usize, occupancy: u64) -> u64 {
    compute_file_attacks(square, occupancy) | compute_rank_attacks(square, occupancy)
}

/// Returns all squares attacked by a bishop on the given square given the board occupancy.
pub fn bishop_attacks(square: usize, occupancy: u64) -> u64 {
    compute_diagonal_attacks(square, occupancy) | compute_anti_diagonal_attacks(square, occupancy)
}

/// Returns all squares attacked by a queen on the given square given the board occupancy.
pub fn queen_attacks(square: usize, occupancy: u64) -> u64 {
    rook_attacks(square, occupancy) | bishop_attacks(square, occupancy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knight_on_a1_attacks_b3_and_c2() {
        let attacks = knight_attacks_for_square(0);
        assert_ne!(attacks & (1u64 << 17), 0, "b3 (17) should be attacked");
        assert_ne!(attacks & (1u64 << 10), 0, "c2 (10) should be attacked");
        assert_eq!(attacks.count_ones(), 2, "knight on a1 attacks exactly 2 squares");
    }

    #[test]
    fn knight_on_e4_attacks_eight_squares() {
        let attacks = knight_attacks_for_square(28);
        assert_eq!(attacks.count_ones(), 8);
    }

    #[test]
    fn king_on_e4_attacks_eight_squares() {
        let attacks = king_attacks_for_square(28);
        assert_eq!(attacks.count_ones(), 8);
    }

    #[test]
    fn king_on_a1_attacks_three_squares() {
        let attacks = king_attacks_for_square(0);
        assert_eq!(attacks.count_ones(), 3);
    }

    #[test]
    fn white_pawn_on_e4_attacks_d5_and_f5() {
        let attacks = pawn_attacks_for_square(28, Color::White);
        assert_ne!(attacks & (1u64 << 35), 0, "d5 (35) should be attacked");
        assert_ne!(attacks & (1u64 << 37), 0, "f5 (37) should be attacked");
        assert_eq!(attacks.count_ones(), 2);
    }

    #[test]
    fn rook_on_e4_empty_board_attacks_fourteen_squares() {
        // Rook on e4 (28) with no other pieces attacks 7+7 = 14 squares
        let attacks = rook_attacks(28, 1u64 << 28);
        assert_eq!(attacks.count_ones(), 14);
    }

    #[test]
    fn rook_on_e4_blocked_by_e6_attacks_correctly() {
        // Rook on e4 (28), blocker on e6 (44): north ray stops at e6
        let occupancy = (1u64 << 28) | (1u64 << 44);
        let attacks = rook_attacks(28, occupancy);
        assert_ne!(attacks & (1u64 << 44), 0, "e6 (blocker) should be in attack set");
        assert_eq!(attacks & (1u64 << 52), 0, "e7 (behind blocker) should not be attacked");
    }

    #[test]
    fn bishop_on_e4_empty_board_attacks_thirteen_squares() {
        // Bishop on e4 (28) with no other pieces
        let attacks = bishop_attacks(28, 1u64 << 28);
        assert_eq!(attacks.count_ones(), 13);
    }
}
