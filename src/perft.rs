use crate::board::{apply_move, Move};
use crate::movegen::generate_legal_moves;

/// Counts leaf nodes at exactly `depth` plies from `position`.
/// At depth 0, counts the position itself as one node.
pub fn perft(position: &crate::board::Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    generate_legal_moves(position)
        .into_iter()
        .map(|chess_move| perft(&apply_move(position, chess_move), depth - 1))
        .sum()
}

/// For each legal move from `position`, returns `(move, perft(position_after_move, depth - 1))`.
/// Returns an empty vec when `depth` is 0.
pub fn perft_divide(position: &crate::board::Position, depth: u32) -> Vec<(Move, u64)> {
    if depth == 0 {
        return Vec::new();
    }
    generate_legal_moves(position)
        .into_iter()
        .map(|chess_move| {
            let node_count = perft(&apply_move(position, chess_move), depth - 1);
            (chess_move, node_count)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::from_fen;

    const START_POSITION_FEN: &str =
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    const KIWIPETE_FEN: &str =
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    const POSITION_3_FEN: &str =
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
    const POSITION_5_FEN: &str =
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";

    // --- Starting position ---

    #[test]
    fn startpos_depth_1_is_20() {
        assert_eq!(perft(&from_fen(START_POSITION_FEN), 1), 20);
    }

    #[test]
    fn startpos_depth_2_is_400() {
        assert_eq!(perft(&from_fen(START_POSITION_FEN), 2), 400);
    }

    #[test]
    fn startpos_depth_3_is_8902() {
        assert_eq!(perft(&from_fen(START_POSITION_FEN), 3), 8902);
    }

    #[test]
    fn startpos_depth_4_is_197281() {
        assert_eq!(perft(&from_fen(START_POSITION_FEN), 4), 197_281);
    }

    // --- Kiwipete (Position 2) ---

    #[test]
    fn kiwipete_depth_1_is_48() {
        assert_eq!(perft(&from_fen(KIWIPETE_FEN), 1), 48);
    }

    #[test]
    fn kiwipete_depth_2_is_2039() {
        assert_eq!(perft(&from_fen(KIWIPETE_FEN), 2), 2039);
    }

    #[test]
    fn kiwipete_depth_3_is_97862() {
        assert_eq!(perft(&from_fen(KIWIPETE_FEN), 3), 97_862);
    }

    // --- Position 3 ---

    #[test]
    fn position_3_depth_1_is_14() {
        assert_eq!(perft(&from_fen(POSITION_3_FEN), 1), 14);
    }

    #[test]
    fn position_3_depth_3_is_2812() {
        assert_eq!(perft(&from_fen(POSITION_3_FEN), 3), 2_812);
    }

    #[test]
    fn position_3_depth_4_is_43238() {
        assert_eq!(perft(&from_fen(POSITION_3_FEN), 4), 43_238);
    }

    // --- Position 5 ---

    #[test]
    fn position_5_depth_1_is_44() {
        assert_eq!(perft(&from_fen(POSITION_5_FEN), 1), 44);
    }

    #[test]
    fn position_5_depth_2_is_1486() {
        assert_eq!(perft(&from_fen(POSITION_5_FEN), 2), 1_486);
    }

    #[test]
    fn position_5_depth_3_is_62379() {
        assert_eq!(perft(&from_fen(POSITION_5_FEN), 3), 62_379);
    }

    // --- perft_divide sanity check ---

    #[test]
    fn perft_divide_depth_1_sums_to_20() {
        let position = from_fen(START_POSITION_FEN);
        let divide = perft_divide(&position, 1);
        assert_eq!(divide.len(), 20, "startpos has 20 legal moves");
        let total: u64 = divide.iter().map(|(_, count)| count).sum();
        assert_eq!(total, 20);
    }

    #[test]
    fn perft_divide_depth_0_returns_empty() {
        let position = from_fen(START_POSITION_FEN);
        let divide = perft_divide(&position, 0);
        assert!(divide.is_empty());
    }
}
