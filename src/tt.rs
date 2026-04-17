use std::sync::Mutex;
use std::sync::OnceLock;

use crate::board::{Color, Move, Position};

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

const SHARD_COUNT: usize = 256;

pub struct ShardedTranspositionTable {
    shards: Vec<Mutex<Vec<Option<TtEntry>>>>,
    pub entries_per_shard: usize,
}

impl ShardedTranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        let entry_bytes = std::mem::size_of::<Option<TtEntry>>();
        let target_entries = (size_mb * 1024 * 1024) / entry_bytes;
        let total_entries = (target_entries.next_power_of_two() / 2).max(SHARD_COUNT);
        let entries_per_shard = (total_entries / SHARD_COUNT).max(1);
        let shards = (0..SHARD_COUNT)
            .map(|_| Mutex::new(vec![None; entries_per_shard]))
            .collect();
        ShardedTranspositionTable { shards, entries_per_shard }
    }

    pub fn clear(&self) {
        for shard in &self.shards {
            shard.lock().unwrap().iter_mut().for_each(|entry| *entry = None);
        }
    }

    pub fn probe(&self, hash: u64) -> Option<TtEntry> {
        let shard_index = (hash as usize) & (SHARD_COUNT - 1);
        let entry_index = ((hash >> 8) as usize) & (self.entries_per_shard - 1);
        let shard = self.shards[shard_index].lock().unwrap();
        shard[entry_index].filter(|entry| entry.hash == hash)
    }

    pub fn store(&self, hash: u64, entry: TtEntry) {
        let shard_index = (hash as usize) & (SHARD_COUNT - 1);
        let entry_index = ((hash >> 8) as usize) & (self.entries_per_shard - 1);
        let mut shard = self.shards[shard_index].lock().unwrap();
        shard[entry_index] = Some(entry);
    }
}

// --- Zobrist hashing ---

struct ZobristKeys {
    pieces: [[[u64; 64]; 6]; 2], // [color][piece_type_index][square]
    black_to_move: u64,
    castling: [u64; 16],
    en_passant_file: [u64; 8],
}

static ZOBRIST_KEYS: OnceLock<ZobristKeys> = OnceLock::new();

fn zobrist_keys() -> &'static ZobristKeys {
    ZOBRIST_KEYS.get_or_init(|| {
        // Deterministic xorshift PRNG — reproducible across runs
        let mut state: u64 = 0x123456789ABCDEF0;
        let mut next = move || -> u64 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        let mut pieces = [[[0u64; 64]; 6]; 2];
        for color_pieces in &mut pieces {
            for piece_squares in color_pieces {
                for square_key in piece_squares {
                    *square_key = next();
                }
            }
        }
        let black_to_move = next();
        let castling = std::array::from_fn(|_| next());
        let en_passant_file = std::array::from_fn(|_| next());

        ZobristKeys { pieces, black_to_move, castling, en_passant_file }
    })
}

/// Computes a Zobrist hash for the given position from scratch.
/// Piece type indices: Pawn=0, Knight=1, Bishop=2, Rook=3, Queen=4, King=5
/// Color indices: White=0, Black=1
pub fn compute_hash(position: &Position) -> u64 {
    let keys = zobrist_keys();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{from_fen, start_position, MoveFlags};

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
    fn sharded_probe_returns_none_on_empty_table() {
        let table = ShardedTranspositionTable::new(1);
        assert!(table.probe(12345).is_none());
    }

    #[test]
    fn sharded_store_then_probe_returns_entry() {
        let table = ShardedTranspositionTable::new(1);
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
    fn sharded_probe_returns_none_on_hash_collision() {
        let table = ShardedTranspositionTable::new(1);
        let dummy_move = Move {
            from_square: 12,
            to_square: 20,
            promotion_piece: None,
            move_flags: MoveFlags::NONE,
        };
        let hash = 0xAAAAu64;
        table.store(hash, TtEntry {
            hash,
            depth: 4,
            score: 150,
            best_move: dummy_move,
            node_type: NodeType::Exact,
        });
        // A hash that maps to the same shard (same low 8 bits) and same slot
        // (same bits 8..N) but is a distinct value — store under `hash` must not
        // be returned when probing `colliding_hash`.
        let colliding_hash = hash.wrapping_add((table.entries_per_shard as u64) << 8);
        assert_ne!(colliding_hash, hash);
        assert!(table.probe(colliding_hash).is_none());
    }

    #[test]
    fn sharded_clear_removes_all_entries() {
        let table = ShardedTranspositionTable::new(1);
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
