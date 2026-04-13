use std::sync::OnceLock;

use crate::board::{Color, Move, MoveFlags, PieceType, Position};

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
    // Unlike file/diagonal attacks, the rank mask includes the piece square itself,
    // so we must explicitly exclude it from the result.
    let attacks_byte = (forward ^ backward) & !piece_bit;
    (attacks_byte as u64) << rank_shift
}

/// Computes bishop attacks along the diagonal (NE/SW) using hyperbola quintessence.
/// Uses reverse_bits as the reversal function: after masking to the diagonal,
/// reverse_bits correctly inverts the bit ordering within the isolated subset.
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
/// Uses reverse_bits as the reversal function: after masking to the anti-diagonal,
/// reverse_bits correctly inverts the bit ordering within the isolated subset.
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

/// Returns true if the given square is attacked by any piece of the given color.
pub fn is_square_attacked(square: usize, attacking_color: Color, position: &Position) -> bool {
    let (
        attacking_pawns,
        attacking_knights,
        attacking_bishops,
        attacking_rooks,
        attacking_queens,
        attacking_king,
    ) = match attacking_color {
        Color::White => (
            position.white_pawns,
            position.white_knights,
            position.white_bishops,
            position.white_rooks,
            position.white_queens,
            position.white_king,
        ),
        Color::Black => (
            position.black_pawns,
            position.black_knights,
            position.black_bishops,
            position.black_rooks,
            position.black_queens,
            position.black_king,
        ),
    };

    // Pawn attacks: use pawn_attacks_for_square from the DEFENDING color's perspective
    // (if a defending pawn on `square` could attack a square that has an attacking pawn, then
    //  the attacking pawn attacks `square`)
    let defending_color = attacking_color.opponent();
    if pawn_attacks_for_square(square, defending_color) & attacking_pawns != 0 {
        return true;
    }

    if knight_attacks_for_square(square) & attacking_knights != 0 {
        return true;
    }
    if king_attacks_for_square(square) & attacking_king != 0 {
        return true;
    }

    let diagonal_attackers = bishop_attacks(square, position.all_occupancy);
    if diagonal_attackers & (attacking_bishops | attacking_queens) != 0 {
        return true;
    }

    let straight_attackers = rook_attacks(square, position.all_occupancy);
    if straight_attackers & (attacking_rooks | attacking_queens) != 0 {
        return true;
    }

    false
}

/// Generates all pseudo-legal moves for the side to move.
/// Pseudo-legal moves are valid in terms of piece movement rules
/// but may leave the king in check.
pub fn generate_pseudo_legal_moves(position: &Position) -> Vec<Move> {
    let mut moves = Vec::new();

    match position.side_to_move {
        Color::White => {
            generate_pawn_moves(
                position.white_pawns,
                position.black_occupancy,
                position.all_occupancy,
                position.en_passant_square,
                Color::White,
                &mut moves,
            );
            generate_knight_moves(position.white_knights, position.white_occupancy, &mut moves);
            generate_bishop_moves(position.white_bishops, position.white_occupancy, position.all_occupancy, &mut moves);
            generate_rook_moves(position.white_rooks, position.white_occupancy, position.all_occupancy, &mut moves);
            generate_queen_moves(position.white_queens, position.white_occupancy, position.all_occupancy, &mut moves);
            generate_king_moves(position.white_king, position.white_occupancy, position, Color::White, &mut moves);
        }
        Color::Black => {
            generate_pawn_moves(
                position.black_pawns,
                position.white_occupancy,
                position.all_occupancy,
                position.en_passant_square,
                Color::Black,
                &mut moves,
            );
            generate_knight_moves(position.black_knights, position.black_occupancy, &mut moves);
            generate_bishop_moves(position.black_bishops, position.black_occupancy, position.all_occupancy, &mut moves);
            generate_rook_moves(position.black_rooks, position.black_occupancy, position.all_occupancy, &mut moves);
            generate_queen_moves(position.black_queens, position.black_occupancy, position.all_occupancy, &mut moves);
            generate_king_moves(position.black_king, position.black_occupancy, position, Color::Black, &mut moves);
        }
    }

    moves
}

/// Iterates over all set bits in a bitboard, calling the callback with each square index.
fn for_each_set_bit(mut bitboard: u64, mut callback: impl FnMut(usize)) {
    while bitboard != 0 {
        let square = bitboard.trailing_zeros() as usize;
        callback(square);
        bitboard &= bitboard - 1; // clear the lowest set bit
    }
}

fn generate_pawn_moves(
    pawns: u64,
    enemy_occupancy: u64,
    all_occupancy: u64,
    en_passant_square: Option<u8>,
    color: Color,
    moves: &mut Vec<Move>,
) {
    match color {
        Color::White => {
            let single_pushes = (pawns << 8) & !all_occupancy;
            let double_pushes = ((single_pushes & RANK_MASKS[2]) << 8) & !all_occupancy;
            let left_captures  = (pawns & !FILE_MASKS[0]) << 7 & enemy_occupancy;
            let right_captures = (pawns & !FILE_MASKS[7]) << 9 & enemy_occupancy;

            // Single pushes (non-promotion)
            for_each_set_bit(single_pushes & !RANK_MASKS[7], |to_square| {
                moves.push(Move {
                    from_square: (to_square - 8) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::NONE,
                });
            });

            // Double pushes
            for_each_set_bit(double_pushes, |to_square| {
                moves.push(Move {
                    from_square: (to_square - 16) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::DOUBLE_PAWN_PUSH,
                });
            });

            // Promotions (single push to rank 8)
            for_each_set_bit(single_pushes & RANK_MASKS[7], |to_square| {
                for piece in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
                    moves.push(Move {
                        from_square: (to_square - 8) as u8,
                        to_square: to_square as u8,
                        promotion_piece: Some(piece),
                        move_flags: MoveFlags::NONE,
                    });
                }
            });

            // Left captures (non-promotion): white left = rank+1, file-1 = shift <<7
            for_each_set_bit(left_captures & !RANK_MASKS[7], |to_square| {
                moves.push(Move {
                    from_square: (to_square - 7) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::CAPTURE,
                });
            });

            // Right captures (non-promotion): white right = rank+1, file+1 = shift <<9
            for_each_set_bit(right_captures & !RANK_MASKS[7], |to_square| {
                moves.push(Move {
                    from_square: (to_square - 9) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::CAPTURE,
                });
            });

            // Capture promotions (left)
            for_each_set_bit(left_captures & RANK_MASKS[7], |to_square| {
                for piece in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
                    moves.push(Move {
                        from_square: (to_square - 7) as u8,
                        to_square: to_square as u8,
                        promotion_piece: Some(piece),
                        move_flags: MoveFlags::CAPTURE,
                    });
                }
            });

            // Capture promotions (right)
            for_each_set_bit(right_captures & RANK_MASKS[7], |to_square| {
                for piece in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
                    moves.push(Move {
                        from_square: (to_square - 9) as u8,
                        to_square: to_square as u8,
                        promotion_piece: Some(piece),
                        move_flags: MoveFlags::CAPTURE,
                    });
                }
            });

            // En passant
            if let Some(en_passant_square_index) = en_passant_square {
                let en_passant_bit = 1u64 << en_passant_square_index;
                let left_en_passant_attacker  = (en_passant_bit & !FILE_MASKS[0]) >> 1 & pawns;
                let right_en_passant_attacker = (en_passant_bit & !FILE_MASKS[7]) << 1 & pawns;
                for_each_set_bit(left_en_passant_attacker | right_en_passant_attacker, |from_square| {
                    moves.push(Move {
                        from_square: from_square as u8,
                        to_square: en_passant_square_index,
                        promotion_piece: None,
                        move_flags: MoveFlags::CAPTURE | MoveFlags::EN_PASSANT,
                    });
                });
            }
        }

        Color::Black => {
            let single_pushes = (pawns >> 8) & !all_occupancy;
            let double_pushes = ((single_pushes & RANK_MASKS[5]) >> 8) & !all_occupancy;
            let left_captures  = (pawns & !FILE_MASKS[7]) >> 7 & enemy_occupancy;
            let right_captures = (pawns & !FILE_MASKS[0]) >> 9 & enemy_occupancy;

            // Single pushes (non-promotion)
            for_each_set_bit(single_pushes & !RANK_MASKS[0], |to_square| {
                moves.push(Move {
                    from_square: (to_square + 8) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::NONE,
                });
            });

            // Double pushes
            for_each_set_bit(double_pushes, |to_square| {
                moves.push(Move {
                    from_square: (to_square + 16) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::DOUBLE_PAWN_PUSH,
                });
            });

            // Promotions (single push to rank 1)
            for_each_set_bit(single_pushes & RANK_MASKS[0], |to_square| {
                for piece in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
                    moves.push(Move {
                        from_square: (to_square + 8) as u8,
                        to_square: to_square as u8,
                        promotion_piece: Some(piece),
                        move_flags: MoveFlags::NONE,
                    });
                }
            });

            // Left captures (non-promotion): black "left" toward higher files, shift >>7
            for_each_set_bit(left_captures & !RANK_MASKS[0], |to_square| {
                moves.push(Move {
                    from_square: (to_square + 7) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::CAPTURE,
                });
            });

            // Right captures (non-promotion): black right toward lower files, shift >>9
            for_each_set_bit(right_captures & !RANK_MASKS[0], |to_square| {
                moves.push(Move {
                    from_square: (to_square + 9) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::CAPTURE,
                });
            });

            // Capture promotions (left)
            for_each_set_bit(left_captures & RANK_MASKS[0], |to_square| {
                for piece in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
                    moves.push(Move {
                        from_square: (to_square + 7) as u8,
                        to_square: to_square as u8,
                        promotion_piece: Some(piece),
                        move_flags: MoveFlags::CAPTURE,
                    });
                }
            });

            // Capture promotions (right)
            for_each_set_bit(right_captures & RANK_MASKS[0], |to_square| {
                for piece in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
                    moves.push(Move {
                        from_square: (to_square + 9) as u8,
                        to_square: to_square as u8,
                        promotion_piece: Some(piece),
                        move_flags: MoveFlags::CAPTURE,
                    });
                }
            });

            // En passant
            if let Some(en_passant_square_index) = en_passant_square {
                let en_passant_bit = 1u64 << en_passant_square_index;
                let left_en_passant_attacker  = (en_passant_bit & !FILE_MASKS[7]) << 1 & pawns;
                let right_en_passant_attacker = (en_passant_bit & !FILE_MASKS[0]) >> 1 & pawns;
                for_each_set_bit(left_en_passant_attacker | right_en_passant_attacker, |from_square| {
                    moves.push(Move {
                        from_square: from_square as u8,
                        to_square: en_passant_square_index,
                        promotion_piece: None,
                        move_flags: MoveFlags::CAPTURE | MoveFlags::EN_PASSANT,
                    });
                });
            }
        }
    }
}

fn generate_knight_moves(knights: u64, own_occupancy: u64, moves: &mut Vec<Move>) {
    for_each_set_bit(knights, |from_square| {
        let attacks = knight_attacks_for_square(from_square) & !own_occupancy;
        for_each_set_bit(attacks, |to_square| {
            moves.push(Move {
                from_square: from_square as u8,
                to_square: to_square as u8,
                promotion_piece: None,
                move_flags: MoveFlags::NONE,
            });
        });
    });
}

fn generate_bishop_moves(bishops: u64, own_occupancy: u64, all_occupancy: u64, moves: &mut Vec<Move>) {
    for_each_set_bit(bishops, |from_square| {
        let attacks = bishop_attacks(from_square, all_occupancy) & !own_occupancy;
        for_each_set_bit(attacks, |to_square| {
            moves.push(Move {
                from_square: from_square as u8,
                to_square: to_square as u8,
                promotion_piece: None,
                move_flags: MoveFlags::NONE,
            });
        });
    });
}

fn generate_rook_moves(rooks: u64, own_occupancy: u64, all_occupancy: u64, moves: &mut Vec<Move>) {
    for_each_set_bit(rooks, |from_square| {
        let attacks = rook_attacks(from_square, all_occupancy) & !own_occupancy;
        for_each_set_bit(attacks, |to_square| {
            moves.push(Move {
                from_square: from_square as u8,
                to_square: to_square as u8,
                promotion_piece: None,
                move_flags: MoveFlags::NONE,
            });
        });
    });
}

fn generate_queen_moves(queens: u64, own_occupancy: u64, all_occupancy: u64, moves: &mut Vec<Move>) {
    for_each_set_bit(queens, |from_square| {
        let attacks = queen_attacks(from_square, all_occupancy) & !own_occupancy;
        for_each_set_bit(attacks, |to_square| {
            moves.push(Move {
                from_square: from_square as u8,
                to_square: to_square as u8,
                promotion_piece: None,
                move_flags: MoveFlags::NONE,
            });
        });
    });
}

fn generate_king_moves(king: u64, own_occupancy: u64, position: &Position, color: Color, moves: &mut Vec<Move>) {
    let king_square = king.trailing_zeros() as usize;

    // Normal king moves
    let attacks = king_attacks_for_square(king_square) & !own_occupancy;
    for_each_set_bit(attacks, |to_square| {
        moves.push(Move {
            from_square: king_square as u8,
            to_square: to_square as u8,
            promotion_piece: None,
            move_flags: MoveFlags::NONE,
        });
    });

    // Castling: check empty squares AND that king path is not attacked
    let opponent = color.opponent();
    match color {
        Color::White => {
            // Kingside: f1(5) and g1(6) must be empty; e1(4),f1(5),g1(6) not attacked
            if position.castling_rights & (1 << 0) != 0
                && position.all_occupancy & 0x0000000000000060 == 0
                && !is_square_attacked(4, opponent, position)
                && !is_square_attacked(5, opponent, position)
                && !is_square_attacked(6, opponent, position)
            {
                moves.push(Move { from_square: 4, to_square: 6, promotion_piece: None, move_flags: MoveFlags::CASTLING });
            }
            // Queenside: b1(1),c1(2),d1(3) must be empty; e1(4),d1(3),c1(2) not attacked
            if position.castling_rights & (1 << 1) != 0
                && position.all_occupancy & 0x000000000000000E == 0
                && !is_square_attacked(4, opponent, position)
                && !is_square_attacked(3, opponent, position)
                && !is_square_attacked(2, opponent, position)
            {
                moves.push(Move { from_square: 4, to_square: 2, promotion_piece: None, move_flags: MoveFlags::CASTLING });
            }
        }
        Color::Black => {
            // Kingside: f8(61) and g8(62) must be empty; e8(60),f8(61),g8(62) not attacked
            if position.castling_rights & (1 << 2) != 0
                && position.all_occupancy & 0x6000000000000000 == 0
                && !is_square_attacked(60, opponent, position)
                && !is_square_attacked(61, opponent, position)
                && !is_square_attacked(62, opponent, position)
            {
                moves.push(Move { from_square: 60, to_square: 62, promotion_piece: None, move_flags: MoveFlags::CASTLING });
            }
            // Queenside: b8(57),c8(58),d8(59) must be empty; e8(60),d8(59),c8(58) not attacked
            if position.castling_rights & (1 << 3) != 0
                && position.all_occupancy & 0x0E00000000000000 == 0
                && !is_square_attacked(60, opponent, position)
                && !is_square_attacked(59, opponent, position)
                && !is_square_attacked(58, opponent, position)
            {
                moves.push(Move { from_square: 60, to_square: 58, promotion_piece: None, move_flags: MoveFlags::CASTLING });
            }
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

    #[test]
    fn start_position_has_sixteen_pseudo_legal_pawn_moves() {
        let position = crate::board::start_position();
        let moves = generate_pseudo_legal_moves(&position);
        let pawn_moves: Vec<_> = moves
            .iter()
            .filter(|m| {
                let from_bit = 1u64 << m.from_square;
                position.white_pawns & from_bit != 0
            })
            .collect();
        assert_eq!(pawn_moves.len(), 16, "8 single pushes + 8 double pushes");
    }

    #[test]
    fn start_position_has_four_pseudo_legal_knight_moves() {
        let position = crate::board::start_position();
        let moves = generate_pseudo_legal_moves(&position);
        let knight_moves: Vec<_> = moves
            .iter()
            .filter(|m| {
                let from_bit = 1u64 << m.from_square;
                position.white_knights & from_bit != 0
            })
            .collect();
        assert_eq!(knight_moves.len(), 4, "Nb1-a3, Nb1-c3, Ng1-f3, Ng1-h3");
    }
}
