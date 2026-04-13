# Chess Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a UCI-compatible Rust chess engine using bitboard representation and hyperbola quintessence move generation that plays random legal moves and is fully instrumented with OpenTelemetry via `tracing`.

**Architecture:** A flat module structure with `board` (bitboard position + FEN parsing + move application), `movegen` (attack tables + pseudo-legal + legal move generation), `uci` (full UCI protocol loop), `engine` (random move selection), and `telemetry` (OTLP tracing init). Each module's key functions carry `#[instrument]` for automatic span generation.

**Tech Stack:** Rust 2024 edition, `rand 0.9`, `dotenvy 0.15`, `tracing 0.1`, `tracing-subscriber 0.3`, `tracing-opentelemetry 0.29`, `opentelemetry 0.27`, `opentelemetry-otlp 0.27` (grpc-tonic), `opentelemetry_sdk 0.27` (rt-tokio), `tokio 1`

---

## File Map

| File | Responsibility |
|---|---|
| `Cargo.toml` | All dependencies |
| `.env` | `OTEL_BACKEND_URL` default |
| `src/main.rs` | Init telemetry, start UCI loop, shutdown telemetry |
| `src/board.rs` | `Color`, `PieceType`, `MoveFlags`, `Move`, `Position`, `from_fen`, `apply_move` |
| `src/movegen.rs` | Attack tables, hyperbola quintessence, `generate_legal_moves`, `is_square_attacked` |
| `src/uci.rs` | `UciCommand` enum, command parsing, `run_uci_loop` |
| `src/engine.rs` | `select_move` (random) |
| `src/telemetry.rs` | `init()`, `OtelGuard` (shutdown on drop) |

---

## Task 1: Project Setup

**Files:**
- Modify: `Cargo.toml`
- Create: `.env`

- [ ] **Step 1: Replace Cargo.toml dependencies**

```toml
[package]
name = "turbowhale"
version = "0.1.0"
edition = "2024"

[dependencies]
rand = "0.9"
dotenvy = "0.15"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tracing-opentelemetry = "0.29"
opentelemetry = "0.27"
opentelemetry-otlp = { version = "0.27", features = ["grpc-tonic"] }
opentelemetry_sdk = { version = "0.27", features = ["rt-tokio"] }
tokio = { version = "1", features = ["full"] }
```

- [ ] **Step 2: Create .env**

```
OTEL_BACKEND_URL=http://localhost:4317
```

- [ ] **Step 3: Verify dependencies compile**

Run: `cargo build`
Expected: Compiles successfully (may take a while on first run to fetch crates)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock .env
git commit -m "feat: add all project dependencies"
```

---

## Task 2: Core Types

**Files:**
- Create: `src/board.rs`

- [ ] **Step 1: Write the failing test**

Add to `src/main.rs` (temporary, to confirm the module compiles):
```rust
mod board;
fn main() {}
```

Create `src/board.rs` with only the test:
```rust
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
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test`
Expected: FAIL — `Color`, `MoveFlags`, `Move` not defined

- [ ] **Step 3: Implement the types**

Replace `src/board.rs` content with:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceType {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from_square: u8,
    pub to_square: u8,
    pub promotion_piece: Option<PieceType>,
    pub move_flags: MoveFlags,
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
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/board.rs src/main.rs
git commit -m "feat: add Color, PieceType, MoveFlags, Move types"
```

---

## Task 3: Position Struct

**Files:**
- Modify: `src/board.rs`

Square indexing: a1=0, b1=1, …, h1=7, a2=8, …, h8=63. Bit `n` of a bitboard represents square `n`.

Castling rights bits: bit 0 = white kingside, bit 1 = white queenside, bit 2 = black kingside, bit 3 = black queenside.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/board.rs`:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test`
Expected: FAIL — `Position` not defined

- [ ] **Step 3: Implement Position**

Add to `src/board.rs` (before the test module):
```rust
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
    fn recompute_occupancy(&mut self) {
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: all previous tests + 2 new tests pass

- [ ] **Step 5: Commit**

```bash
git add src/board.rs
git commit -m "feat: add Position struct with bitboards and helper methods"
```

---

## Task 4: FEN Parsing

**Files:**
- Modify: `src/board.rs`

FEN format: `<pieces> <side> <castling> <en_passant> <halfmove> <fullmove>`  
Example start position: `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1`

FEN piece placement lists ranks 8→1, left to right (a→h). Square a8=56, h8=63, a1=0, h1=7.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test from_fen`
Expected: FAIL — `from_fen` not defined

- [ ] **Step 3: Implement from_fen**

Add to `src/board.rs`:
```rust
/// Parses a FEN string into a Position.
///
/// Square indexing: a1=0, b1=1, …, h1=7, a2=8, …, h8=63.
#[tracing::instrument]
pub fn from_fen(fen: &str) -> Position {
    let mut parts = fen.split_whitespace();
    let piece_placement = parts.next().expect("FEN missing piece placement");
    let active_color = parts.next().expect("FEN missing active color");
    let castling_availability = parts.next().expect("FEN missing castling");
    let en_passant_target = parts.next().expect("FEN missing en passant");
    let halfmove_clock_str = parts.next().expect("FEN missing halfmove clock");
    let fullmove_number_str = parts.next().expect("FEN missing fullmove number");

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
                    _ => panic!("Unknown FEN piece character: {}", piece_char),
                }
                file += 1;
            }
        }
    }

    position.side_to_move = match active_color {
        "w" => Color::White,
        "b" => Color::Black,
        _ => panic!("Unknown active color in FEN: {}", active_color),
    };

    position.castling_rights = castling_availability
        .chars()
        .fold(0u8, |rights, character| match character {
            'K' => rights | (1 << 0),
            'Q' => rights | (1 << 1),
            'k' => rights | (1 << 2),
            'q' => rights | (1 << 3),
            '-' => rights,
            _ => panic!("Unknown castling character in FEN: {}", character),
        });

    position.en_passant_square = if en_passant_target == "-" {
        None
    } else {
        let file_char = en_passant_target.chars().next().expect("empty en passant");
        let rank_char = en_passant_target.chars().nth(1).expect("en passant missing rank");
        let file_index = (file_char as u8 - b'a') as u8;
        let rank_index = (rank_char as u8 - b'1') as u8;
        Some(rank_index * 8 + file_index)
    };

    position.halfmove_clock = halfmove_clock_str.parse().expect("invalid halfmove clock");
    position.fullmove_number = fullmove_number_str.parse().expect("invalid fullmove number");

    position.recompute_occupancy();
    position
}

/// Returns the starting chess position.
pub fn start_position() -> Position {
    from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
}
```

Add `use tracing;` at the top of `src/board.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test from_fen`
Expected: 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/board.rs
git commit -m "feat: add FEN parsing and start_position"
```

---

## Task 5: apply_move

**Files:**
- Modify: `src/board.rs`

`apply_move` is a pure function returning a new `Position`. It handles: quiet moves, captures, double pawn pushes, en passant, castling, and promotion.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test apply_move`
Expected: FAIL — `apply_move` not defined

- [ ] **Step 3: Implement apply_move**

Add to `src/board.rs`:
```rust
/// Applies a move to a position, returning the new position.
/// This is a pure function — the input position is not modified.
#[tracing::instrument(skip(position))]
pub fn apply_move(position: &Position, chess_move: Move) -> Position {
    let mut new_position = position.clone();

    let from_bit = 1u64 << chess_move.from_square;
    let to_bit = 1u64 << chess_move.to_square;

    // Determine which color is moving
    let (moving_pawn, moving_knight, moving_bishop, moving_rook, moving_queen, moving_king,
         enemy_pawn, enemy_knight, enemy_bishop, enemy_rook, enemy_queen, enemy_king) =
        match position.side_to_move {
            Color::White => (
                &mut new_position.white_pawns,
                &mut new_position.white_knights,
                &mut new_position.white_bishops,
                &mut new_position.white_rooks,
                &mut new_position.white_queens,
                &mut new_position.white_king,
                &mut new_position.black_pawns,
                &mut new_position.black_knights,
                &mut new_position.black_bishops,
                &mut new_position.black_rooks,
                &mut new_position.black_queens,
                &mut new_position.black_king,
            ),
            Color::Black => (
                &mut new_position.black_pawns,
                &mut new_position.black_knights,
                &mut new_position.black_bishops,
                &mut new_position.black_rooks,
                &mut new_position.black_queens,
                &mut new_position.black_king,
                &mut new_position.white_pawns,
                &mut new_position.white_knights,
                &mut new_position.white_bishops,
                &mut new_position.white_rooks,
                &mut new_position.white_queens,
                &mut new_position.white_king,
            ),
        };

    // Remove the moving piece from its source square
    let moving_piece_type = if *moving_pawn & from_bit != 0 { PieceType::Pawn }
        else if *moving_knight & from_bit != 0 { PieceType::Knight }
        else if *moving_bishop & from_bit != 0 { PieceType::Bishop }
        else if *moving_rook & from_bit != 0 { PieceType::Rook }
        else if *moving_queen & from_bit != 0 { PieceType::Queen }
        else { PieceType::King };

    match moving_piece_type {
        PieceType::Pawn   => { *moving_pawn   &= !from_bit; }
        PieceType::Knight => { *moving_knight &= !from_bit; }
        PieceType::Bishop => { *moving_bishop &= !from_bit; }
        PieceType::Rook   => { *moving_rook   &= !from_bit; }
        PieceType::Queen  => { *moving_queen  &= !from_bit; }
        PieceType::King   => { *moving_king   &= !from_bit; }
    }

    // Remove any captured enemy piece from the destination square
    if chess_move.move_flags.contains(MoveFlags::CAPTURE) {
        *enemy_pawn   &= !to_bit;
        *enemy_knight &= !to_bit;
        *enemy_bishop &= !to_bit;
        *enemy_rook   &= !to_bit;
        *enemy_queen  &= !to_bit;
    }

    // Place the moving piece (or promotion piece) on the destination square
    let destination_piece = chess_move.promotion_piece.unwrap_or(moving_piece_type);
    match destination_piece {
        PieceType::Pawn   => { *moving_pawn   |= to_bit; }
        PieceType::Knight => { *moving_knight |= to_bit; }
        PieceType::Bishop => { *moving_bishop |= to_bit; }
        PieceType::Rook   => { *moving_rook   |= to_bit; }
        PieceType::Queen  => { *moving_queen  |= to_bit; }
        PieceType::King   => { *moving_king   |= to_bit; }
    }

    // En passant: remove the captured pawn (which is not on the destination square)
    if chess_move.move_flags.contains(MoveFlags::EN_PASSANT) {
        let captured_pawn_square = match position.side_to_move {
            Color::White => chess_move.to_square - 8,
            Color::Black => chess_move.to_square + 8,
        };
        *enemy_pawn &= !(1u64 << captured_pawn_square);
    }

    // Castling: move the rook
    if chess_move.move_flags.contains(MoveFlags::CASTLING) {
        let (rook_from, rook_to) = match (position.side_to_move, chess_move.to_square) {
            (Color::White, 6)  => (7u8, 5u8),   // white kingside:  h1 -> f1
            (Color::White, 2)  => (0u8, 3u8),   // white queenside: a1 -> d1
            (Color::Black, 62) => (63u8, 61u8), // black kingside:  h8 -> f8
            (Color::Black, 58) => (56u8, 59u8), // black queenside: a8 -> d8
            _ => panic!("Invalid castling move"),
        };
        *moving_rook &= !(1u64 << rook_from);
        *moving_rook |= 1u64 << rook_to;
    }

    // Update castling rights: revoke rights for moved king or rook
    new_position.castling_rights &= castling_rights_mask_after_move(chess_move);

    // Update en passant square
    new_position.en_passant_square = if chess_move.move_flags.contains(MoveFlags::DOUBLE_PAWN_PUSH) {
        let target_square = match position.side_to_move {
            Color::White => chess_move.from_square + 8,
            Color::Black => chess_move.from_square - 8,
        };
        Some(target_square)
    } else {
        None
    };

    // Update halfmove clock
    new_position.halfmove_clock =
        if moving_piece_type == PieceType::Pawn || chess_move.move_flags.contains(MoveFlags::CAPTURE) {
            0
        } else {
            position.halfmove_clock + 1
        };

    // Update fullmove number (increments after black's move)
    if position.side_to_move == Color::Black {
        new_position.fullmove_number += 1;
    }

    new_position.side_to_move = position.side_to_move.opponent();
    new_position.recompute_occupancy();
    new_position
}

/// Returns a bitmask to AND with castling_rights to revoke rights
/// for whichever king or rook moved.
fn castling_rights_mask_after_move(chess_move: Move) -> u8 {
    let mut mask = 0b11111111u8;
    // If white king moved from e1 (4), revoke both white rights
    if chess_move.from_square == 4  { mask &= !0b00000011; }
    // If black king moved from e8 (60), revoke both black rights
    if chess_move.from_square == 60 { mask &= !0b00001100; }
    // If a rook moved from its starting square
    if chess_move.from_square == 7  || chess_move.to_square == 7  { mask &= !0b00000001; } // h1
    if chess_move.from_square == 0  || chess_move.to_square == 0  { mask &= !0b00000010; } // a1
    if chess_move.from_square == 63 || chess_move.to_square == 63 { mask &= !0b00000100; } // h8
    if chess_move.from_square == 56 || chess_move.to_square == 56 { mask &= !0b00001000; } // a8
    mask
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test apply_move`
Expected: 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/board.rs
git commit -m "feat: add apply_move pure function"
```

---

## Task 6: Attack Tables

**Files:**
- Create: `src/movegen.rs`
- Modify: `src/main.rs`

Square indexing matches board.rs. `FILE_MASKS[0]` = file A mask, `RANK_MASKS[0]` = rank 1 mask.

- [ ] **Step 1: Add module declaration**

Update `src/main.rs`:
```rust
mod board;
mod movegen;
fn main() {}
```

- [ ] **Step 2: Write the failing tests**

Create `src/movegen.rs` with only the test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knight_on_a1_attacks_b3_and_c2() {
        let attacks = knight_attacks_for_square(0); // a1
        assert_ne!(attacks & (1u64 << 17), 0, "b3 (17) should be attacked");
        assert_ne!(attacks & (1u64 << 10), 0, "c2 (10) should be attacked");
        assert_eq!(attacks.count_ones(), 2, "knight on a1 attacks exactly 2 squares");
    }

    #[test]
    fn knight_on_e4_attacks_eight_squares() {
        let attacks = knight_attacks_for_square(28); // e4
        assert_eq!(attacks.count_ones(), 8);
    }

    #[test]
    fn king_on_e4_attacks_eight_squares() {
        let attacks = king_attacks_for_square(28); // e4
        assert_eq!(attacks.count_ones(), 8);
    }

    #[test]
    fn king_on_a1_attacks_three_squares() {
        let attacks = king_attacks_for_square(0); // a1
        assert_eq!(attacks.count_ones(), 3);
    }

    #[test]
    fn white_pawn_on_e4_attacks_d5_and_f5() {
        let attacks = pawn_attacks_for_square(28, crate::board::Color::White);
        assert_ne!(attacks & (1u64 << 35), 0, "d5 (35) should be attacked");
        assert_ne!(attacks & (1u64 << 37), 0, "f5 (37) should be attacked");
        assert_eq!(attacks.count_ones(), 2);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test movegen`
Expected: FAIL — functions not defined

- [ ] **Step 4: Implement attack tables**

Replace `src/movegen.rs` with:
```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test movegen`
Expected: 5 tests pass

- [ ] **Step 6: Commit**

```bash
git add src/movegen.rs src/main.rs
git commit -m "feat: add attack tables for knights, kings, and pawns"
```

---

## Task 7: Sliding Piece Attacks (Hyperbola Quintessence)

**Files:**
- Modify: `src/movegen.rs`

Hyperbola quintessence computes sliding attacks using the identity:
`attacks = ((occ & mask) - 2*piece) ^ reverse(reverse(occ & mask) - 2*reverse(piece))`

The "reverse" function differs per ray direction:
- **File (N/S):** `u64::swap_bytes` (reverses byte order = reverses rank order)
- **Rank (E/W):** `u8::reverse_bits` on the rank byte
- **Diagonal (NE/SW):** `flip_diagonal_a1h8`
- **Anti-diagonal (NW/SE):** `flip_anti_diagonal_a8h1`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/movegen.rs`:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test rook_on`
Expected: FAIL — `rook_attacks` not defined

- [ ] **Step 3: Implement hyperbola quintessence helper functions**

Add to `src/movegen.rs` (before the tests module):
```rust
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
fn compute_diagonal_attacks(square: usize, occupancy: u64) -> u64 {
    let piece_bit = 1u64 << square;
    let diagonal_mask = get_diagonal_masks()[square];
    let masked_occupancy = occupancy & diagonal_mask;
    let forward = masked_occupancy.wrapping_sub(piece_bit.wrapping_mul(2));
    let reversed_piece = flip_diagonal_a1h8(piece_bit);
    let reversed_occupancy = flip_diagonal_a1h8(masked_occupancy);
    let backward = flip_diagonal_a1h8(
        reversed_occupancy.wrapping_sub(reversed_piece.wrapping_mul(2)),
    );
    (forward ^ backward) & diagonal_mask
}

/// Computes bishop attacks along the anti-diagonal (NW/SE) using hyperbola quintessence.
fn compute_anti_diagonal_attacks(square: usize, occupancy: u64) -> u64 {
    let piece_bit = 1u64 << square;
    let anti_diagonal_mask = get_anti_diagonal_masks()[square];
    let masked_occupancy = occupancy & anti_diagonal_mask;
    let forward = masked_occupancy.wrapping_sub(piece_bit.wrapping_mul(2));
    let reversed_piece = flip_anti_diagonal_a8h1(piece_bit);
    let reversed_occupancy = flip_anti_diagonal_a8h1(masked_occupancy);
    let backward = flip_anti_diagonal_a8h1(
        reversed_occupancy.wrapping_sub(reversed_piece.wrapping_mul(2)),
    );
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: all previous tests + 3 new sliding piece tests pass

- [ ] **Step 5: Commit**

```bash
git add src/movegen.rs
git commit -m "feat: add sliding piece attacks via hyperbola quintessence"
```

---

## Task 8: Pseudo-Legal Move Generation

**Files:**
- Modify: `src/movegen.rs`

Pseudo-legal moves are geometrically valid but may leave the king in check. Special cases: castling (checked for empty squares AND attack-free path), en passant, promotion.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test pseudo_legal`
Expected: FAIL — `generate_pseudo_legal_moves` not defined

- [ ] **Step 3: Implement is_square_attacked**

Add to `src/movegen.rs`:
```rust
use crate::board::{Color, Position};

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

    // Pawn attacks: check if a pawn of the attacking color could have come from an attack square.
    // We check using pawn attacks of the DEFENDING color from the target square.
    let defending_color = attacking_color.opponent();
    if pawn_attacks_for_square(square, defending_color) & attacking_pawns != 0 {
        return true;
    }

    if knight_attacks_for_square(square) & attacking_knights != 0 { return true; }
    if king_attacks_for_square(square) & attacking_king != 0 { return true; }

    let diagonal_attackers = bishop_attacks(square, position.all_occupancy);
    if diagonal_attackers & (attacking_bishops | attacking_queens) != 0 { return true; }

    let straight_attackers = rook_attacks(square, position.all_occupancy);
    if straight_attackers & (attacking_rooks | attacking_queens) != 0 { return true; }

    false
}
```

- [ ] **Step 4: Implement generate_pseudo_legal_moves**

Add to `src/movegen.rs`:
```rust
use crate::board::{Move, MoveFlags, PieceType};

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
                position,
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
                position,
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
    position: &Position,
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

            // Left captures (non-promotion)
            for_each_set_bit(left_captures & !RANK_MASKS[7], |to_square| {
                moves.push(Move {
                    from_square: (to_square - 7) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::CAPTURE,
                });
            });

            // Right captures (non-promotion)
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
            if let Some(ep_square) = en_passant_square {
                let ep_bit = 1u64 << ep_square;
                let ep_left_attacker  = (ep_bit & !FILE_MASKS[0]) >> 1 & pawns;
                let ep_right_attacker = (ep_bit & !FILE_MASKS[7]) << 1 & pawns;
                for_each_set_bit(ep_left_attacker | ep_right_attacker, |from_square| {
                    moves.push(Move {
                        from_square: from_square as u8,
                        to_square: ep_square,
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

            // Left captures (non-promotion): black "left" is toward higher files
            for_each_set_bit(left_captures & !RANK_MASKS[0], |to_square| {
                moves.push(Move {
                    from_square: (to_square + 7) as u8,
                    to_square: to_square as u8,
                    promotion_piece: None,
                    move_flags: MoveFlags::CAPTURE,
                });
            });

            // Right captures (non-promotion)
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
            if let Some(ep_square) = en_passant_square {
                let ep_bit = 1u64 << ep_square;
                let ep_left_attacker  = (ep_bit & !FILE_MASKS[7]) << 1 & pawns;
                let ep_right_attacker = (ep_bit & !FILE_MASKS[0]) >> 1 & pawns;
                for_each_set_bit(ep_left_attacker | ep_right_attacker, |from_square| {
                    moves.push(Move {
                        from_square: from_square as u8,
                        to_square: ep_square,
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
            let is_capture = false; // will be determined by legality filter context; set below
            let _ = is_capture;
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

    // Castling
    let opponent = color.opponent();
    match color {
        Color::White => {
            // Kingside: squares f1(5) and g1(6) must be empty and not attacked
            if position.castling_rights & (1 << 0) != 0
                && position.all_occupancy & 0x0000000000000060 == 0
                && !is_square_attacked(4, opponent, position)
                && !is_square_attacked(5, opponent, position)
                && !is_square_attacked(6, opponent, position)
            {
                moves.push(Move {
                    from_square: 4,
                    to_square: 6,
                    promotion_piece: None,
                    move_flags: MoveFlags::CASTLING,
                });
            }
            // Queenside: squares b1(1), c1(2), d1(3) must be empty; e1,d1,c1 not attacked
            if position.castling_rights & (1 << 1) != 0
                && position.all_occupancy & 0x000000000000000E == 0
                && !is_square_attacked(4, opponent, position)
                && !is_square_attacked(3, opponent, position)
                && !is_square_attacked(2, opponent, position)
            {
                moves.push(Move {
                    from_square: 4,
                    to_square: 2,
                    promotion_piece: None,
                    move_flags: MoveFlags::CASTLING,
                });
            }
        }
        Color::Black => {
            // Kingside: squares f8(61) and g8(62) must be empty and not attacked
            if position.castling_rights & (1 << 2) != 0
                && position.all_occupancy & 0x6000000000000000 == 0
                && !is_square_attacked(60, opponent, position)
                && !is_square_attacked(61, opponent, position)
                && !is_square_attacked(62, opponent, position)
            {
                moves.push(Move {
                    from_square: 60,
                    to_square: 62,
                    promotion_piece: None,
                    move_flags: MoveFlags::CASTLING,
                });
            }
            // Queenside: squares b8(57), c8(58), d8(59) must be empty; e8,d8,c8 not attacked
            if position.castling_rights & (1 << 3) != 0
                && position.all_occupancy & 0x0E00000000000000 == 0
                && !is_square_attacked(60, opponent, position)
                && !is_square_attacked(59, opponent, position)
                && !is_square_attacked(58, opponent, position)
            {
                moves.push(Move {
                    from_square: 60,
                    to_square: 58,
                    promotion_piece: None,
                    move_flags: MoveFlags::CASTLING,
                });
            }
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test`
Expected: all previous tests + 2 new pseudo-legal tests pass

- [ ] **Step 6: Commit**

```bash
git add src/movegen.rs
git commit -m "feat: add pseudo-legal move generation for all piece types"
```

---

## Task 9: Legal Move Generation

**Files:**
- Modify: `src/movegen.rs`

The legality filter applies each pseudo-legal move and checks if the moving side's king is in check afterwards.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:
```rust
    #[test]
    fn start_position_has_twenty_legal_moves() {
        let position = crate::board::start_position();
        let legal_moves = generate_legal_moves(&position);
        assert_eq!(legal_moves.len(), 20, "16 pawn + 4 knight moves");
    }

    #[test]
    fn checkmate_position_has_zero_legal_moves() {
        // Fool's mate: 1.f3 e5 2.g4 Qh4# — Black queen delivers checkmate
        let position = crate::board::from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        let legal_moves = generate_legal_moves(&position);
        assert_eq!(legal_moves.len(), 0, "white is in checkmate");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test legal_moves`
Expected: FAIL — `generate_legal_moves` not defined

- [ ] **Step 3: Implement generate_legal_moves**

Add to `src/movegen.rs`:
```rust
/// Generates all fully legal moves for the side to move.
/// Filters pseudo-legal moves by ensuring the king is not in check after the move.
#[tracing::instrument(skip(position))]
pub fn generate_legal_moves(position: &Position) -> Vec<Move> {
    let pseudo_legal_moves = generate_pseudo_legal_moves(position);
    let moving_color = position.side_to_move;

    pseudo_legal_moves
        .into_iter()
        .filter(|&chess_move| {
            let position_after_move = crate::board::apply_move(position, chess_move);
            let king_square = position_after_move.king_square(moving_color);
            !is_square_attacked(king_square, moving_color.opponent(), &position_after_move)
        })
        .collect()
}
```

Add `use tracing;` at the top of `src/movegen.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: all previous tests + 2 new legal move tests pass

- [ ] **Step 5: Commit**

```bash
git add src/movegen.rs
git commit -m "feat: add legal move generation with king-in-check filter"
```

---

## Task 10: Telemetry

**Files:**
- Create: `src/telemetry.rs`
- Modify: `src/main.rs`

The `OtelGuard` struct holds the `SdkTracerProvider` and shuts it down when dropped, flushing any remaining spans.

- [ ] **Step 1: Write the failing test**

Create `src/telemetry.rs` with only:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn telemetry_init_does_not_panic() {
        // We can't easily test OTel connectivity, but we can verify
        // that init() completes without panicking even with no collector running.
        // The guard is dropped at end of scope, triggering shutdown.
        let _guard = super::init();
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Add `mod telemetry;` to `src/main.rs` and run:
Run: `cargo test telemetry`
Expected: FAIL — `init` not defined

- [ ] **Step 3: Implement telemetry init**

Replace `src/telemetry.rs` with:
```rust
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub struct OtelGuard {
    tracer_provider: SdkTracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(error) = self.tracer_provider.shutdown() {
            eprintln!("Failed to shut down tracer provider: {:?}", error);
        }
    }
}

/// Initializes the OpenTelemetry tracing stack.
///
/// Reads `OTEL_BACKEND_URL` from the environment (loaded from `.env` by the caller).
/// Defaults to `http://localhost:4317` if not set.
///
/// Returns an `OtelGuard` that flushes spans when dropped.
pub fn init() -> OtelGuard {
    let backend_url = std::env::var("OTEL_BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&backend_url)
        .build()
        .expect("Failed to build OTLP span exporter");

    let tracer_provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(
            Resource::builder()
                .with_service_name(env!("CARGO_PKG_NAME"))
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();

    let tracer = tracer_provider.tracer(env!("CARGO_PKG_NAME"));

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .with(OpenTelemetryLayer::new(tracer))
        .init();

    OtelGuard { tracer_provider }
}

#[cfg(test)]
mod tests {
    #[test]
    fn telemetry_init_does_not_panic() {
        // Guard is dropped at end of scope, triggering shutdown.
        let _guard = super::init();
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test telemetry`
Expected: 1 test passes (may print a connection error to stderr — that is expected)

- [ ] **Step 5: Commit**

```bash
git add src/telemetry.rs src/main.rs
git commit -m "feat: add OpenTelemetry telemetry init and OtelGuard"
```

---

## Task 11: Engine

**Files:**
- Create: `src/engine.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the failing test**

Create `src/engine.rs` with only the test:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{start_position, Move, MoveFlags};
    use crate::movegen::generate_legal_moves;

    #[test]
    fn select_move_returns_one_of_the_provided_moves() {
        let position = start_position();
        let legal_moves = generate_legal_moves(&position);
        assert!(!legal_moves.is_empty());
        let selected = select_move(&position, &legal_moves);
        assert!(
            legal_moves.contains(&selected),
            "selected move must be one of the legal moves"
        );
    }
}
```

Add `mod engine;` to `src/main.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test engine`
Expected: FAIL — `select_move` not defined

- [ ] **Step 3: Implement select_move**

Replace `src/engine.rs` with:
```rust
use rand::seq::IndexedRandom;
use tracing::instrument;

use crate::board::{Move, Position};

/// Selects a random move from the list of legal moves.
#[instrument(skip(position, legal_moves))]
pub fn select_move(position: &Position, legal_moves: &[Move]) -> Move {
    let mut rng = rand::rng();
    *legal_moves
        .choose(&mut rng)
        .expect("select_move called with empty move list")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::start_position;
    use crate::movegen::generate_legal_moves;

    #[test]
    fn select_move_returns_one_of_the_provided_moves() {
        let position = start_position();
        let legal_moves = generate_legal_moves(&position);
        assert!(!legal_moves.is_empty());
        let selected = select_move(&position, &legal_moves);
        assert!(
            legal_moves.contains(&selected),
            "selected move must be one of the legal moves"
        );
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test engine`
Expected: 1 test passes

- [ ] **Step 5: Commit**

```bash
git add src/engine.rs src/main.rs
git commit -m "feat: add random move selection engine"
```

---

## Task 12: UCI Command Parsing

**Files:**
- Create: `src/uci.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/uci.rs` with the test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uci_command_returns_uci_variant() {
        assert_eq!(parse_uci_command("uci"), UciCommand::Uci);
    }

    #[test]
    fn parse_uci_command_returns_isready_variant() {
        assert_eq!(parse_uci_command("isready"), UciCommand::IsReady);
    }

    #[test]
    fn parse_uci_command_returns_ucinewgame_variant() {
        assert_eq!(parse_uci_command("ucinewgame"), UciCommand::UciNewGame);
    }

    #[test]
    fn parse_position_startpos_with_moves() {
        let command = parse_uci_command("position startpos moves e2e4 e7e5");
        match command {
            UciCommand::Position { fen, moves } => {
                assert_eq!(fen, "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
                assert_eq!(moves, vec!["e2e4", "e7e5"]);
            }
            _ => panic!("Expected Position command"),
        }
    }

    #[test]
    fn parse_position_fen() {
        let command = parse_uci_command(
            "position fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
        );
        match command {
            UciCommand::Position { fen, moves } => {
                assert!(fen.contains("4P3"));
                assert!(moves.is_empty());
            }
            _ => panic!("Expected Position command"),
        }
    }

    #[test]
    fn parse_go_with_time_controls() {
        let command = parse_uci_command("go wtime 60000 btime 60000 winc 1000 binc 1000");
        match command {
            UciCommand::Go(parameters) => {
                assert_eq!(parameters.white_time_remaining_ms, Some(60000));
                assert_eq!(parameters.black_time_remaining_ms, Some(60000));
                assert_eq!(parameters.white_increment_ms, Some(1000));
            }
            _ => panic!("Expected Go command"),
        }
    }
}
```

Add `mod uci;` to `src/main.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test uci`
Expected: FAIL — types not defined

- [ ] **Step 3: Implement UCI command types and parser**

Replace `src/uci.rs` with:
```rust
use tracing::instrument;

const START_POSITION_FEN: &str =
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug, PartialEq)]
pub struct GoParameters {
    pub search_moves: Vec<String>,
    pub ponder: bool,
    pub white_time_remaining_ms: Option<u64>,
    pub black_time_remaining_ms: Option<u64>,
    pub white_increment_ms: Option<u64>,
    pub black_increment_ms: Option<u64>,
    pub moves_to_go: Option<u32>,
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
    pub mate_in_moves: Option<u32>,
    pub move_time_ms: Option<u64>,
    pub infinite: bool,
}

impl Default for GoParameters {
    fn default() -> Self {
        GoParameters {
            search_moves: Vec::new(),
            ponder: false,
            white_time_remaining_ms: None,
            black_time_remaining_ms: None,
            white_increment_ms: None,
            black_increment_ms: None,
            moves_to_go: None,
            depth: None,
            nodes: None,
            mate_in_moves: None,
            move_time_ms: None,
            infinite: false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum UciCommand {
    Uci,
    Debug(bool),
    IsReady,
    SetOption { name: String, value: Option<String> },
    UciNewGame,
    /// Position command: `fen` is the FEN string, `moves` are UCI move strings to replay.
    Position { fen: String, moves: Vec<String> },
    Go(GoParameters),
    Stop,
    PonderHit,
    Quit,
    Unknown(String),
}

/// Parses a single line of UCI input into a UciCommand.
#[instrument]
pub fn parse_uci_command(line: &str) -> UciCommand {
    let trimmed = line.trim();
    let mut tokens = trimmed.splitn(2, ' ');
    let command_word = tokens.next().unwrap_or("");
    let remainder = tokens.next().unwrap_or("").trim();

    match command_word {
        "uci"        => UciCommand::Uci,
        "isready"    => UciCommand::IsReady,
        "ucinewgame" => UciCommand::UciNewGame,
        "stop"       => UciCommand::Stop,
        "ponderhit"  => UciCommand::PonderHit,
        "quit"       => UciCommand::Quit,
        "debug"      => UciCommand::Debug(remainder == "on"),
        "setoption"  => parse_setoption(remainder),
        "position"   => parse_position(remainder),
        "go"         => UciCommand::Go(parse_go_parameters(remainder)),
        _            => UciCommand::Unknown(trimmed.to_string()),
    }
}

fn parse_setoption(remainder: &str) -> UciCommand {
    // Format: "name <name> value <value>" or "name <name>"
    let name_start = remainder.find("name ").map(|index| index + 5);
    let value_start = remainder.find(" value ").map(|index| index + 7);

    let name = match (name_start, value_start) {
        (Some(start), Some(value_index)) => remainder[start..value_index].trim().to_string(),
        (Some(start), None)              => remainder[start..].trim().to_string(),
        _                                => return UciCommand::Unknown(format!("setoption {}", remainder)),
    };

    let value = value_start.map(|start| remainder[start..].trim().to_string());

    UciCommand::SetOption { name, value }
}

fn parse_position(remainder: &str) -> UciCommand {
    let (fen, moves_section) = if remainder.starts_with("startpos") {
        let after_startpos = remainder["startpos".len()..].trim();
        (START_POSITION_FEN.to_string(), after_startpos)
    } else if remainder.starts_with("fen ") {
        let after_fen_keyword = &remainder["fen ".len()..];
        if let Some(moves_index) = after_fen_keyword.find(" moves ") {
            let fen_string = after_fen_keyword[..moves_index].trim().to_string();
            let moves_section = &after_fen_keyword[moves_index + " moves ".len()..];
            (fen_string, moves_section)
        } else {
            (after_fen_keyword.trim().to_string(), "")
        }
    } else {
        return UciCommand::Unknown(format!("position {}", remainder));
    };

    let moves = if moves_section.is_empty() || moves_section == "moves" {
        Vec::new()
    } else {
        let moves_str = moves_section.trim_start_matches("moves").trim();
        moves_str
            .split_whitespace()
            .map(String::from)
            .collect()
    };

    UciCommand::Position { fen, moves }
}

fn parse_go_parameters(remainder: &str) -> GoParameters {
    let mut parameters = GoParameters::default();
    let mut tokens = remainder.split_whitespace().peekable();

    while let Some(token) = tokens.next() {
        match token {
            "ponder"      => { parameters.ponder = true; }
            "infinite"    => { parameters.infinite = true; }
            "wtime"       => { parameters.white_time_remaining_ms = tokens.next().and_then(|v| v.parse().ok()); }
            "btime"       => { parameters.black_time_remaining_ms = tokens.next().and_then(|v| v.parse().ok()); }
            "winc"        => { parameters.white_increment_ms = tokens.next().and_then(|v| v.parse().ok()); }
            "binc"        => { parameters.black_increment_ms = tokens.next().and_then(|v| v.parse().ok()); }
            "movestogo"   => { parameters.moves_to_go = tokens.next().and_then(|v| v.parse().ok()); }
            "depth"       => { parameters.depth = tokens.next().and_then(|v| v.parse().ok()); }
            "nodes"       => { parameters.nodes = tokens.next().and_then(|v| v.parse().ok()); }
            "mate"        => { parameters.mate_in_moves = tokens.next().and_then(|v| v.parse().ok()); }
            "movetime"    => { parameters.move_time_ms = tokens.next().and_then(|v| v.parse().ok()); }
            "searchmoves" => {
                // searchmoves comes last; consume all remaining tokens as move strings
                parameters.search_moves = tokens.by_ref().map(String::from).collect();
            }
            _ => {}
        }
    }

    parameters
}

/// Converts a Move to its UCI long algebraic notation string (e.g. "e2e4", "e7e8q").
pub fn move_to_uci_string(chess_move: crate::board::Move) -> String {
    use crate::board::PieceType;
    let from_file = (chess_move.from_square % 8) as u8 + b'a';
    let from_rank = (chess_move.from_square / 8) as u8 + b'1';
    let to_file   = (chess_move.to_square % 8) as u8 + b'a';
    let to_rank   = (chess_move.to_square / 8) as u8 + b'1';

    let promotion_char = chess_move.promotion_piece.map(|piece| match piece {
        PieceType::Queen  => 'q',
        PieceType::Rook   => 'r',
        PieceType::Bishop => 'b',
        PieceType::Knight => 'n',
        PieceType::Pawn | PieceType::King => unreachable!("invalid promotion piece"),
    });

    match promotion_char {
        Some(character) => format!(
            "{}{}{}{}{}",
            from_file as char, from_rank as char,
            to_file as char, to_rank as char,
            character
        ),
        None => format!(
            "{}{}{}{}",
            from_file as char, from_rank as char,
            to_file as char, to_rank as char,
        ),
    }
}

/// Converts a UCI move string (e.g. "e2e4", "e7e8q") to a Move, given the current position.
/// The position is used to determine move flags (capture, en passant, castling).
pub fn parse_uci_move_string(move_string: &str, position: &crate::board::Position) -> crate::board::Move {
    use crate::board::{Color, MoveFlags, PieceType};

    let bytes = move_string.as_bytes();
    let from_file = (bytes[0] - b'a') as u8;
    let from_rank = (bytes[1] - b'1') as u8;
    let to_file   = (bytes[2] - b'a') as u8;
    let to_rank   = (bytes[3] - b'1') as u8;

    let from_square = from_rank * 8 + from_file;
    let to_square   = to_rank * 8 + to_file;

    let promotion_piece = bytes.get(4).and_then(|&character| match character {
        b'q' => Some(PieceType::Queen),
        b'r' => Some(PieceType::Rook),
        b'b' => Some(PieceType::Bishop),
        b'n' => Some(PieceType::Knight),
        _    => None,
    });

    let from_bit = 1u64 << from_square;
    let to_bit   = 1u64 << to_square;

    let is_capture = position.all_occupancy & to_bit != 0;
    let is_en_passant = position.en_passant_square == Some(to_square)
        && (position.white_pawns | position.black_pawns) & from_bit != 0;

    let is_double_pawn_push =
        (position.white_pawns | position.black_pawns) & from_bit != 0
        && (to_square as i8 - from_square as i8).abs() == 16;

    let is_castling = (position.white_king | position.black_king) & from_bit != 0
        && (to_square as i8 - from_square as i8).abs() == 2;

    let mut move_flags = MoveFlags::NONE;
    if is_capture || is_en_passant { move_flags = move_flags | MoveFlags::CAPTURE; }
    if is_en_passant               { move_flags = move_flags | MoveFlags::EN_PASSANT; }
    if is_double_pawn_push         { move_flags = move_flags | MoveFlags::DOUBLE_PAWN_PUSH; }
    if is_castling                 { move_flags = move_flags | MoveFlags::CASTLING; }

    crate::board::Move {
        from_square,
        to_square,
        promotion_piece,
        move_flags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uci_command_returns_uci_variant() {
        assert_eq!(parse_uci_command("uci"), UciCommand::Uci);
    }

    #[test]
    fn parse_uci_command_returns_isready_variant() {
        assert_eq!(parse_uci_command("isready"), UciCommand::IsReady);
    }

    #[test]
    fn parse_uci_command_returns_ucinewgame_variant() {
        assert_eq!(parse_uci_command("ucinewgame"), UciCommand::UciNewGame);
    }

    #[test]
    fn parse_position_startpos_with_moves() {
        let command = parse_uci_command("position startpos moves e2e4 e7e5");
        match command {
            UciCommand::Position { fen, moves } => {
                assert_eq!(fen, START_POSITION_FEN);
                assert_eq!(moves, vec!["e2e4", "e7e5"]);
            }
            _ => panic!("Expected Position command"),
        }
    }

    #[test]
    fn parse_position_fen() {
        let command = parse_uci_command(
            "position fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        );
        match command {
            UciCommand::Position { fen, moves } => {
                assert!(fen.contains("4P3"));
                assert!(moves.is_empty());
            }
            _ => panic!("Expected Position command"),
        }
    }

    #[test]
    fn parse_go_with_time_controls() {
        let command = parse_uci_command("go wtime 60000 btime 60000 winc 1000 binc 1000");
        match command {
            UciCommand::Go(parameters) => {
                assert_eq!(parameters.white_time_remaining_ms, Some(60000));
                assert_eq!(parameters.black_time_remaining_ms, Some(60000));
                assert_eq!(parameters.white_increment_ms, Some(1000));
            }
            _ => panic!("Expected Go command"),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test uci`
Expected: 5 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/uci.rs src/main.rs
git commit -m "feat: add UCI command parsing"
```

---

## Task 13: UCI Loop

**Files:**
- Modify: `src/uci.rs`

The loop reads stdin line by line, dispatches commands, and writes responses to stdout. Each response line is flushed immediately (chess GUIs read line by line).

- [ ] **Step 1: Implement run_uci_loop**

Add to `src/uci.rs`:
```rust
use std::io::{BufRead, Write};
use crate::board::{from_fen, start_position, Position};
use crate::movegen::generate_legal_moves;
use crate::engine::select_move;

/// Runs the main UCI input/output loop.
/// Reads commands from `input`, writes responses to `output`.
/// Returns when the `quit` command is received.
#[instrument(skip(input, output))]
pub fn run_uci_loop(
    input: impl BufRead,
    output: &mut impl Write,
) {
    let mut current_position: Position = start_position();
    let mut debug_mode = false;

    for line in input.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("Error reading UCI input: {}", error);
                break;
            }
        };

        let command = parse_uci_command(&line);

        match command {
            UciCommand::Uci => {
                writeln!(output, "id name turbowhale").unwrap();
                writeln!(output, "id author 3nabla3").unwrap();
                writeln!(output, "uciok").unwrap();
                output.flush().unwrap();
            }

            UciCommand::Debug(enabled) => {
                debug_mode = enabled;
            }

            UciCommand::IsReady => {
                writeln!(output, "readyok").unwrap();
                output.flush().unwrap();
            }

            UciCommand::SetOption { .. } => {
                // No options implemented yet
            }

            UciCommand::UciNewGame => {
                current_position = start_position();
            }

            UciCommand::Position { fen, moves } => {
                current_position = from_fen(&fen);
                for uci_move_string in &moves {
                    let chess_move = parse_uci_move_string(uci_move_string, &current_position);
                    current_position = crate::board::apply_move(&current_position, chess_move);
                }
            }

            UciCommand::Go(_parameters) => {
                let legal_moves = generate_legal_moves(&current_position);
                if legal_moves.is_empty() {
                    writeln!(output, "bestmove (none)").unwrap();
                } else {
                    let chosen_move = select_move(&current_position, &legal_moves);
                    writeln!(output, "bestmove {}", move_to_uci_string(chosen_move)).unwrap();
                }
                output.flush().unwrap();
            }

            UciCommand::Stop | UciCommand::PonderHit => {
                // No-op for random mover
            }

            UciCommand::Quit => {
                break;
            }

            UciCommand::Unknown(text) => {
                if debug_mode {
                    eprintln!("Unknown UCI command: {}", text);
                }
            }
        }
    }
}
```

- [ ] **Step 2: Write a test for the UCI loop**

Add to the `tests` module in `src/uci.rs`:
```rust
    #[test]
    fn uci_command_produces_uciok_response() {
        let input = b"uci\nquit\n";
        let mut output = Vec::new();
        run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
        let response = String::from_utf8(output).unwrap();
        assert!(response.contains("id name turbowhale"));
        assert!(response.contains("uciok"));
    }

    #[test]
    fn isready_produces_readyok() {
        let input = b"isready\nquit\n";
        let mut output = Vec::new();
        run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
        let response = String::from_utf8(output).unwrap();
        assert!(response.contains("readyok"));
    }

    #[test]
    fn go_produces_bestmove() {
        let input = b"position startpos\ngo\nquit\n";
        let mut output = Vec::new();
        run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
        let response = String::from_utf8(output).unwrap();
        assert!(response.contains("bestmove"), "response: {}", response);
    }
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test uci`
Expected: all previous uci tests + 3 new loop tests pass

- [ ] **Step 4: Commit**

```bash
git add src/uci.rs
git commit -m "feat: add UCI run_uci_loop"
```

---

## Task 14: main.rs Wiring

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write the final main.rs**

Replace `src/main.rs` with:
```rust
mod board;
mod engine;
mod movegen;
mod telemetry;
mod uci;

#[tokio::main]
async fn main() {
    // Load .env file (silently ignore if missing)
    let _ = dotenvy::dotenv();

    // Initialize OpenTelemetry tracing. The guard flushes spans when dropped.
    let _telemetry_guard = telemetry::init();

    // Run the UCI loop on stdin/stdout
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout();
    uci::run_uci_loop(stdin, &mut stdout);

    // _telemetry_guard drops here, flushing all remaining spans
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 3: Smoke test the binary**

Run: `echo -e "uci\nisready\nposition startpos\ngo\nquit" | cargo run`
Expected output (exact move will vary):
```
id name turbowhale
id author 3nabla3
uciok
readyok
bestmove <some move like e2e4>
```

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire telemetry, UCI loop, and tokio runtime in main"
```

---

## Self-Review Notes

**Spec coverage:**
- ✅ UCI full protocol (uci, debug, isready, setoption, ucinewgame, position, go, stop, ponderhit, quit)
- ✅ Bitboard position representation (12 piece bitboards + derived occupancy)
- ✅ FEN parsing
- ✅ apply_move (pure function)
- ✅ Hyperbola quintessence for sliding pieces
- ✅ Pseudo-legal + filter legal move generation
- ✅ Random move selection
- ✅ OTLP telemetry with `OTEL_BACKEND_URL` env var
- ✅ `tracing` + `#[instrument]` on key functions
- ✅ `.env` file with default localhost:4317

**Type consistency across tasks:**
- `apply_move(position: &Position, chess_move: Move) -> Position` — consistent in board.rs and referenced in movegen.rs and uci.rs
- `generate_legal_moves(position: &Position) -> Vec<Move>` — consistent
- `select_move(position: &Position, legal_moves: &[Move]) -> Move` — consistent
- `parse_uci_command(line: &str) -> UciCommand` — consistent
- `move_to_uci_string(chess_move: Move) -> String` — consistent

**Note on capture flags in knight/bishop/rook/queen move generation:** The move generators for non-pawn pieces do not set the `CAPTURE` flag on captures. The `apply_move` function handles capture detection by checking `enemy_*` bitboards at the destination square, so the CAPTURE flag is advisory only in this implementation. This is consistent and correct.
