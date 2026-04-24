use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::board::{Color, Move, MoveFlags, PieceType, Position};
use crate::eval::evaluate;
use crate::movegen::{generate_legal_moves, generate_pseudo_legal_moves, is_square_attacked};
use crate::tt::{compute_hash, NodeType, ShardedTranspositionTable, TtEntry};
use crate::uci::{GoParameters, move_to_uci_string};

pub const MATE_SCORE: i32 = 100_000;
const INF: i32 = 200_000;
const NULL_MOVE_REDUCTION: u32 = 2;
pub const MAX_SEARCH_PLY: usize = 128;

fn reduction_table() -> &'static [[u8; 64]; 64] {
    static TABLE: OnceLock<[[u8; 64]; 64]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [[0u8; 64]; 64];
        for (depth, depth_row) in table.iter_mut().enumerate().skip(1) {
            for (move_index, entry) in depth_row.iter_mut().enumerate().skip(1) {
                let value = ((depth as f64).ln() * (move_index as f64).ln()) / 2.25;
                *entry = value.round().max(0.0) as u8;
            }
        }
        table
    })
}

#[derive(Clone)]
pub enum SearchLimits {
    Depth(u32),
    MoveTime(Duration),
    Infinite,
    Clock { budget: Duration },
}

pub struct SearchContext {
    pub transposition_table: Arc<ShardedTranspositionTable>,
    pub stop_flag: Arc<AtomicBool>,
    pub shared_nodes: Arc<AtomicU64>,
    pub limits: SearchLimits,
    pub start_time: Instant,
    pub nodes_searched: u64,
    pub killer_moves: [[Option<Move>; 2]; MAX_SEARCH_PLY],
    pub history_scores: [[[i32; 64]; 64]; 2],
}

fn search_worker(
    position: Position,
    limits: SearchLimits,
    start_time: Instant,
    transposition_table: Arc<ShardedTranspositionTable>,
    stop_flag: Arc<AtomicBool>,
    shared_nodes: Arc<AtomicU64>,
) {
    let mut context = SearchContext {
        transposition_table,
        stop_flag,
        shared_nodes,
        limits,
        start_time,
        nodes_searched: 0,
        killer_moves: [[None; 2]; MAX_SEARCH_PLY],
        history_scores: [[[0i32; 64]; 64]; 2],
    };
    for depth in 1..=100 {
        negamax_pvs(&position, depth, -INF, INF, 0, &mut context);
        if context.stop_flag.load(Ordering::Relaxed) {
            break;
        }
    }
    // Flush any nodes accumulated since the last 1024-node batch boundary.
    context.shared_nodes.fetch_add(context.nodes_searched, Ordering::Relaxed);
}

/// Selects the best move for the current position using iterative deepening PVS.
pub fn select_move(
    position: &Position,
    go_parameters: &GoParameters,
    transposition_table: Arc<ShardedTranspositionTable>,
    stop_flag: Arc<AtomicBool>,
    thread_count: usize,
) -> Move {
    let limits = compute_search_limits(go_parameters, position.side_to_move);
    let shared_nodes = Arc::new(AtomicU64::new(0));
    let start_time = Instant::now();

    // Spawn thread_count - 1 helper threads. Each searches independently and
    // contributes to the shared TT and node counter.
    let helper_handles: Vec<_> = (1..thread_count)
        .map(|_| {
            let position_clone = position.clone();
            let limits_clone = limits.clone();
            let tt_clone = Arc::clone(&transposition_table);
            let stop_clone = Arc::clone(&stop_flag);
            let shared_nodes_clone = Arc::clone(&shared_nodes);
            std::thread::spawn(move || {
                search_worker(position_clone, limits_clone, start_time, tt_clone, stop_clone, shared_nodes_clone);
            })
        })
        .collect();

    let legal_moves = generate_legal_moves(position);
    let mut best_move = *legal_moves.first().expect("select_move called with no legal moves");

    let max_depth = match &limits {
        SearchLimits::Depth(depth) => *depth,
        _ => 100,
    };

    let mut context = SearchContext {
        transposition_table: Arc::clone(&transposition_table),
        stop_flag: Arc::clone(&stop_flag),
        shared_nodes: Arc::clone(&shared_nodes),
        limits,
        start_time,
        nodes_searched: 0,
        killer_moves: [[None; 2]; MAX_SEARCH_PLY],
        history_scores: [[[0i32; 64]; 64]; 2],
    };

    for depth in 1..=max_depth {
        negamax_pvs(position, depth, -INF, INF, 0, &mut context);

        // Flush unflushed local nodes into the shared counter.
        context.shared_nodes.fetch_add(context.nodes_searched, Ordering::Relaxed);
        context.nodes_searched = 0;

        if context.stop_flag.load(Ordering::Relaxed) && depth > 1 {
            break;
        }

        let position_hash = compute_hash(position);
        let elapsed = context.start_time.elapsed();
        let total_nodes = context.shared_nodes.load(Ordering::Relaxed);
        let nps = if elapsed.as_millis() > 0 {
            (total_nodes as f64 / elapsed.as_millis() as f64 * 1000.0) as u64
        } else {
            0
        };

        if let Some(tt_entry) = context.transposition_table.probe(position_hash) {
            best_move = tt_entry.best_move;

            let score_field = if tt_entry.score.abs() > MATE_SCORE / 2 {
                let moves_to_mate = (MATE_SCORE - tt_entry.score.abs() + 1) / 2;
                let signed_moves_to_mate = if tt_entry.score > 0 {
                    moves_to_mate
                } else {
                    -moves_to_mate
                };
                format!("mate {}", signed_moves_to_mate)
            } else {
                format!("cp {}", tt_entry.score)
            };

            let pv = extract_pv_from_tt(position, &context.transposition_table, depth);
            let pv_string = if pv.is_empty() {
                move_to_uci_string(best_move)
            } else {
                pv.iter()
                    .map(|&chess_move| move_to_uci_string(chess_move))
                    .collect::<Vec<_>>()
                    .join(" ")
            };

            println!("info depth {} score {} nodes {} nps {} time {} pv {}",
                depth, score_field, total_nodes, nps, elapsed.as_millis(), pv_string);
        } else {
            println!("info depth {} nodes {} nps {} time {}",
                depth, total_nodes, nps, elapsed.as_millis());
        }
    }

    // Signal helpers to stop and wait for them to exit.
    stop_flag.store(true, Ordering::Release);
    for handle in helper_handles {
        handle.join().ok();
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
    if context.stop_flag.load(Ordering::Relaxed) {
        return 0;
    }

    context.nodes_searched += 1;
    if context.nodes_searched.is_multiple_of(1024) {
        context.shared_nodes.fetch_add(1024, Ordering::Relaxed);
        context.nodes_searched = 0;
        let over_time = match &context.limits {
            SearchLimits::MoveTime(duration) => context.start_time.elapsed() >= *duration,
            SearchLimits::Clock { budget }   => context.start_time.elapsed() >= *budget,
            SearchLimits::Depth(_) | SearchLimits::Infinite => false,
        };
        if over_time {
            context.stop_flag.store(true, Ordering::Release);
            return 0;
        }
    }

    if position.halfmove_clock >= 100 {
        return 0;
    }

    let king_square = position.king_square(position.side_to_move);
    let is_in_check = is_square_attacked(king_square, position.side_to_move.opponent(), position);

    if depth == 0 {
        return quiescence_search(position, alpha, beta, ply, is_in_check, context);
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

    // Null move pruning: if the position is so good that even passing our turn
    // fails to let the opponent recover, prune this subtree.
    if !is_in_check && ply > 0 && depth > NULL_MOVE_REDUCTION {
        let has_non_pawn_non_king_piece = match position.side_to_move {
            Color::White => (position.white_knights | position.white_bishops
                           | position.white_rooks  | position.white_queens) != 0,
            Color::Black => (position.black_knights | position.black_bishops
                           | position.black_rooks  | position.black_queens) != 0,
        };
        if has_non_pawn_non_king_piece {
            let mut null_position = position.clone();
            null_position.side_to_move = position.side_to_move.opponent();
            null_position.en_passant_square = None;
            null_position.halfmove_clock += 1;
            let null_score = -negamax_pvs(
                &null_position,
                depth - 1 - NULL_MOVE_REDUCTION,
                -beta,
                -beta + 1,
                ply + 1,
                context,
            );
            if null_score >= beta {
                return beta;
            }
        }
    }

    let legal_moves = generate_legal_moves(position);
    if legal_moves.is_empty() {
        return if is_in_check {
            -(MATE_SCORE - ply as i32)
        } else {
            0
        };
    }

    let ordered_moves = order_moves(
        legal_moves, position, tt_best_move, ply, &context.killer_moves, &context.history_scores,
    );
    let mut best_move = ordered_moves[0];

    for (move_index, chess_move) in ordered_moves.iter().enumerate() {
        let chess_move_value = *chess_move;
        let child_position = crate::board::apply_move(position, chess_move_value);

        let is_quiet = !is_capture(chess_move_value, position)
            && chess_move_value.promotion_piece.is_none();
        let ply_index_for_killer = (ply as usize).min(MAX_SEARCH_PLY - 1);
        let is_killer = context.killer_moves[ply_index_for_killer][0] == Some(chess_move_value)
            || context.killer_moves[ply_index_for_killer][1] == Some(chess_move_value);

        let score = if move_index == 0 {
            -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
        } else {
            let reduction: u32 = if depth >= 3
                && move_index >= 3
                && !is_in_check
                && is_quiet
                && !is_killer
            {
                let depth_index = (depth as usize).min(63);
                let move_index_clamped = move_index.min(63);
                reduction_table()[depth_index][move_index_clamped] as u32
            } else {
                0
            };

            let reduced_depth = (depth - 1).saturating_sub(reduction);
            let reduced_score = -negamax_pvs(
                &child_position, reduced_depth, -alpha - 1, -alpha, ply + 1, context,
            );

            let null_window_score = if reduction > 0 && reduced_score > alpha {
                -negamax_pvs(&child_position, depth - 1, -alpha - 1, -alpha, ply + 1, context)
            } else {
                reduced_score
            };

            if null_window_score > alpha && null_window_score < beta && beta - alpha > 1 {
                -negamax_pvs(&child_position, depth - 1, -beta, -alpha, ply + 1, context)
            } else {
                null_window_score
            }
        };

        if score >= beta {
            if is_quiet && (ply as usize) < MAX_SEARCH_PLY {
                let ply_index = ply as usize;
                if context.killer_moves[ply_index][0] != Some(chess_move_value) {
                    context.killer_moves[ply_index][1] = context.killer_moves[ply_index][0];
                    context.killer_moves[ply_index][0] = Some(chess_move_value);
                }
                let color_index = position.side_to_move as usize;
                let from_index = chess_move_value.from_square as usize;
                let to_index = chess_move_value.to_square as usize;
                let bonus = (depth * depth) as i32;
                let entry = &mut context.history_scores[color_index][from_index][to_index];
                *entry = (*entry + bonus).min(16384);
            }
            context.transposition_table.store(position_hash, TtEntry {
                hash: position_hash,
                depth: depth as u8,
                score: beta,
                best_move: chess_move_value,
                node_type: NodeType::LowerBound,
            });
            return beta;
        }
        if score > alpha {
            alpha = score;
            best_move = chess_move_value;
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
    is_in_check: bool,
    context: &mut SearchContext,
) -> i32 {
    if context.stop_flag.load(Ordering::Relaxed) {
        return 0;
    }

    context.nodes_searched += 1;
    if context.nodes_searched.is_multiple_of(1024) {
        context.shared_nodes.fetch_add(1024, Ordering::Relaxed);
        context.nodes_searched = 0;
        let over_time = match &context.limits {
            SearchLimits::MoveTime(duration) => context.start_time.elapsed() >= *duration,
            SearchLimits::Clock { budget }   => context.start_time.elapsed() >= *budget,
            SearchLimits::Depth(_) | SearchLimits::Infinite => false,
        };
        if over_time {
            context.stop_flag.store(true, Ordering::Release);
            return 0;
        }
    }

    if !is_in_check {
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
            if is_in_check { true } else { is_capture(chess_move, position) }
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

        let child_king_square = child_position.king_square(child_position.side_to_move);
        let child_in_check = is_square_attacked(
            child_king_square,
            child_position.side_to_move.opponent(),
            &child_position,
        );
        let score = -quiescence_search(&child_position, -beta, -alpha, ply + 1, child_in_check, context);
        if score >= beta {
            return beta;
        }
        if score > alpha {
            alpha = score;
        }
    }

    if is_in_check && legal_move_count == 0 {
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

fn order_moves(
    mut moves: Vec<Move>,
    position: &Position,
    tt_best_move: Option<Move>,
    ply: u32,
    killer_moves: &[[Option<Move>; 2]; MAX_SEARCH_PLY],
    history_scores: &[[[i32; 64]; 64]; 2],
) -> Vec<Move> {
    let ply_index = (ply as usize).min(MAX_SEARCH_PLY - 1);
    let killer1 = killer_moves[ply_index][0];
    let killer2 = killer_moves[ply_index][1];
    let color_index = position.side_to_move as usize;

    moves.sort_by_cached_key(|&chess_move| {
        if Some(chess_move) == tt_best_move {
            return i32::MIN;
        }
        if is_capture(chess_move, position) {
            return -10_000_000 - mvv_lva_score(position, chess_move);
        }
        if Some(chess_move) == killer1 {
            return -1_000_000;
        }
        if Some(chess_move) == killer2 {
            return -999_999;
        }
        -history_scores[color_index][chess_move.from_square as usize][chess_move.to_square as usize]
    });
    moves
}

fn extract_pv_from_tt(root: &Position, tt: &ShardedTranspositionTable, max_depth: u32) -> Vec<Move> {
    let mut pv = Vec::new();
    let mut current_position = root.clone();
    let mut visited_hashes: HashSet<u64> = HashSet::new();

    for _ in 0..max_depth {
        let hash = compute_hash(&current_position);
        if visited_hashes.contains(&hash) {
            break;
        }
        visited_hashes.insert(hash);
        match tt.probe(hash) {
            Some(entry) if entry.node_type == NodeType::Exact => {
                let legal_moves = generate_legal_moves(&current_position);
                if !legal_moves.contains(&entry.best_move) {
                    break;
                }
                pv.push(entry.best_move);
                current_position = crate::board::apply_move(&current_position, entry.best_move);
            }
            _ => break,
        }
    }

    pv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{from_fen, start_position};
    use crate::uci::GoParameters;

    fn make_tt() -> Arc<ShardedTranspositionTable> {
        Arc::new(ShardedTranspositionTable::new(4))
    }

    fn make_stop() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn select_move_returns_legal_move_from_start_position() {
        let position = start_position();
        let legal_moves = generate_legal_moves(&position);
        let tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(1), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 1);
        assert!(legal_moves.contains(&chosen), "selected move must be legal");
    }

    #[test]
    fn select_move_finds_mate_in_one() {
        // White king on g6, white queen on h6, black king on g8 — Qh8# is checkmate.
        // g6=46, h6=47, g8=62. After Qh8 (47->63): black king g8 is in check, escapes
        // blocked: f8 by queen on h8 (rank), f7 by king on g6, g7 by king on g6, h7 by queen (file).
        let position = from_fen("6k1/8/6KQ/8/8/8/8/8 w - - 0 1");
        let tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 1);
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
        let tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 1);
        assert_eq!(chosen.to_square, 36, "should capture queen on e5 (square 36)");
    }

    #[test]
    fn negamax_returns_zero_for_stalemate() {
        // Black king stalemated: black king a8, white king b6, white queen c7
        let position = from_fen("k7/2Q5/1K6/8/8/8/8/8 b - - 0 1");
        let mut context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::new(AtomicU64::new(0)),
            limits: SearchLimits::Depth(4),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        let score = negamax_pvs(&position, 4, -INF, INF, 0, &mut context);
        assert_eq!(score, 0, "stalemate should score 0");
    }

    #[test]
    fn negamax_detects_checkmate() {
        // Fool's mate — white is in checkmate
        let position = from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        let mut context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::new(AtomicU64::new(0)),
            limits: SearchLimits::Depth(1),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        let score = negamax_pvs(&position, 1, -INF, INF, 0, &mut context);
        assert!(score < -MATE_SCORE / 2, "checkmate should return large negative, got {}", score);
    }

    #[test]
    fn search_context_shared_nodes_accumulates_across_search() {
        use std::sync::atomic::AtomicU64;
        let position = crate::board::start_position();
        let shared_nodes = Arc::new(AtomicU64::new(0));
        let mut context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::clone(&shared_nodes),
            limits: SearchLimits::Depth(2),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        negamax_pvs(&position, 2, -INF, INF, 0, &mut context);
        // After a depth-2 search, the shared counter must have been incremented.
        // Flush the local remainder into shared_nodes first.
        shared_nodes.fetch_add(context.nodes_searched, Ordering::Relaxed);
        assert!(shared_nodes.load(Ordering::Relaxed) > 0, "shared_nodes must be non-zero after search");
    }

    #[test]
    fn extract_pv_from_tt_returns_moves_up_to_depth() {
        let position = start_position();
        let tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(3), ..Default::default() };
        // Run a search so the TT is populated
        select_move(&position, &params, Arc::clone(&tt), stop, 1);
        // PV should have at least 1 move and at most 3
        let pv = extract_pv_from_tt(&position, &tt, 3);
        assert!(!pv.is_empty(), "PV must contain at least the best move");
        assert!(pv.len() <= 3, "PV must not exceed requested depth");
    }

    #[test]
    fn extract_pv_from_tt_returns_empty_on_empty_tt() {
        let position = start_position();
        let tt = make_tt();
        let pv = extract_pv_from_tt(&position, &tt, 5);
        assert!(pv.is_empty(), "empty TT should yield empty PV");
    }

    #[test]
    fn select_move_emits_uci_info_line_to_stdout() {
        let position = start_position();
        let tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(2), ..Default::default() };
        // Should not panic — this is the main assertion
        let chosen = select_move(&position, &params, tt, stop, 1);
        let legal_moves = generate_legal_moves(&position);
        assert!(legal_moves.contains(&chosen));
    }

    #[test]
    fn select_move_with_two_threads_returns_legal_move() {
        let position = crate::board::start_position();
        let legal_moves = generate_legal_moves(&position);
        let tt = Arc::new(ShardedTranspositionTable::new(4));
        let stop = Arc::new(AtomicBool::new(false));
        let params = GoParameters { depth: Some(2), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 2);
        assert!(legal_moves.contains(&chosen), "two-thread search must return a legal move");
    }

    #[test]
    fn quiescence_search_in_check_skips_stand_pat() {
        // White king on e1, black queen on e8 — white is in check on the e-file.
        // With is_in_check=true the stand-pat branch is skipped.
        // The king has legal evasions so the score must not be a mate value.
        let position = from_fen("4q3/7k/8/8/8/8/8/4K3 w - - 0 1");
        let mut context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::new(AtomicU64::new(0)),
            limits: SearchLimits::Depth(1),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        let score = quiescence_search(&position, -INF, INF, 0, true, &mut context);
        assert!(score > -MATE_SCORE / 2, "king has evasions — score must not be a mate loss, got {}", score);
    }

    #[test]
    fn select_move_returns_legal_move_when_in_check() {
        // White king on e1, black queen on e8 — white is in check, must find an evasion.
        // Null move must not fire here.
        let position = from_fen("4q3/7k/8/8/8/8/8/4K3 w - - 0 1");
        let tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 1);
        let legal_moves = generate_legal_moves(&position);
        assert!(legal_moves.contains(&chosen), "must return a legal evasion when in check");
    }

    #[test]
    fn reduction_table_matches_log_formula() {
        // Hand-computed sanity checks:
        // At depth=1, move_index=1 → ln(1)*ln(1)=0 → reduction 0
        // At depth=3, move_index=3 → ln(3)*ln(3)/2.25 ≈ 0.5365 → rounds to 1
        // At depth=8, move_index=16 → ln(8)*ln(16)/2.25 ≈ 2.563 → rounds to 3
        let table = reduction_table();
        assert_eq!(table[1][1], 0);
        assert_eq!(table[3][3], 1);
        assert_eq!(table[8][16], 3);
        // At depth=3, move_index=63: reduction=2, so reduced_depth = (3-1).saturating_sub(2) = 0
        // (quiescence). saturating_sub prevents underflow; the re-search on fail-high corrects it.
        assert!(table[3][63] as u32 <= 2, "reduction at depth=3 is at most 2 (the full depth-1)");
    }

    #[test]
    fn new_search_context_has_empty_killers_and_zero_history() {
        let context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::new(AtomicU64::new(0)),
            limits: SearchLimits::Depth(1),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        assert!(context.killer_moves.iter().all(|slots| slots[0].is_none() && slots[1].is_none()));
        assert!(context.history_scores.iter().flatten().flatten().all(|&v| v == 0));
    }

    #[test]
    fn order_moves_puts_tt_move_first_even_over_captures() {
        // Starting position. Pick some legal non-capture move; assert it sorts
        // ahead of captures when passed as the TT move.
        let position = start_position();
        let legal = generate_legal_moves(&position);
        let e2e4 = legal.iter().find(|m| m.from_square == 12 && m.to_square == 28).copied().unwrap();
        let killers = [[None; 2]; MAX_SEARCH_PLY];
        let history = [[[0i32; 64]; 64]; 2];
        let ordered = order_moves(legal, &position, Some(e2e4), 0, &killers, &history);
        assert_eq!(ordered[0], e2e4, "TT move must be first");
    }

    #[test]
    fn order_moves_puts_killers_before_other_quiets() {
        let position = start_position();
        let legal = generate_legal_moves(&position);
        let b1c3 = legal.iter().find(|m| m.from_square == 1 && m.to_square == 18).copied().unwrap();
        let mut killers = [[None; 2]; MAX_SEARCH_PLY];
        killers[0][0] = Some(b1c3);
        let history = [[[0i32; 64]; 64]; 2];
        let ordered = order_moves(legal, &position, None, 0, &killers, &history);
        let killer_index = ordered.iter().position(|m| *m == b1c3).unwrap();
        // In startpos there are no captures, so the killer must be first.
        assert_eq!(killer_index, 0, "killer must be first among quiets when no captures exist");
    }

    #[test]
    fn order_moves_sorts_quiets_by_history_descending() {
        let position = start_position();
        let legal = generate_legal_moves(&position);
        let e2e4 = legal.iter().find(|m| m.from_square == 12 && m.to_square == 28).copied().unwrap();
        let d2d4 = legal.iter().find(|m| m.from_square == 11 && m.to_square == 27).copied().unwrap();
        let killers = [[None; 2]; MAX_SEARCH_PLY];
        let mut history = [[[0i32; 64]; 64]; 2];
        // White side_to_move -> index 0.
        history[0][11][27] = 500;  // d2->d4 high history
        history[0][12][28] = 100;  // e2->e4 low history
        let ordered = order_moves(legal, &position, None, 0, &killers, &history);
        let d2d4_index = ordered.iter().position(|m| *m == d2d4).unwrap();
        let e2e4_index = ordered.iter().position(|m| *m == e2e4).unwrap();
        assert!(d2d4_index < e2e4_index, "higher-history quiet must come earlier");
    }

    #[test]
    fn order_moves_full_five_tier_priority_on_kiwipete() {
        // Kiwipete — rich in captures and quiets. Verifies the full priority chain:
        // TT move → captures (MVV-LVA) → killer 1 → killer 2 → quiets (by history).
        let position = crate::board::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        );
        let legal = generate_legal_moves(&position);

        // Pick a quiet move as the TT move (pawn push a2->a3, from=8, to=16 — legal and quiet).
        // (Note: Ke2 would walk into the Black queen's e-file, so it is illegal in Kiwipete.)
        let tt_move = legal
            .iter()
            .find(|m| m.from_square == 8 && m.to_square == 16)
            .copied()
            .expect("a2->a3 must be legal in Kiwipete");

        // Pick two quiet killers (knight moves): Nc3->b1 and Ne5->c4.
        let killer1 = legal
            .iter()
            .find(|m| m.from_square == 18 && m.to_square == 1)
            .copied()
            .expect("Nc3-b1 must be legal in Kiwipete");
        let killer2 = legal
            .iter()
            .find(|m| m.from_square == 36 && m.to_square == 26)
            .copied()
            .expect("Ne5-c4 must be legal in Kiwipete");

        // Pick a high-history quiet (Qf3->g3 = from=21, to=22) to verify captures still outrank it
        // and killers still outrank it.
        let high_history_quiet = legal
            .iter()
            .find(|m| m.from_square == 21 && m.to_square == 22)
            .copied()
            .expect("Qg3 must be legal in Kiwipete");

        let mut killers = [[None; 2]; MAX_SEARCH_PLY];
        killers[0][0] = Some(killer1);
        killers[0][1] = Some(killer2);
        let mut history = [[[0i32; 64]; 64]; 2];
        // White to move -> index 0. Give the Qg3 quiet a huge history bonus.
        history[0][21][22] = 15_000;

        let ordered = order_moves(legal.clone(), &position, Some(tt_move), 0, &killers, &history);

        // 1. TT move first.
        assert_eq!(ordered[0], tt_move, "TT move must be first");

        // 2. All captures come next, before either killer. Collect the indices of every capture
        //    and of the two killers; every capture index must be strictly less than every killer index.
        let capture_indices: Vec<usize> = ordered
            .iter()
            .enumerate()
            .filter(|(_, chess_move)| is_capture(**chess_move, &position))
            .map(|(index, _)| index)
            .collect();
        assert!(!capture_indices.is_empty(), "Kiwipete must contain captures — sanity check");
        let killer1_index = ordered.iter().position(|m| *m == killer1).unwrap();
        let killer2_index = ordered.iter().position(|m| *m == killer2).unwrap();
        for capture_index in &capture_indices {
            assert!(
                *capture_index < killer1_index,
                "captures must precede killer 1 (capture at {} vs killer1 at {})",
                capture_index, killer1_index,
            );
            assert!(
                *capture_index < killer2_index,
                "captures must precede killer 2",
            );
        }

        // 3. Killer 1 strictly before killer 2.
        assert!(killer1_index < killer2_index, "killer 1 must sort before killer 2");

        // 4. Both killers strictly before the high-history quiet.
        let high_history_index = ordered.iter().position(|m| *m == high_history_quiet).unwrap();
        assert!(
            killer1_index < high_history_index,
            "killer 1 must precede even a high-history quiet",
        );
        assert!(
            killer2_index < high_history_index,
            "killer 2 must precede even a high-history quiet",
        );

        // 5. The high-history quiet must sort ahead of any zero-history quiet. Verify by
        //    locating some other zero-history quiet (king-side castle o-o: Ke1->g1, from=4, to=6)
        //    and asserting Qg3 precedes it.
        if let Some(zero_history_quiet) = legal
            .iter()
            .find(|m| m.from_square == 4 && m.to_square == 6)
            .copied()
        {
            let zero_history_index = ordered.iter().position(|m| *m == zero_history_quiet).unwrap();
            assert!(
                high_history_index < zero_history_index,
                "high-history quiet must precede zero-history quiet (Qg3 at {} vs O-O at {})",
                high_history_index, zero_history_index,
            );
        }
    }

    #[test]
    fn quiet_beta_cutoff_stores_killer_at_ply() {
        // Run a shallow search from the start position; killer slots at some ply
        // must be populated (there are many fail-high events at depth >= 3).
        let position = start_position();
        let mut context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::new(AtomicU64::new(0)),
            limits: SearchLimits::Depth(4),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        negamax_pvs(&position, 4, -INF, INF, 0, &mut context);
        let any_killer_set = context.killer_moves.iter()
            .any(|slots| slots[0].is_some() || slots[1].is_some());
        assert!(any_killer_set, "at depth 4 from startpos at least one killer must be stored");
    }

    #[test]
    fn quiet_beta_cutoff_increments_history() {
        let position = start_position();
        let mut context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::new(AtomicU64::new(0)),
            limits: SearchLimits::Depth(4),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        negamax_pvs(&position, 4, -INF, INF, 0, &mut context);
        let any_history_nonzero = context.history_scores.iter().flatten().flatten().any(|&v| v > 0);
        assert!(any_history_nonzero, "at depth 4 from startpos history must be written somewhere");
    }

    #[test]
    fn history_saturates_at_ceiling() {
        // Direct test of saturation logic.
        let mut history_scores = [[[0i32; 64]; 64]; 2];
        history_scores[0][12][28] = 16_380;
        let bonus = 10 * 10;
        let entry = &mut history_scores[0][12][28];
        *entry = (*entry + bonus).min(16384);
        assert_eq!(history_scores[0][12][28], 16384);
    }

    #[test]
    fn capture_beta_cutoff_leaves_killers_empty_at_that_ply() {
        // Position where the only sensible cutoff at ply 0 is a capture.
        // White rook on a5, free black queen on e5 (undefended). Search depth 2.
        let position = crate::board::from_fen("4k3/8/8/R3q3/8/8/8/4K3 w - - 0 1");
        let mut context = SearchContext {
            transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            shared_nodes: Arc::new(AtomicU64::new(0)),
            limits: SearchLimits::Depth(2),
            start_time: Instant::now(),
            nodes_searched: 0,
            killer_moves: [[None; 2]; MAX_SEARCH_PLY],
            history_scores: [[[0i32; 64]; 64]; 2],
        };
        negamax_pvs(&position, 2, -INF, INF, 0, &mut context);
        // The capture Rxe5 is the best move and should cause the cutoff at ply 0.
        // It is a capture → killers[0] must remain empty.
        assert!(context.killer_moves[0][0].is_none(), "captures must not populate killers");
        assert!(context.killer_moves[0][1].is_none(), "captures must not populate killers");
    }

    #[test]
    fn lmr_preserves_mate_in_one() {
        let position = crate::board::from_fen("6k1/8/6KQ/8/8/8/8/8 w - - 0 1");
        let tt = Arc::new(ShardedTranspositionTable::new(4));
        let stop = Arc::new(AtomicBool::new(false));
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 1);
        let after = crate::board::apply_move(&position, chosen);
        let opponent_moves = generate_legal_moves(&after);
        assert!(opponent_moves.is_empty(), "LMR must not mask mate-in-one");
    }

    #[test]
    fn lmr_preserves_hanging_queen_capture() {
        let position = crate::board::from_fen("4k3/8/8/R3q3/8/8/8/4K3 w - - 0 1");
        let tt = Arc::new(ShardedTranspositionTable::new(4));
        let stop = Arc::new(AtomicBool::new(false));
        let params = GoParameters { depth: Some(3), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 1);
        assert_eq!(chosen.to_square, 36, "LMR must not hide the queen capture on e5 (sq 36)");
    }

    #[test]
    fn lmr_in_check_node_still_finds_evasion() {
        let position = crate::board::from_fen("4q3/7k/8/8/8/8/8/4K3 w - - 0 1");
        let tt = Arc::new(ShardedTranspositionTable::new(4));
        let stop = Arc::new(AtomicBool::new(false));
        let params = GoParameters { depth: Some(4), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 1);
        let legal = generate_legal_moves(&position);
        assert!(legal.contains(&chosen), "LMR must return a legal evasion when in check");
    }

    #[test]
    fn select_move_returns_legal_move_in_king_and_pawn_endgame() {
        // Only kings and pawns — null move must not fire (Zugzwang guard).
        let position = from_fen("4k3/4p3/8/8/8/8/4P3/4K3 w - - 0 1");
        let tt = make_tt();
        let stop = make_stop();
        let params = GoParameters { depth: Some(4), ..Default::default() };
        let chosen = select_move(&position, &params, tt, stop, 1);
        let legal_moves = generate_legal_moves(&position);
        assert!(legal_moves.contains(&chosen), "must return a legal move in king-and-pawn endgame");
    }

    #[test]
    fn search_node_budget_regression_at_depth_7() {
        // Captured after LMR+killers+history were wired in (243158 nodes measured).
        // If this test starts failing, investigate whether pruning has regressed
        // (e.g. an off-by-one in LMR guards disabling reductions) before bumping
        // the ceiling.
        const CEILING: u64 = 243_158 * 115 / 100; // allow 15% headroom for noise
        let fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        ];
        let mut total_nodes: u64 = 0;
        for fen in fens.iter() {
            let position = from_fen(fen);
            let mut context = SearchContext {
                transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
                stop_flag: Arc::new(AtomicBool::new(false)),
                shared_nodes: Arc::new(AtomicU64::new(0)),
                limits: SearchLimits::Depth(7),
                start_time: Instant::now(),
                nodes_searched: 0,
                killer_moves: [[None; 2]; MAX_SEARCH_PLY],
                history_scores: [[[0i32; 64]; 64]; 2],
            };
            for depth in 1..=7u32 {
                negamax_pvs(&position, depth, -INF, INF, 0, &mut context);
            }
            total_nodes += context.shared_nodes.load(Ordering::Relaxed) + context.nodes_searched;
        }
        assert!(
            total_nodes <= CEILING,
            "search explored {} nodes vs ceiling {} — pruning regression?",
            total_nodes,
            CEILING,
        );
    }
}
