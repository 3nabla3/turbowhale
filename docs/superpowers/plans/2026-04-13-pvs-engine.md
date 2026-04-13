# PVS Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the random-move engine with NegaMax/PVS search, transposition table, quiescence search, and iterative deepening so the engine plays real chess.

**Architecture:** Four-task sequence: static eval → transposition table → search algorithm → UCI threading. Each task compiles and has tests before moving on. The search is single-threaded; the UCI loop runs on a separate I/O thread sharing an `Arc<AtomicBool>` stop flag and `Arc<Mutex<TranspositionTable>>`.

**Tech Stack:** Rust, no new dependencies. Uses `std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}`, `std::thread`, `std::time::{Duration, Instant}`.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/eval.rs` | Create | `evaluate(position) -> i32` — material count in centipawns from side-to-move's perspective |
| `src/tt.rs` | Create | Zobrist hashing, `TranspositionTable`, `TtEntry`, `NodeType` |
| `src/engine.rs` | Rewrite | `select_move`, `negamax_pvs`, `quiescence_search`, `SearchContext`, `SearchLimits` |
| `src/uci.rs` | Modify | Add `Arc<AtomicBool>` + `Arc<Mutex<TranspositionTable>>`; spawn search thread on `go`; join on `stop`/`quit` |
| `src/main.rs` | Modify | Register `mod eval; mod tt;` |

---

## Task 1: Static Evaluation

**Files:**
- Create: `src/eval.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Register the module in `src/main.rs`**

Add `mod eval;` after `mod engine;`:

```rust
mod board;
mod engine;
mod eval;
mod movegen;
mod telemetry;
mod uci;
```

- [ ] **Step 2: Write failing tests in `src/eval.rs`**

```rust
use crate::board::{Color, Position};

pub fn evaluate(_position: &Position) -> i32 {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{from_fen, start_position};

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
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test eval:: 2>&1 | head -20
```

Expected: compile error (todo!() panics) or test failures.

- [ ] **Step 4: Implement `evaluate`**

```rust
use crate::board::{Color, Position};

const PAWN_VALUE: i32 = 100;
const KNIGHT_VALUE: i32 = 320;
const BISHOP_VALUE: i32 = 330;
const ROOK_VALUE: i32 = 500;
const QUEEN_VALUE: i32 = 900;

pub fn evaluate(position: &Position) -> i32 {
    let white_material = count_material(
        position.white_pawns,
        position.white_knights,
        position.white_bishops,
        position.white_rooks,
        position.white_queens,
    );
    let black_material = count_material(
        position.black_pawns,
        position.black_knights,
        position.black_bishops,
        position.black_rooks,
        position.black_queens,
    );
    let absolute_score = white_material - black_material;
    match position.side_to_move {
        Color::White => absolute_score,
        Color::Black => -absolute_score,
    }
}

fn count_material(pawns: u64, knights: u64, bishops: u64, rooks: u64, queens: u64) -> i32 {
    pawns.count_ones() as i32   * PAWN_VALUE
        + knights.count_ones() as i32 * KNIGHT_VALUE
        + bishops.count_ones() as i32 * BISHOP_VALUE
        + rooks.count_ones() as i32   * ROOK_VALUE
        + queens.count_ones() as i32  * QUEEN_VALUE
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test eval::
```

Expected: all 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/eval.rs src/main.rs
git commit -m "feat: add material-only static evaluation"
```

---

## Task 2: Transposition Table

**Files:**
- Create: `src/tt.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Register the module in `src/main.rs`**

```rust
mod board;
mod engine;
mod eval;
mod movegen;
mod telemetry;
mod tt;
mod uci;
```

- [ ] **Step 2: Write failing tests in `src/tt.rs`**

```rust
use crate::board::{Color, Move, MoveFlags, PieceType, Position};

// --- Types ---

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeType {
    Exact,
    LowerBound,
    UpperBound,
}

#[derive(Clone, Copy, Debug)]
pub struct TtEntry {
    pub hash: u64,
    pub depth: u8,
    pub score: i32,
    pub best_move: Move,
    pub node_type: NodeType,
}

pub struct TranspositionTable {
    entries: Vec<Option<TtEntry>>,
    size: usize,
}

impl TranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        todo!()
    }

    pub fn clear(&mut self) {
        todo!()
    }

    pub fn probe(&self, hash: u64) -> Option<TtEntry> {
        todo!()
    }

    pub fn store(&mut self, hash: u64, entry: TtEntry) {
        todo!()
    }
}

pub fn compute_hash(position: &Position) -> u64 {
    todo!()
}

// --- Zobrist tables (lazily initialised) ---

struct ZobristKeys {
    pieces: [[[u64; 64]; 6]; 2], // [color][piece_type][square]
    black_to_move: u64,
    castling: [u64; 16],
    en_passant_file: [u64; 8],
}

fn zobrist_keys() -> &'static ZobristKeys {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{from_fen, start_position};

    #[test]
    fn start_position_hash_is_nonzero() {
        let position = start_position();
        assert_ne!(compute_hash(&position), 0);
    }

    #[test]
    fn same_position_same_hash() {
        let a = start_position();
        let b = start_position();
        assert_eq!(compute_hash(&a), compute_hash(&b));
    }

    #[test]
    fn different_positions_different_hashes() {
        let a = start_position();
        let b = from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
        assert_ne!(compute_hash(&a), compute_hash(&b));
    }

    #[test]
    fn side_to_move_changes_hash() {
        let white = from_fen("8/8/8/8/8/8/8/4K3 w - - 0 1");
        let black = from_fen("8/8/8/8/8/8/8/4K3 b - - 0 1");
        assert_ne!(compute_hash(&white), compute_hash(&black));
    }

    #[test]
    fn probe_returns_none_on_empty_table() {
        let table = TranspositionTable::new(1);
        assert!(table.probe(12345).is_none());
    }

    #[test]
    fn store_then_probe_returns_entry() {
        let mut table = TranspositionTable::new(1);
        let dummy_move = Move {
            from_square: 12,
            to_square: 20,
            promotion_piece: None,
            move_flags: MoveFlags::NONE,
        };
        let entry = TtEntry {
            hash: 0xDEADBEEF,
            depth: 4,
            score: 150,
            best_move: dummy_move,
            node_type: NodeType::Exact,
        };
        table.store(0xDEADBEEF, entry);
        let retrieved = table.probe(0xDEADBEEF).expect("should find entry");
        assert_eq!(retrieved.score, 150);
        assert_eq!(retrieved.depth, 4);
        assert_eq!(retrieved.node_type, NodeType::Exact);
    }

    #[test]
    fn probe_returns_none_on_hash_collision() {
        let mut table = TranspositionTable::new(1);
        let dummy_move = Move {
            from_square: 12,
            to_square: 20,
            promotion_piece: None,
            move_flags: MoveFlags::NONE,
        };
        // Store entry with hash A
        let entry = TtEntry {
            hash: 0xAAAA,
            depth: 4,
            score: 150,
            best_move: dummy_move,
            node_type: NodeType::Exact,
        };
        table.store(0xAAAA, entry);
        // Probe with different hash that maps to same slot — should reject
        // Use a hash that differs but same index: hash_b % size == hash_a % size
        // With size=131072 (1MB table), 0xAAAA and 0xAAAA + size have same slot
        let size = table.size;
        let colliding_hash = 0xAAAAu64.wrapping_add(size as u64);
        assert!(table.probe(colliding_hash).is_none());
    }

    #[test]
    fn clear_removes_all_entries() {
        let mut table = TranspositionTable::new(1);
        let dummy_move = Move {
            from_square: 12,
            to_square: 20,
            promotion_piece: None,
            move_flags: MoveFlags::NONE,
        };
        table.store(0xDEADBEEF, TtEntry {
            hash: 0xDEADBEEF,
            depth: 4,
            score: 150,
            best_move: dummy_move,
            node_type: NodeType::Exact,
        });
        table.clear();
        assert!(table.probe(0xDEADBEEF).is_none());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test tt:: 2>&1 | head -20
```

Expected: compile errors or panics from `todo!()`.

- [ ] **Step 4: Implement `ZobristKeys` and `zobrist_keys()`**

Add after the `ZobristKeys` struct definition:

```rust
use std::sync::OnceLock;

static ZOBRIST_KEYS: OnceLock<ZobristKeys> = OnceLock::new();

fn zobrist_keys() -> &'static ZobristKeys {
    ZOBRIST_KEYS.get_or_init(|| {
        // LCG-based deterministic PRNG seeded with a fixed constant.
        // Using a simple but sufficient generator — no need for crypto quality here.
        let mut state: u64 = 0x123456789ABCDEF0;
        let mut next = move || -> u64 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        let mut pieces = [[[0u64; 64]; 6]; 2];
        for color in 0..2 {
            for piece in 0..6 {
                for square in 0..64 {
                    pieces[color][piece][square] = next();
                }
            }
        }
        let black_to_move = next();
        let castling = std::array::from_fn(|_| next());
        let en_passant_file = std::array::from_fn(|_| next());

        ZobristKeys { pieces, black_to_move, castling, en_passant_file }
    })
}
```

- [ ] **Step 5: Implement `compute_hash`**

```rust
pub fn compute_hash(position: &Position) -> u64 {
    use crate::board::PieceType;
    let keys = zobrist_keys();

    // piece index: [White=0, Black=1][Pawn=0, Knight=1, Bishop=2, Rook=3, Queen=4, King=5]
    let mut hash = 0u64;

    let piece_boards: [(usize, usize, u64); 12] = [
        (0, 0, position.white_pawns),
        (0, 1, position.white_knights),
        (0, 2, position.white_bishops),
        (0, 3, position.white_rooks),
        (0, 4, position.white_queens),
        (0, 5, position.white_king),
        (1, 0, position.black_pawns),
        (1, 1, position.black_knights),
        (1, 2, position.black_bishops),
        (1, 3, position.black_rooks),
        (1, 4, position.black_queens),
        (1, 5, position.black_king),
    ];

    for (color, piece, mut bitboard) in piece_boards {
        while bitboard != 0 {
            let square = bitboard.trailing_zeros() as usize;
            hash ^= keys.pieces[color][piece][square];
            bitboard &= bitboard - 1;
        }
    }

    if position.side_to_move == Color::Black {
        hash ^= keys.black_to_move;
    }

    hash ^= keys.castling[position.castling_rights as usize];

    if let Some(ep_square) = position.en_passant_square {
        let file = (ep_square % 8) as usize;
        hash ^= keys.en_passant_file[file];
    }

    hash
}
```

- [ ] **Step 6: Implement `TranspositionTable`**

```rust
impl TranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        // Round down to nearest power of two for fast modulo via bitmasking
        let entry_bytes = std::mem::size_of::<Option<TtEntry>>();
        let target_entries = (size_mb * 1024 * 1024) / entry_bytes;
        let size = target_entries.next_power_of_two() / 2; // round down
        let size = size.max(1);
        TranspositionTable {
            entries: vec![None; size],
            size,
        }
    }

    pub fn clear(&mut self) {
        self.entries.iter_mut().for_each(|entry| *entry = None);
    }

    pub fn probe(&self, hash: u64) -> Option<TtEntry> {
        let index = (hash as usize) & (self.size - 1);
        self.entries[index].filter(|entry| entry.hash == hash)
    }

    pub fn store(&mut self, hash: u64, entry: TtEntry) {
        let index = (hash as usize) & (self.size - 1);
        self.entries[index] = Some(entry);
    }
}
```

- [ ] **Step 7: Run tests**

```bash
cargo test tt::
```

Expected: all 8 tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/tt.rs src/main.rs
git commit -m "feat: add Zobrist hashing and transposition table"
```

---

## Task 3: PVS Search Engine

**Files:**
- Rewrite: `src/engine.rs`

This is the largest task. Read the full file before editing.

- [ ] **Step 1: Write failing tests at the top of `src/engine.rs`**

Replace the entire file with the test skeleton first:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::board::{Color, Move, Position};
use crate::eval::evaluate;
use crate::movegen::{generate_legal_moves, generate_pseudo_legal_moves, is_square_attacked};
use crate::tt::{compute_hash, NodeType, TranspositionTable, TtEntry};
use crate::uci::GoParameters;

pub const MATE_SCORE: i32 = 100_000;
const INF: i32 = 200_000;

#[derive(Clone)]
pub enum SearchLimits {
    Depth(u32),
    MoveTime(Duration),
    Infinite,
    Clock { budget: Duration },
}

pub struct SearchContext<'a> {
    pub transposition_table: &'a mut TranspositionTable,
    pub stop_flag: &'a AtomicBool,
    pub limits: SearchLimits,
    pub start_time: Instant,
    pub nodes_searched: u64,
}

/// Selects the best move for the current position using iterative deepening PVS.
pub fn select_move(
    position: &Position,
    go_parameters: &GoParameters,
    transposition_table: &mut TranspositionTable,
    stop_flag: &AtomicBool,
) -> Move {
    todo!()
}

fn compute_search_limits(go_parameters: &GoParameters, side_to_move: Color) -> SearchLimits {
    todo!()
}

fn negamax_pvs(
    position: &Position,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
    todo!()
}

fn quiescence_search(
    position: &Position,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
    todo!()
}

fn mvv_lva_score(position: &crate::board::Position, chess_move: Move) -> i32 {
    todo!()
}

fn piece_value(piece_type: crate::board::PieceType) -> i32 {
    todo!()
}

fn order_moves(
    moves: Vec<Move>,
    position: &Position,
    tt_best_move: Option<Move>,
) -> Vec<Move> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{from_fen, start_position};
    use crate::uci::GoParameters;
    use std::sync::atomic::AtomicBool;

    fn make_tt() -> TranspositionTable {
        TranspositionTable::new(4)
    }

    fn make_stop() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn select_move_returns_legal_move_from_start_position() {
        let position = start_position();
        let legal_moves = generate_legal_moves(&position);
        let mut tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(1), ..Default::default() };
        let chosen = select_move(&position, &params, &mut tt, &stop);
        assert!(legal_moves.contains(&chosen), "selected move must be legal");
    }

    #[test]
    fn select_move_finds_mate_in_one() {
        // White to move, Qh5# is checkmate
        // Position: white queen on d1, king on e1; black king on e8, no pieces blocking
        let position = from_fen("4k3/8/8/8/8/8/8/3QK3 w - - 0 1");
        let mut tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, &mut tt, &stop);
        // Apply the chosen move and verify opponent has no legal moves
        let after = crate::board::apply_move(&position, chosen);
        let opponent_moves = generate_legal_moves(&after);
        assert!(
            opponent_moves.is_empty(),
            "engine should deliver checkmate in one, got move {:?}",
            chosen
        );
    }

    #[test]
    fn select_move_captures_hanging_queen() {
        // White can take a free queen on e5 — should capture it
        // White: king e1, rook a1. Black: king e8, queen e5
        let position = from_fen("4k3/8/8/4q3/8/8/8/R3K3 w Q - 0 1");
        let mut tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, &mut tt, &stop);
        assert_eq!(chosen.to_square, 36, "should capture queen on e5 (square 36)");
    }

    #[test]
    fn negamax_returns_zero_for_stalemate() {
        // Black king in stalemate: black king on a8, white king on b6, white queen on c7
        let position = from_fen("k7/2Q5/1K6/8/8/8/8/8 b - - 0 1");
        let mut tt = make_tt();
        let stop = make_stop();
        let mut context = SearchContext {
            transposition_table: &mut tt,
            stop_flag: &stop,
            limits: SearchLimits::Depth(4),
            start_time: Instant::now(),
            nodes_searched: 0,
        };
        let score = negamax_pvs(&position, 4, -INF, INF, 0, &mut context);
        assert_eq!(score, 0, "stalemate should score 0");
    }

    #[test]
    fn negamax_detects_checkmate() {
        // Fool's mate: white is in checkmate
        let position = from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        let mut tt = make_tt();
        let stop = make_stop();
        let mut context = SearchContext {
            transposition_table: &mut tt,
            stop_flag: &stop,
            limits: SearchLimits::Depth(1),
            start_time: Instant::now(),
            nodes_searched: 0,
        };
        let score = negamax_pvs(&position, 1, -INF, INF, 0, &mut context);
        assert!(score < -MATE_SCORE / 2, "checkmate position should return a large negative score, got {}", score);
    }
}
```

- [ ] **Step 2: Run to verify tests fail**

```bash
cargo test engine:: 2>&1 | head -30
```

Expected: compile errors (todo!() stubs).

- [ ] **Step 3: Implement helpers — `piece_value`, `mvv_lva_score`, `order_moves`**

```rust
fn piece_value(piece_type: crate::board::PieceType) -> i32 {
    use crate::board::PieceType;
    match piece_type {
        PieceType::Pawn   => 100,
        PieceType::Knight => 320,
        PieceType::Bishop => 330,
        PieceType::Rook   => 500,
        PieceType::Queen  => 900,
        PieceType::King   => 20_000,
    }
}

fn mvv_lva_score(position: &Position, chess_move: Move) -> i32 {
    use crate::board::PieceType;
    let from_bit = 1u64 << chess_move.from_square;
    let to_bit   = 1u64 << chess_move.to_square;

    let attacker_type = [
        (position.white_pawns | position.black_pawns,     PieceType::Pawn),
        (position.white_knights | position.black_knights, PieceType::Knight),
        (position.white_bishops | position.black_bishops, PieceType::Bishop),
        (position.white_rooks | position.black_rooks,     PieceType::Rook),
        (position.white_queens | position.black_queens,   PieceType::Queen),
        (position.white_king | position.black_king,       PieceType::King),
    ]
    .iter()
    .find(|(bb, _)| bb & from_bit != 0)
    .map(|(_, pt)| *pt)
    .unwrap_or(PieceType::Pawn);

    let victim_type = [
        (position.white_pawns | position.black_pawns,     PieceType::Pawn),
        (position.white_knights | position.black_knights, PieceType::Knight),
        (position.white_bishops | position.black_bishops, PieceType::Bishop),
        (position.white_rooks | position.black_rooks,     PieceType::Rook),
        (position.white_queens | position.black_queens,   PieceType::Queen),
    ]
    .iter()
    .find(|(bb, _)| bb & to_bit != 0)
    .map(|(_, pt)| *pt)
    .unwrap_or(PieceType::Pawn);

    piece_value(victim_type) * 10 - piece_value(attacker_type)
}

fn order_moves(mut moves: Vec<Move>, position: &Position, tt_best_move: Option<Move>) -> Vec<Move> {
    moves.sort_by_cached_key(|&chess_move| {
        // Lower key = searched first (we negate scores below)
        if tt_best_move == Some(chess_move) {
            return i32::MIN; // always first
        }
        if chess_move.move_flags.contains(crate::board::MoveFlags::CAPTURE) {
            return -mvv_lva_score(position, chess_move); // best captures near front
        }
        0 // quiet moves last, unordered
    });
    moves
}
```

- [ ] **Step 4: Implement `compute_search_limits`**

```rust
fn compute_search_limits(go_parameters: &GoParameters, side_to_move: Color) -> SearchLimits {
    if let Some(depth) = go_parameters.depth {
        return SearchLimits::Depth(depth);
    }
    if let Some(ms) = go_parameters.move_time_ms {
        return SearchLimits::MoveTime(Duration::from_millis(ms));
    }
    if go_parameters.infinite {
        return SearchLimits::Infinite;
    }
    // Clock-based: use remaining time and increment for side to move
    let remaining_ms = match side_to_move {
        Color::White => go_parameters.white_time_remaining_ms,
        Color::Black => go_parameters.black_time_remaining_ms,
    };
    let increment_ms = match side_to_move {
        Color::White => go_parameters.white_increment_ms,
        Color::Black => go_parameters.black_increment_ms,
    };
    let remaining = remaining_ms.unwrap_or(5_000);
    let increment = increment_ms.unwrap_or(0);
    let budget_ms = remaining / 30 + increment / 2;
    SearchLimits::Clock { budget: Duration::from_millis(budget_ms.max(50)) }
}
```

- [ ] **Step 5: Implement `quiescence_search`**

```rust
fn quiescence_search(
    position: &Position,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
    context.nodes_searched += 1;
    if context.nodes_searched % 1024 == 0 {
        if context.stop_flag.load(Ordering::Relaxed) {
            return 0;
        }
        let over_time = match &context.limits {
            SearchLimits::MoveTime(duration) => context.start_time.elapsed() >= *duration,
            SearchLimits::Clock { budget }   => context.start_time.elapsed() >= *budget,
            SearchLimits::Depth(_) | SearchLimits::Infinite => false,
        };
        if over_time {
            context.stop_flag.store(true, Ordering::Relaxed);
            return 0;
        }
    }

    let king_square = position.king_square(position.side_to_move);
    let in_check = is_square_attacked(king_square, position.side_to_move.opponent(), position);

    if !in_check {
        // Stand-pat: assume we can do at least as well as the static eval
        let stand_pat = evaluate(position);
        if stand_pat >= beta {
            return beta;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }
    }

    // Generate moves: captures only when quiet, all evasions when in check
    let pseudo_legal = generate_pseudo_legal_moves(position);
    let mut candidate_moves: Vec<Move> = pseudo_legal
        .into_iter()
        .filter(|chess_move| {
            if in_check {
                true // search all evasions
            } else {
                chess_move.move_flags.contains(crate::board::MoveFlags::CAPTURE)
            }
        })
        .collect();

    // Order captures first (MVV-LVA), then quiet evasions
    candidate_moves.sort_by_cached_key(|&chess_move| {
        if chess_move.move_flags.contains(crate::board::MoveFlags::CAPTURE) {
            -mvv_lva_score(position, chess_move)
        } else {
            0
        }
    });

    let mut legal_move_count = 0;
    for chess_move in candidate_moves {
        let child_position = crate::board::apply_move(position, chess_move);
        // Filter pseudo-legal: skip if king still in check after move
        let moving_king_square = child_position.king_square(position.side_to_move);
        if is_square_attacked(moving_king_square, position.side_to_move.opponent(), &child_position) {
            continue;
        }
        legal_move_count += 1;

        let score = -quiescence_search(&child_position, -beta, -alpha, ply + 1, context);
        if score >= beta {
            return beta;
        }
        if score > alpha {
            alpha = score;
        }
    }

    // Checkmate in quiescence: in check with no legal moves
    if in_check && legal_move_count == 0 {
        return -(MATE_SCORE - ply as i32);
    }

    alpha
}
```

- [ ] **Step 6: Implement `negamax_pvs`**

```rust
fn negamax_pvs(
    position: &Position,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
    context.nodes_searched += 1;
    if context.nodes_searched % 1024 == 0 {
        if context.stop_flag.load(Ordering::Relaxed) {
            return 0;
        }
        let over_time = match &context.limits {
            SearchLimits::MoveTime(duration) => context.start_time.elapsed() >= *duration,
            SearchLimits::Clock { budget }   => context.start_time.elapsed() >= *budget,
            SearchLimits::Depth(_) | SearchLimits::Infinite => false,
        };
        if over_time {
            context.stop_flag.store(true, Ordering::Relaxed);
            return 0;
        }
    }

    // 50-move rule
    if position.halfmove_clock >= 100 {
        return 0;
    }

    // TT lookup
    let position_hash = compute_hash(position);
    let alpha_original = alpha;
    let mut tt_best_move: Option<Move> = None;

    if let Some(tt_entry) = context.transposition_table.probe(position_hash) {
        tt_best_move = Some(tt_entry.best_move);
        if tt_entry.depth >= depth as u8 {
            match tt_entry.node_type {
                NodeType::Exact => return tt_entry.score,
                NodeType::LowerBound => {
                    if tt_entry.score > alpha { alpha = tt_entry.score; }
                }
                NodeType::UpperBound => {
                    let new_beta = beta.min(tt_entry.score);
                    if alpha >= new_beta { return tt_entry.score; }
                }
            }
            if alpha >= beta {
                return tt_entry.score;
            }
        }
    }

    // Generate legal moves (must come before depth==0 to detect checkmate/stalemate at leaves)
    let legal_moves = generate_legal_moves(position);
    if legal_moves.is_empty() {
        let king_square = position.king_square(position.side_to_move);
        let in_check = is_square_attacked(king_square, position.side_to_move.opponent(), position);
        return if in_check {
            -(MATE_SCORE - ply as i32) // checkmate — prefer shorter mates
        } else {
            0 // stalemate
        };
    }

    // Leaf node: quiescence search
    if depth == 0 {
        return quiescence_search(position, alpha, beta, ply, context);
    }

    let ordered_moves = order_moves(legal_moves, position, tt_best_move);
    let mut best_move = ordered_moves[0];
    let mut first_move = true;

    for chess_move in &ordered_moves {
        let child_position = crate::board::apply_move(position, *chess_move);
        let score = if first_move {
            first_move = false;
            -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
        } else {
            // PVS: null-window search first
            let null_score = -negamax_pvs(&child_position, depth - 1, -alpha - 1, -alpha, ply + 1, context);
            if null_score > alpha && null_score < beta && beta - alpha > 1 {
                // Failed high on null window — re-search with full window
                -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
            } else {
                null_score
            }
        };

        // Fail-hard: check beta cutoff BEFORE updating alpha
        if score >= beta {
            context.transposition_table.store(position_hash, TtEntry {
                hash: position_hash,
                depth: depth as u8,
                score: beta,
                best_move: *chess_move,
                node_type: NodeType::LowerBound,
            });
            return beta;
        }
        if score > alpha {
            alpha = score;
            best_move = *chess_move;
        }
    }

    // Store in TT
    let node_type = if alpha > alpha_original { NodeType::Exact } else { NodeType::UpperBound };
    context.transposition_table.store(position_hash, TtEntry {
        hash: position_hash,
        depth: depth as u8,
        score: alpha,
        best_move,
        node_type,
    });

    alpha
}
```

- [ ] **Step 7: Implement `select_move`**

```rust
pub fn select_move(
    position: &Position,
    go_parameters: &GoParameters,
    transposition_table: &mut TranspositionTable,
    stop_flag: &AtomicBool,
) -> Move {
    let limits = compute_search_limits(go_parameters, position.side_to_move);

    // Fallback: first legal move (in case depth-1 search is interrupted)
    let legal_moves = generate_legal_moves(position);
    let mut best_move = *legal_moves.first().expect("select_move called with no legal moves");

    let max_depth = match &limits {
        SearchLimits::Depth(depth) => *depth,
        _ => 100, // effectively unlimited; time limit stops the search
    };

    let mut context = SearchContext {
        transposition_table,
        stop_flag,
        limits,
        start_time: Instant::now(),
        nodes_searched: 0,
    };

    for depth in 1..=max_depth {
        negamax_pvs(position, depth, -INF, INF, 0, &mut context);

        if context.stop_flag.load(Ordering::Relaxed) && depth > 1 {
            break; // Incomplete iteration — keep previous best_move
        }

        // Extract best move from TT root entry
        let position_hash = compute_hash(position);
        if let Some(tt_entry) = context.transposition_table.probe(position_hash) {
            best_move = tt_entry.best_move;
        }
    }

    best_move
}
```

- [ ] **Step 8: Run all engine tests**

```bash
cargo test engine::
```

Expected: all 5 tests pass.

- [ ] **Step 9: Run the full test suite to confirm nothing regressed**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add src/engine.rs
git commit -m "feat: implement PVS search with iterative deepening and quiescence search"
```

---

## Task 4: UCI Threading & GoParameters Integration

**Files:**
- Modify: `src/uci.rs`

The current `uci.rs` calls `select_move(position, &legal_moves)` with a random-mover signature. We need to:
1. Change the `run_uci_loop` function to own `Arc<AtomicBool>` + `Arc<Mutex<TranspositionTable>>`
2. On `go`: reset stop flag, clone the arcs, move position + params into a search thread
3. On `stop`/`quit`: set stop flag, join the search thread
4. The search thread calls `select_move` and writes `bestmove` to stdout

- [ ] **Step 1: Write a failing integration test**

Add to the `#[cfg(test)]` block in `src/uci.rs`:

```rust
#[test]
fn go_with_depth_produces_bestmove() {
    let input = b"position startpos\ngo depth 3\nquit\n";
    let mut output = Vec::new();
    run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
    let response = String::from_utf8(output).unwrap();
    assert!(response.contains("bestmove"), "response was: {}", response);
    // bestmove should be a real move like "e2e4", not "0000"
    let bestmove_line = response.lines().find(|l| l.starts_with("bestmove")).unwrap();
    assert_ne!(bestmove_line, "bestmove 0000", "should not be null move");
}

#[test]
fn go_infinite_then_stop_produces_bestmove() {
    // Sends "go infinite" then "stop" — engine should respond with bestmove
    // We interleave them via a pipe-like byte sequence
    let input = b"position startpos\ngo infinite\nstop\nquit\n";
    let mut output = Vec::new();
    run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
    let response = String::from_utf8(output).unwrap();
    assert!(response.contains("bestmove"), "response was: {}", response);
}

#[test]
fn ucinewgame_clears_tt_without_panic() {
    let input = b"ucinewgame\nposition startpos\ngo depth 2\nquit\n";
    let mut output = Vec::new();
    run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
    let response = String::from_utf8(output).unwrap();
    assert!(response.contains("bestmove"));
}
```

- [ ] **Step 2: Run to verify new tests fail**

```bash
cargo test uci::tests::go_with_depth 2>&1 | head -20
```

Expected: test fails (wrong output or compile error after we change the signature).

- [ ] **Step 3: Rewrite `src/uci.rs`**

Replace the `run_uci_loop` function and its helpers. Keep all the parsing functions (`parse_uci_command`, `parse_position`, `parse_go_parameters`, `move_to_uci_string`, `parse_uci_move_string`) unchanged. Replace only `handle_stdin_line`, `run_uci_loop`, and the `LineOutcome` enum:

```rust
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::instrument;

use crate::board::{apply_move, from_fen, start_position, Position};
use crate::engine::select_move;
use crate::tt::TranspositionTable;

// (keep all existing use statements and parsing functions above)

enum LineOutcome {
    Continue,
    Quit,
}

struct UciState {
    current_position: Position,
    debug_mode: bool,
    stop_flag: Arc<AtomicBool>,
    transposition_table: Arc<Mutex<TranspositionTable>>,
    search_thread: Option<std::thread::JoinHandle<()>>,
}

impl UciState {
    fn new() -> Self {
        UciState {
            current_position: start_position(),
            debug_mode: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
            transposition_table: Arc::new(Mutex::new(TranspositionTable::new(16))),
            search_thread: None,
        }
    }

    fn stop_search(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.search_thread.take() {
            handle.join().ok();
        }
    }
}

#[instrument(skip(output, state))]
fn handle_stdin_line(
    line: &str,
    output: &mut impl Write,
    state: &mut UciState,
) -> LineOutcome {
    let command = parse_uci_command(line);

    match command {
        UciCommand::Uci => {
            writeln!(output, "id name chess-engine").unwrap();
            writeln!(output, "id author chess-engine").unwrap();
            writeln!(output, "uciok").unwrap();
            output.flush().unwrap();
        }

        UciCommand::Debug(enabled) => {
            state.debug_mode = enabled;
        }

        UciCommand::IsReady => {
            writeln!(output, "readyok").unwrap();
            output.flush().unwrap();
        }

        UciCommand::SetOption { .. } => {}

        UciCommand::UciNewGame => {
            state.stop_search();
            state.current_position = start_position();
            state.stop_flag.store(false, Ordering::Relaxed);
            state.transposition_table.lock().unwrap().clear();
        }

        UciCommand::Position { fen, moves } => {
            state.current_position = from_fen(&fen);
            for uci_move_string in &moves {
                let chess_move = parse_uci_move_string(uci_move_string, &state.current_position);
                state.current_position = apply_move(&state.current_position, chess_move);
            }
        }

        UciCommand::Go(parameters) => {
            // Stop any in-progress search before starting a new one
            state.stop_search();
            state.stop_flag.store(false, Ordering::Relaxed);

            let legal_moves = generate_legal_moves(&state.current_position);
            if legal_moves.is_empty() {
                writeln!(output, "bestmove 0000").unwrap();
                output.flush().unwrap();
                return LineOutcome::Continue;
            }

            // Clone shared state for the search thread
            let position = state.current_position.clone();
            let stop_flag = Arc::clone(&state.stop_flag);
            let tt_arc = Arc::clone(&state.transposition_table);

            let handle = std::thread::spawn(move || {
                let mut tt = tt_arc.lock().unwrap();
                let chosen = select_move(&position, &parameters, &mut tt, &stop_flag);
                // Write directly to stdout (the search thread owns output here)
                let uci_move = move_to_uci_string(chosen);
                println!("bestmove {}", uci_move);
            });

            state.search_thread = Some(handle);
        }

        UciCommand::Stop => {
            state.stop_search();
            state.stop_flag.store(false, Ordering::Relaxed);
        }

        UciCommand::PonderHit => {}

        UciCommand::Quit => {
            state.stop_search();
            return LineOutcome::Quit;
        }

        UciCommand::Unknown(text) => {
            if state.debug_mode {
                eprintln!("Unknown UCI command: {}", text);
            }
        }
    }
    LineOutcome::Continue
}

/// Runs the main UCI input/output loop.
pub fn run_uci_loop(input: impl BufRead, output: &mut impl Write) {
    let mut state = UciState::new();

    for line in input.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("Error reading UCI input: {}", error);
                break;
            }
        };

        if matches!(
            handle_stdin_line(&line, output, &mut state),
            LineOutcome::Quit
        ) {
            break;
        }
    }
}
```

Also add the missing import at the top of `uci.rs`:

```rust
use crate::movegen::generate_legal_moves;
```

- [ ] **Step 4: Run all tests**

```bash
cargo test
```

Expected: all tests pass including the three new UCI threading tests and the existing `go_produces_bestmove` test.

- [ ] **Step 5: Smoke-test with a real UCI session**

```bash
echo -e "uci\nisready\nposition startpos\ngo depth 5\nquit" | cargo run --release 2>/dev/null
```

Expected output includes:
```
id name chess-engine
uciok
readyok
bestmove <some move like e2e4>
```

- [ ] **Step 6: Commit**

```bash
git add src/uci.rs
git commit -m "feat: add UCI threading with stop flag and search thread per go command"
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] `src/eval.rs` — material evaluation (Task 1)
- [x] `src/tt.rs` — Zobrist hashing, TranspositionTable, NodeType, TtEntry (Task 2)
- [x] `negamax_pvs` — TT lookup, 50-move rule, legal move generation, terminal detection, depth==0 → quiescence, PVS null-window + re-search, beta-alpha>1 guard, fail-hard cutoff, best_move tracking, TT store (Task 3)
- [x] `quiescence_search` — in-check detection, stand-pat skip when in check, captures-only when quiet, all evasions when in check, checkmate detection (Task 3)
- [x] `select_move` — iterative deepening, stop on incomplete iteration, TT root extraction, fallback move (Task 3)
- [x] `compute_search_limits` — Depth, MoveTime, Infinite, Clock with budget formula (Task 3)
- [x] UCI threading — Arc<AtomicBool>, Arc<Mutex<TT>>, search thread per go, stop/join on stop/quit/ucinewgame (Task 4)
- [x] MVV-LVA — `victim * 10 - attacker` (Task 3, `mvv_lva_score`)
- [x] Move ordering — TT best move first, captures by MVV-LVA, quiets last (Task 3, `order_moves`)
- [x] `Vec::with_capacity` — not explicitly used; the existing `generate_legal_moves` already allocates. Not critical for correctness.
- [x] `mod eval; mod tt;` registered in `main.rs` (Tasks 1 and 2)

**No placeholders found.**

**Type consistency verified:** `select_move` signature used in Task 3 matches what Task 4's `uci.rs` calls. `TtEntry` fields used in Task 3 match the struct defined in Task 2. `SearchLimits` variants used in `negamax_pvs` and `quiescence_search` match the enum defined in Task 3's skeleton.
