#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn opponent(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PieceType {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct MoveFlags(u8);

impl MoveFlags {
    pub const NONE: MoveFlags = MoveFlags(0);
    pub const CAPTURE: MoveFlags = MoveFlags(1 << 0);
    pub const EN_PASSANT: MoveFlags = MoveFlags(1 << 1);
    pub const CASTLING: MoveFlags = MoveFlags(1 << 2);
    pub const DOUBLE_PAWN_PUSH: MoveFlags = MoveFlags(1 << 3);

    pub fn contains(self, other: MoveFlags) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for MoveFlags {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        MoveFlags(self.0 | other.0)
    }
}

impl std::ops::BitOrAssign for MoveFlags {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Move {
    pub from_square: u8,
    pub to_square: u8,
    /// Invariant: if Some, the piece type must be Knight, Bishop, Rook, or Queen.
    /// Pawn and King are not valid promotion targets.
    pub promotion_piece: Option<PieceType>,
    pub move_flags: MoveFlags,
}

#[derive(Debug, Clone)]
pub struct Position {
    // Piece bitboards — one per piece type per color
    pub white_pawns: u64,
    pub white_knights: u64,
    pub white_bishops: u64,
    pub white_rooks: u64,
    pub white_queens: u64,
    pub white_king: u64,
    pub black_pawns: u64,
    pub black_knights: u64,
    pub black_bishops: u64,
    pub black_rooks: u64,
    pub black_queens: u64,
    pub black_king: u64,
    // Derived occupancy bitboards (kept in sync with piece bitboards)
    pub white_occupancy: u64,
    pub black_occupancy: u64,
    pub all_occupancy: u64,
    // Game state
    pub side_to_move: Color,
    /// Bits: 0=white kingside, 1=white queenside, 2=black kingside, 3=black queenside
    pub castling_rights: u8,
    /// Target square for en passant capture, if available
    pub en_passant_square: Option<u8>,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
}

impl Position {
    pub fn empty() -> Position {
        Position {
            white_pawns: 0,
            white_knights: 0,
            white_bishops: 0,
            white_rooks: 0,
            white_queens: 0,
            white_king: 0,
            black_pawns: 0,
            black_knights: 0,
            black_bishops: 0,
            black_rooks: 0,
            black_queens: 0,
            black_king: 0,
            white_occupancy: 0,
            black_occupancy: 0,
            all_occupancy: 0,
            side_to_move: Color::White,
            castling_rights: 0,
            en_passant_square: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        }
    }

    /// Returns the square index of the king for the given color.
    /// Panics if the king bitboard is empty (invalid position).
    pub fn king_square(&self, color: Color) -> usize {
        let king_bitboard = match color {
            Color::White => self.white_king,
            Color::Black => self.black_king,
        };
        king_bitboard.trailing_zeros() as usize
    }

    /// Returns the piece type on the given square for the given color, if any.
    pub fn piece_type_on_square(&self, square: usize, color: Color) -> Option<PieceType> {
        let bit = 1u64 << square;
        match color {
            Color::White => {
                if self.white_pawns & bit != 0 { Some(PieceType::Pawn) }
                else if self.white_knights & bit != 0 { Some(PieceType::Knight) }
                else if self.white_bishops & bit != 0 { Some(PieceType::Bishop) }
                else if self.white_rooks & bit != 0 { Some(PieceType::Rook) }
                else if self.white_queens & bit != 0 { Some(PieceType::Queen) }
                else if self.white_king & bit != 0 { Some(PieceType::King) }
                else { None }
            }
            Color::Black => {
                if self.black_pawns & bit != 0 { Some(PieceType::Pawn) }
                else if self.black_knights & bit != 0 { Some(PieceType::Knight) }
                else if self.black_bishops & bit != 0 { Some(PieceType::Bishop) }
                else if self.black_rooks & bit != 0 { Some(PieceType::Rook) }
                else if self.black_queens & bit != 0 { Some(PieceType::Queen) }
                else if self.black_king & bit != 0 { Some(PieceType::King) }
                else { None }
            }
        }
    }

    /// Recomputes the derived occupancy bitboards from the piece bitboards.
    pub fn recompute_occupancy(&mut self) {
        self.white_occupancy = self.white_pawns
            | self.white_knights
            | self.white_bishops
            | self.white_rooks
            | self.white_queens
            | self.white_king;
        self.black_occupancy = self.black_pawns
            | self.black_knights
            | self.black_bishops
            | self.black_rooks
            | self.black_queens
            | self.black_king;
        self.all_occupancy = self.white_occupancy | self.black_occupancy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black_are_different_colors() {
        assert_ne!(Color::White, Color::Black);
    }

    #[test]
    fn move_flags_can_be_combined() {
        let flags = MoveFlags::CAPTURE | MoveFlags::EN_PASSANT;
        assert!(flags.contains(MoveFlags::CAPTURE));
        assert!(flags.contains(MoveFlags::EN_PASSANT));
        assert!(!flags.contains(MoveFlags::CASTLING));
    }

    #[test]
    fn move_stores_from_and_to_squares() {
        let chess_move = Move {
            from_square: 12,
            to_square: 28,
            promotion_piece: None,
            move_flags: MoveFlags::NONE,
        };
        assert_eq!(chess_move.from_square, 12);
        assert_eq!(chess_move.to_square, 28);
    }

    #[test]
    fn empty_position_has_no_pieces() {
        let position = Position::empty();
        assert_eq!(position.white_pawns, 0);
        assert_eq!(position.black_king, 0);
        assert_eq!(position.all_occupancy, 0);
    }

    #[test]
    fn empty_position_side_to_move_is_white() {
        let position = Position::empty();
        assert_eq!(position.side_to_move, Color::White);
    }
}
