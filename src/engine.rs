use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::board::{Color, Move, MoveFlags, PieceType, Position};
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
    let limits = compute_search_limits(go_parameters, position.side_to_move);

    let legal_moves = generate_legal_moves(position);
    let mut best_move = *legal_moves.first().expect("select_move called with no legal moves");

    let max_depth = match &limits {
        SearchLimits::Depth(depth) => *depth,
        _ => 100,
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
            break;
        }

        let position_hash = compute_hash(position);
        if let Some(tt_entry) = context.transposition_table.probe(position_hash) {
            best_move = tt_entry.best_move;
        }
    }

    best_move
}

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

fn negamax_pvs(
    position: &Position,
    depth: u32,
    mut alpha: i32,
    mut beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
    context.nodes_searched += 1;
    if context.nodes_searched.is_multiple_of(1024) {
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

    if position.halfmove_clock >= 100 {
        return 0;
    }

    // Generate legal moves before depth==0 and TT checks — required to detect
    // checkmate/stalemate correctly before any transposition table shortcut.
    let legal_moves = generate_legal_moves(position);
    if legal_moves.is_empty() {
        let king_square = position.king_square(position.side_to_move);
        let in_check = is_square_attacked(king_square, position.side_to_move.opponent(), position);
        return if in_check {
            -(MATE_SCORE - ply as i32)
        } else {
            0
        };
    }

    if depth == 0 {
        return quiescence_search(position, alpha, beta, ply, context);
    }

    let position_hash = compute_hash(position);
    let alpha_original = alpha;
    let mut tt_best_move: Option<Move> = None;

    if let Some(tt_entry) = context.transposition_table.probe(position_hash) {
        tt_best_move = Some(tt_entry.best_move);
        if tt_entry.depth >= depth as u8 {
            match tt_entry.node_type {
                NodeType::Exact => return tt_entry.score,
                NodeType::LowerBound => {
                    if tt_entry.score > alpha {
                        alpha = tt_entry.score;
                    }
                }
                NodeType::UpperBound => {
                    if tt_entry.score < beta {
                        beta = tt_entry.score;
                    }
                }
            }
            if alpha >= beta {
                return tt_entry.score;
            }
        }
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
            let null_score = -negamax_pvs(&child_position, depth - 1, -alpha - 1, -alpha, ply + 1, context);
            if null_score > alpha && null_score < beta && beta - alpha > 1 {
                -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
            } else {
                null_score
            }
        };

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

fn quiescence_search(
    position: &Position,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
    context.nodes_searched += 1;
    if context.nodes_searched.is_multiple_of(1024) {
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
        let stand_pat = evaluate(position);
        if stand_pat >= beta {
            return beta;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }
    }

    let pseudo_legal = generate_pseudo_legal_moves(position);
    let mut candidate_moves: Vec<Move> = pseudo_legal
        .into_iter()
        .filter(|&chess_move| {
            if in_check { true } else { is_capture(chess_move, position) }
        })
        .collect();

    candidate_moves.sort_by_cached_key(|&chess_move| {
        if is_capture(chess_move, position) {
            -mvv_lva_score(position, chess_move)
        } else {
            0
        }
    });

    let mut legal_move_count = 0;
    for chess_move in candidate_moves {
        let child_position = crate::board::apply_move(position, chess_move);
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

    if in_check && legal_move_count == 0 {
        return -(MATE_SCORE - ply as i32);
    }

    alpha
}

/// Returns true if the move lands on a square occupied by an enemy piece.
/// This is necessary because the movegen only sets MoveFlags::CAPTURE for pawn captures
/// and en passant — not for sliding piece or knight captures.
fn is_capture(chess_move: Move, position: &Position) -> bool {
    let destination_bit = 1u64 << chess_move.to_square;
    let enemy_occupancy = match position.side_to_move {
        Color::White => {
            position.black_pawns | position.black_knights | position.black_bishops
                | position.black_rooks | position.black_queens | position.black_king
        }
        Color::Black => {
            position.white_pawns | position.white_knights | position.white_bishops
                | position.white_rooks | position.white_queens | position.white_king
        }
    };
    destination_bit & enemy_occupancy != 0
        || chess_move.move_flags.contains(MoveFlags::CAPTURE)
}

fn piece_value(piece_type: PieceType) -> i32 {
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
        if tt_best_move == Some(chess_move) {
            return i32::MIN;
        }
        if is_capture(chess_move, position) {
            return -mvv_lva_score(position, chess_move);
        }
        0
    });
    moves
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{from_fen, start_position};
    use crate::uci::GoParameters;

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
        // White king on g6, white queen on h6, black king on g8 — Qh8# is checkmate.
        // g6=46, h6=47, g8=62. After Qh8 (47->63): black king g8 is in check, escapes
        // blocked: f8 by queen on h8 (rank), f7 by king on g6, g7 by king on g6, h7 by queen (file).
        let position = from_fen("6k1/8/6KQ/8/8/8/8/8 w - - 0 1");
        let mut tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, &mut tt, &stop);
        let after = crate::board::apply_move(&position, chosen);
        let opponent_moves = generate_legal_moves(&after);
        assert!(
            opponent_moves.is_empty(),
            "engine should deliver checkmate, got move from={} to={}",
            chosen.from_square, chosen.to_square
        );
    }

    #[test]
    fn select_move_captures_hanging_queen() {
        // White rook on a5 can take free black queen on e5 (same rank, undefended)
        let position = from_fen("4k3/8/8/R3q3/8/8/8/4K3 w - - 0 1");
        let mut tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, &mut tt, &stop);
        assert_eq!(chosen.to_square, 36, "should capture queen on e5 (square 36)");
    }

    #[test]
    fn negamax_returns_zero_for_stalemate() {
        // Black king stalemated: black king a8, white king b6, white queen c7
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
        // Fool's mate — white is in checkmate
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
        assert!(score < -MATE_SCORE / 2, "checkmate should return large negative, got {}", score);
    }
}
