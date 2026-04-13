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
}
