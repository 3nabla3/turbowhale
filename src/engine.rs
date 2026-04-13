use rand::seq::IndexedRandom;
use tracing::instrument;

use crate::board::{Move, Position};

/// Selects a random move from the list of legal moves.
#[instrument(skip(_position, legal_moves))]
pub fn select_move(_position: &Position, legal_moves: &[Move]) -> Move {
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
