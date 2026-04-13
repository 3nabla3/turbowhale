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
    pawns.count_ones() as i32 * PAWN_VALUE
        + knights.count_ones() as i32 * KNIGHT_VALUE
        + bishops.count_ones() as i32 * BISHOP_VALUE
        + rooks.count_ones() as i32 * ROOK_VALUE
        + queens.count_ones() as i32 * QUEEN_VALUE
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
