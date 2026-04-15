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

#[derive(Clone)]
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
    /// Incrementally-maintained middlegame PST score (White minus Black, centipawns).
    /// Updated by apply_move on every move; initialised by from_fen and start_position.
    pub middlegame_score: i32,
    /// Incrementally-maintained endgame PST score (White minus Black, centipawns).
    pub endgame_score: i32,
    /// Current game phase (0 = full endgame, 24 = full opening material on the board).
    pub game_phase: i32,
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
            middlegame_score: 0,
            endgame_score: 0,
            game_phase: 0,
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
}

/// Renders a Position as a FEN string for readable output in traces and logs.
impl std::fmt::Debug for Position {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Piece placement: rank 8 (index 7) down to rank 1 (index 0)
        let mut fen = String::new();
        for rank in (0..8).rev() {
            let mut empty_count: u8 = 0;
            for file in 0..8u8 {
                let bit = 1u64 << (rank * 8 + file as usize);
                let piece_char =
                    if self.white_pawns & bit != 0        { Some('P') }
                    else if self.white_knights & bit != 0 { Some('N') }
                    else if self.white_bishops & bit != 0 { Some('B') }
                    else if self.white_rooks & bit != 0   { Some('R') }
                    else if self.white_queens & bit != 0  { Some('Q') }
                    else if self.white_king & bit != 0    { Some('K') }
                    else if self.black_pawns & bit != 0   { Some('p') }
                    else if self.black_knights & bit != 0 { Some('n') }
                    else if self.black_bishops & bit != 0 { Some('b') }
                    else if self.black_rooks & bit != 0   { Some('r') }
                    else if self.black_queens & bit != 0  { Some('q') }
                    else if self.black_king & bit != 0    { Some('k') }
                    else                                  { None };
                match piece_char {
                    Some(character) => {
                        if empty_count > 0 {
                            fen.push((b'0' + empty_count) as char);
                            empty_count = 0;
                        }
                        fen.push(character);
                    }
                    None => empty_count += 1,
                }
            }
            if empty_count > 0 {
                fen.push((b'0' + empty_count) as char);
            }
            if rank > 0 {
                fen.push('/');
            }
        }

        // Side to move
        let side_char = match self.side_to_move {
            Color::White => 'w',
            Color::Black => 'b',
        };

        // Castling rights
        let castling = if self.castling_rights == 0 {
            "-".to_string()
        } else {
            let mut castling_string = String::new();
            if self.castling_rights & (1 << 0) != 0 { castling_string.push('K'); }
            if self.castling_rights & (1 << 1) != 0 { castling_string.push('Q'); }
            if self.castling_rights & (1 << 2) != 0 { castling_string.push('k'); }
            if self.castling_rights & (1 << 3) != 0 { castling_string.push('q'); }
            castling_string
        };

        // En passant square
        let en_passant = match self.en_passant_square {
            None => "-".to_string(),
            Some(square) => {
                let file = square % 8;
                let rank = square / 8;
                format!("{}{}", (b'a' + file) as char, (b'1' + rank) as char)
            }
        };

        write!(
            formatter,
            "{} {} {} {} {} {}",
            fen, side_char, castling, en_passant,
            self.halfmove_clock, self.fullmove_number
        )
    }
}

/// Parses a FEN string into a Position, returning an error message on failure.
///
/// Square indexing: a1=0, b1=1, …, h1=7, a2=8, …, h8=63.
pub fn try_from_fen(fen: &str) -> Result<Position, String> {
    let mut parts = fen.split_whitespace();
    let piece_placement = parts.next().ok_or("FEN missing piece placement")?;
    let active_color = parts.next().ok_or("FEN missing active color")?;
    let castling_availability = parts.next().ok_or("FEN missing castling availability")?;
    let en_passant_target = parts.next().ok_or("FEN missing en passant target")?;
    let halfmove_clock_str = parts.next().ok_or("FEN missing halfmove clock")?;
    let fullmove_number_str = parts.next().ok_or("FEN missing fullmove number")?;

    let mut position = Position::empty();

    // Parse piece placement: FEN lists rank 8 first (index 7), down to rank 1 (index 0)
    let mut rank: i32 = 7;
    let mut file: i32 = 0;
    for character in piece_placement.chars() {
        match character {
            '/' => {
                rank -= 1;
                file = 0;
            }
            '1'..='8' => {
                file += character as i32 - '0' as i32;
            }
            piece_char => {
                if !(0..=7).contains(&rank) || !(0..=7).contains(&file) {
                    return Err(format!("FEN piece placement out of bounds at rank={} file={}", rank, file));
                }
                let square = (rank * 8 + file) as usize;
                let bit = 1u64 << square;
                match piece_char {
                    'P' => position.white_pawns |= bit,
                    'N' => position.white_knights |= bit,
                    'B' => position.white_bishops |= bit,
                    'R' => position.white_rooks |= bit,
                    'Q' => position.white_queens |= bit,
                    'K' => position.white_king |= bit,
                    'p' => position.black_pawns |= bit,
                    'n' => position.black_knights |= bit,
                    'b' => position.black_bishops |= bit,
                    'r' => position.black_rooks |= bit,
                    'q' => position.black_queens |= bit,
                    'k' => position.black_king |= bit,
                    _ => return Err(format!("Unknown FEN piece character: {}", piece_char)),
                }
                file += 1;
            }
        }
    }

    position.side_to_move = match active_color {
        "w" => Color::White,
        "b" => Color::Black,
        _ => return Err(format!("Unknown active color in FEN: {}", active_color)),
    };

    let mut castling_rights = 0u8;
    for character in castling_availability.chars() {
        match character {
            'K' => castling_rights |= 1 << 0,
            'Q' => castling_rights |= 1 << 1,
            'k' => castling_rights |= 1 << 2,
            'q' => castling_rights |= 1 << 3,
            '-' => {}
            _ => return Err(format!("Unknown castling character in FEN: {}", character)),
        }
    }
    position.castling_rights = castling_rights;

    position.en_passant_square = if en_passant_target == "-" {
        None
    } else {
        let mut en_passant_chars = en_passant_target.chars();
        let file_char = en_passant_chars.next().ok_or("empty en passant target")?;
        let rank_char = en_passant_chars.next().ok_or("en passant target missing rank")?;
        if !file_char.is_ascii_lowercase() || !(b'a'..=b'h').contains(&(file_char as u8)) {
            return Err(format!("Invalid en passant file: {}", file_char));
        }
        if !('1'..='8').contains(&rank_char) {
            return Err(format!("Invalid en passant rank: {}", rank_char));
        }
        let file_index = file_char as u8 - b'a';
        let rank_index = rank_char as u8 - b'1';
        Some(rank_index * 8 + file_index)
    };

    position.halfmove_clock = halfmove_clock_str
        .parse()
        .map_err(|_| format!("Invalid halfmove clock: {}", halfmove_clock_str))?;
    position.fullmove_number = fullmove_number_str
        .parse()
        .map_err(|_| format!("Invalid fullmove number: {}", fullmove_number_str))?;

    position.recompute_occupancy();
    position.recompute_incremental_scores();
    Ok(position)
}

/// Parses a FEN string into a Position.
///
/// Square indexing: a1=0, b1=1, …, h1=7, a2=8, …, h8=63.
///
/// Panics on malformed input. Use [`try_from_fen`] for untrusted input.
#[tracing::instrument]
pub fn from_fen(fen: &str) -> Position {
    try_from_fen(fen).unwrap_or_else(|error| panic!("Invalid FEN \"{}\": {}", fen, error))
}

/// Returns the starting chess position.
pub fn start_position() -> Position {
    from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
}

/// Applies a move to a position, returning the new position.
/// This is a pure function — the input position is not modified.
#[tracing::instrument(level = "debug", skip(position))]
pub fn apply_move(position: &Position, chess_move: Move) -> Position {
    let mut new_position = position.clone();

    let from_bit = 1u64 << chess_move.from_square;
    let to_bit = 1u64 << chess_move.to_square;

    // Determine the type of the moving piece
    let moving_piece_type = if position.white_pawns & from_bit != 0 || position.black_pawns & from_bit != 0 {
        PieceType::Pawn
    } else if position.white_knights & from_bit != 0 || position.black_knights & from_bit != 0 {
        PieceType::Knight
    } else if position.white_bishops & from_bit != 0 || position.black_bishops & from_bit != 0 {
        PieceType::Bishop
    } else if position.white_rooks & from_bit != 0 || position.black_rooks & from_bit != 0 {
        PieceType::Rook
    } else if position.white_queens & from_bit != 0 || position.black_queens & from_bit != 0 {
        PieceType::Queen
    } else {
        PieceType::King
    };

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

    // Remove the moving piece from its source square
    match (position.side_to_move, moving_piece_type) {
        (Color::White, PieceType::Pawn)   => new_position.white_pawns   &= !from_bit,
        (Color::White, PieceType::Knight) => new_position.white_knights &= !from_bit,
        (Color::White, PieceType::Bishop) => new_position.white_bishops &= !from_bit,
        (Color::White, PieceType::Rook)   => new_position.white_rooks   &= !from_bit,
        (Color::White, PieceType::Queen)  => new_position.white_queens  &= !from_bit,
        (Color::White, PieceType::King)   => new_position.white_king    &= !from_bit,
        (Color::Black, PieceType::Pawn)   => new_position.black_pawns   &= !from_bit,
        (Color::Black, PieceType::Knight) => new_position.black_knights &= !from_bit,
        (Color::Black, PieceType::Bishop) => new_position.black_bishops &= !from_bit,
        (Color::Black, PieceType::Rook)   => new_position.black_rooks   &= !from_bit,
        (Color::Black, PieceType::Queen)  => new_position.black_queens  &= !from_bit,
        (Color::Black, PieceType::King)   => new_position.black_king    &= !from_bit,
    }
    new_position.update_piece_square_scores(
        moving_piece_type,
        position.side_to_move,
        chess_move.from_square as usize,
        -1,
    );

    // Remove any captured enemy piece from the destination square.
    // Always clear enemy pieces at the destination regardless of MoveFlags::CAPTURE, because
    // sliding piece (rook, bishop, queen) and knight move generators do not set the CAPTURE flag
    // even when the move lands on an enemy piece. En passant is handled separately below.
    if !chess_move.move_flags.contains(MoveFlags::EN_PASSANT) {
        match position.side_to_move {
            Color::White => {
                new_position.black_pawns   &= !to_bit;
                new_position.black_knights &= !to_bit;
                new_position.black_bishops &= !to_bit;
                new_position.black_rooks   &= !to_bit;
                new_position.black_queens  &= !to_bit;
            }
            Color::Black => {
                new_position.white_pawns   &= !to_bit;
                new_position.white_knights &= !to_bit;
                new_position.white_bishops &= !to_bit;
                new_position.white_rooks   &= !to_bit;
                new_position.white_queens  &= !to_bit;
            }
        }
        if let Some(piece_type) = captured_piece_type {
            new_position.update_piece_square_scores(
                piece_type,
                position.side_to_move.opponent(),
                chess_move.to_square as usize,
                -1,
            );
        }
    }

    // Place the moving piece (or promotion piece) on the destination square
    let destination_piece_type = chess_move.promotion_piece.unwrap_or(moving_piece_type);
    match (position.side_to_move, destination_piece_type) {
        (Color::White, PieceType::Pawn)   => new_position.white_pawns   |= to_bit,
        (Color::White, PieceType::Knight) => new_position.white_knights |= to_bit,
        (Color::White, PieceType::Bishop) => new_position.white_bishops |= to_bit,
        (Color::White, PieceType::Rook)   => new_position.white_rooks   |= to_bit,
        (Color::White, PieceType::Queen)  => new_position.white_queens  |= to_bit,
        (Color::White, PieceType::King)   => new_position.white_king    |= to_bit,
        (Color::Black, PieceType::Pawn)   => new_position.black_pawns   |= to_bit,
        (Color::Black, PieceType::Knight) => new_position.black_knights |= to_bit,
        (Color::Black, PieceType::Bishop) => new_position.black_bishops |= to_bit,
        (Color::Black, PieceType::Rook)   => new_position.black_rooks   |= to_bit,
        (Color::Black, PieceType::Queen)  => new_position.black_queens  |= to_bit,
        (Color::Black, PieceType::King)   => new_position.black_king    |= to_bit,
    }
    new_position.update_piece_square_scores(
        destination_piece_type,
        position.side_to_move,
        chess_move.to_square as usize,
        1,
    );

    // En passant: remove the captured pawn (which is NOT on the destination square)
    if chess_move.move_flags.contains(MoveFlags::EN_PASSANT) {
        let captured_pawn_square = match position.side_to_move {
            Color::White => chess_move.to_square - 8,
            Color::Black => chess_move.to_square + 8,
        };
        match position.side_to_move {
            Color::White => new_position.black_pawns &= !(1u64 << captured_pawn_square),
            Color::Black => new_position.white_pawns &= !(1u64 << captured_pawn_square),
        }
        new_position.update_piece_square_scores(
            PieceType::Pawn,
            position.side_to_move.opponent(),
            captured_pawn_square as usize,
            -1,
        );
    }

    // Castling: move the rook to its new square
    if chess_move.move_flags.contains(MoveFlags::CASTLING) {
        let (rook_from_square, rook_to_square) = match (position.side_to_move, chess_move.to_square) {
            (Color::White, 6)  => (7u8, 5u8),   // white kingside:  h1 -> f1
            (Color::White, 2)  => (0u8, 3u8),   // white queenside: a1 -> d1
            (Color::Black, 62) => (63u8, 61u8), // black kingside:  h8 -> f8
            (Color::Black, 58) => (56u8, 59u8), // black queenside: a8 -> d8
            _ => panic!("Invalid castling destination square: {}", chess_move.to_square),
        };
        match position.side_to_move {
            Color::White => {
                new_position.white_rooks &= !(1u64 << rook_from_square);
                new_position.white_rooks |= 1u64 << rook_to_square;
            }
            Color::Black => {
                new_position.black_rooks &= !(1u64 << rook_from_square);
                new_position.black_rooks |= 1u64 << rook_to_square;
            }
        }
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
    }

    // Update castling rights: revoke rights for any moved king or rook
    new_position.castling_rights &= castling_rights_mask_after_move(chess_move);

    // Update en passant square: set for double pawn push, clear otherwise
    new_position.en_passant_square = if chess_move.move_flags.contains(MoveFlags::DOUBLE_PAWN_PUSH) {
        let en_passant_target_square = match position.side_to_move {
            Color::White => chess_move.from_square + 8,
            Color::Black => chess_move.from_square - 8,
        };
        Some(en_passant_target_square)
    } else {
        None
    };

    // Update halfmove clock: reset on pawn move or capture, otherwise increment
    new_position.halfmove_clock =
        if moving_piece_type == PieceType::Pawn || chess_move.move_flags.contains(MoveFlags::CAPTURE) {
            0
        } else {
            position.halfmove_clock + 1
        };

    // Fullmove number increments after black's move
    if position.side_to_move == Color::Black {
        new_position.fullmove_number += 1;
    }

    new_position.side_to_move = position.side_to_move.opponent();
    new_position.game_phase = new_position.game_phase.clamp(0, crate::eval::MAX_GAME_PHASE);
    new_position.recompute_occupancy();
    new_position
}

/// Returns a bitmask to AND with castling_rights after a move to revoke any lost rights.
fn castling_rights_mask_after_move(chess_move: Move) -> u8 {
    let mut mask = 0b11111111u8;
    // King moves: revoke both rights for that color
    if chess_move.from_square == 4  { mask &= !0b00000011; } // white king from e1
    if chess_move.from_square == 60 { mask &= !0b00001100; } // black king from e8
    // Rook moves or captures: revoke the specific rook's right
    if chess_move.from_square == 7  || chess_move.to_square == 7  { mask &= !0b00000001; } // h1
    if chess_move.from_square == 0  || chess_move.to_square == 0  { mask &= !0b00000010; } // a1
    if chess_move.from_square == 63 || chess_move.to_square == 63 { mask &= !0b00000100; } // h8
    if chess_move.from_square == 56 || chess_move.to_square == 56 { mask &= !0b00001000; } // a8
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn apply_move_keeps_incremental_scores_consistent_with_recompute() {
        use crate::movegen::generate_legal_moves;
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

    #[test]
    fn from_fen_parses_start_position_white_pieces() {
        let position = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        // White pawns on rank 2 (squares 8–15)
        assert_eq!(position.white_pawns, 0x000000000000FF00);
        // White rooks on a1 (0) and h1 (7)
        assert_eq!(position.white_rooks, 0x0000000000000081);
        // White king on e1 (4)
        assert_eq!(position.white_king, 1u64 << 4);
    }

    #[test]
    fn from_fen_parses_start_position_state() {
        let position = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert_eq!(position.side_to_move, Color::White);
        assert_eq!(position.castling_rights, 0b00001111); // all four castling rights
        assert_eq!(position.en_passant_square, None);
        assert_eq!(position.halfmove_clock, 0);
        assert_eq!(position.fullmove_number, 1);
    }

    #[test]
    fn from_fen_parses_en_passant_square() {
        // After 1.e4 — en passant target is e3 (square 20)
        let position = from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
        assert_eq!(position.en_passant_square, Some(20)); // e3 = rank2*8+file4 = 2*8+4 = 20
    }

    #[test]
    fn apply_move_white_pawn_single_push() {
        let position = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        // e2 (square 12) -> e3 (square 20)
        let chess_move = Move {
            from_square: 12,
            to_square: 20,
            promotion_piece: None,
            move_flags: MoveFlags::NONE,
        };
        let new_position = apply_move(&position, chess_move);
        assert_eq!(new_position.white_pawns & (1u64 << 12), 0, "pawn should have left e2");
        assert_ne!(new_position.white_pawns & (1u64 << 20), 0, "pawn should be on e3");
        assert_eq!(new_position.side_to_move, Color::Black);
        assert_eq!(new_position.en_passant_square, None);
    }

    #[test]
    fn apply_move_white_pawn_double_push_sets_en_passant() {
        let position = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        // e2 (12) -> e4 (28)
        let chess_move = Move {
            from_square: 12,
            to_square: 28,
            promotion_piece: None,
            move_flags: MoveFlags::DOUBLE_PAWN_PUSH,
        };
        let new_position = apply_move(&position, chess_move);
        assert_eq!(new_position.en_passant_square, Some(20)); // e3 is the target
    }

    #[test]
    fn apply_move_capture_removes_captured_piece() {
        // White pawn on e5 (36), black pawn on d6 (43). White captures d6.
        let position = from_fen("8/8/3p4/4P3/8/8/8/8 w - - 0 1");
        let chess_move = Move {
            from_square: 36, // e5
            to_square: 43,   // d6
            promotion_piece: None,
            move_flags: MoveFlags::CAPTURE,
        };
        let new_position = apply_move(&position, chess_move);
        assert_eq!(new_position.black_pawns & (1u64 << 43), 0, "black pawn on d6 should be captured");
        assert_ne!(new_position.white_pawns & (1u64 << 43), 0, "white pawn should be on d6");
    }
}
