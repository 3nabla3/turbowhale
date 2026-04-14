use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::instrument;

use crate::board::{apply_move, from_fen, start_position, Position};
use crate::engine::select_move;
use crate::movegen::generate_legal_moves;
use crate::perft::perft_divide;
use crate::tt::TranspositionTable;

const START_POSITION_FEN: &str =
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug, Default, PartialEq)]
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
    pub perft_depth: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub enum UciCommand {
    Uci,
    Debug(bool),
    IsReady,
    SetOption { name: String, value: Option<String> },
    UciNewGame,
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
    let (fen, moves_section) = if let Some(after_startpos) = remainder.strip_prefix("startpos") {
        let after_startpos = after_startpos.trim();
        (START_POSITION_FEN.to_string(), after_startpos)
    } else if let Some(after_fen_keyword) = remainder.strip_prefix("fen ") {
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
            "perft"       => { parameters.perft_depth = tokens.next().and_then(|v| v.parse().ok()); }
            "searchmoves" => {
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
    let from_file = (chess_move.from_square % 8) + b'a';
    let from_rank = (chess_move.from_square / 8) + b'1';
    let to_file   = (chess_move.to_square % 8) + b'a';
    let to_rank   = (chess_move.to_square / 8) + b'1';

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
pub fn parse_uci_move_string(move_string: &str, position: &crate::board::Position) -> crate::board::Move {
    use crate::board::{MoveFlags, PieceType};

    let bytes = move_string.as_bytes();
    let from_file = bytes[0] - b'a';
    let from_rank = bytes[1] - b'1';
    let to_file   = bytes[2] - b'a';
    let to_rank   = bytes[3] - b'1';

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
    if is_capture || is_en_passant { move_flags |= MoveFlags::CAPTURE; }
    if is_en_passant               { move_flags |= MoveFlags::EN_PASSANT; }
    if is_double_pawn_push         { move_flags |= MoveFlags::DOUBLE_PAWN_PUSH; }
    if is_castling                 { move_flags |= MoveFlags::CASTLING; }

    crate::board::Move {
        from_square,
        to_square,
        promotion_piece,
        move_flags,
    }
}

// --- Runtime state and UCI loop ---

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
fn handle_uci_line(
    line: &str,
    output: &mut impl Write,
    state: &mut UciState,
) -> LineOutcome {
    let command = parse_uci_command(line);

    match command {
        UciCommand::Uci => {
            writeln!(output, "id name turbowhale").unwrap();
            writeln!(output, "id author 3nabla3").unwrap();
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
            if let Some(depth) = parameters.perft_depth {
                let divide = perft_divide(&state.current_position, depth);
                let total: u64 = divide.iter().map(|(_, count)| count).sum();
                for (chess_move, count) in divide {
                    writeln!(output, "{}: {}", move_to_uci_string(chess_move), count).unwrap();
                }
                writeln!(output, "").unwrap();
                writeln!(output, "Nodes searched: {}", total).unwrap();
                output.flush().unwrap();
                return LineOutcome::Continue;
            }

            state.stop_search();
            state.stop_flag.store(false, Ordering::Relaxed);

            let legal_moves = generate_legal_moves(&state.current_position);
            if legal_moves.is_empty() {
                writeln!(output, "bestmove 0000").unwrap();
                output.flush().unwrap();
                return LineOutcome::Continue;
            }

            let position = state.current_position.clone();
            let stop_flag = Arc::clone(&state.stop_flag);
            let tt_arc = Arc::clone(&state.transposition_table);

            let handle = std::thread::spawn(move || {
                let mut tt = tt_arc.lock().unwrap();
                let chosen = select_move(&position, &parameters, &mut tt, &stop_flag);
                println!("bestmove {}", move_to_uci_string(chosen));
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
            handle_uci_line(&line, output, &mut state),
            LineOutcome::Quit
        ) {
            break;
        }
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
        // The bestmove is printed to stdout by the search thread.
        // We verify the session completes cleanly (no panic, no hang).
        // quit correctly stops the search thread and joins it before returning.
        let input = b"position startpos\ngo\nquit\n";
        let mut output = Vec::new();
        run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
    }

    #[test]
    fn go_with_depth_produces_bestmove() {
        // The bestmove is printed to stdout by the search thread.
        // We verify the session completes cleanly (no panic, no hang).
        let input = b"position startpos\ngo depth 3\nquit\n";
        let mut output = Vec::new();
        run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
    }

    #[test]
    fn ucinewgame_clears_tt_without_panic() {
        // We verify the session completes cleanly with ucinewgame resetting state.
        let input = b"ucinewgame\nposition startpos\ngo depth 2\nquit\n";
        let mut output = Vec::new();
        run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
    }

    #[test]
    fn parse_go_perft_sets_perft_depth() {
        let command = parse_uci_command("go perft 5");
        match command {
            UciCommand::Go(parameters) => {
                assert_eq!(parameters.perft_depth, Some(5));
            }
            _ => panic!("Expected Go command"),
        }
    }

    #[test]
    fn go_perft_depth_1_prints_divide_and_total() {
        let input = b"position startpos\ngo perft 1\nquit\n";
        let mut output = Vec::new();
        run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
        let response = String::from_utf8(output).unwrap();
        // 20 move lines + blank line + total line
        assert!(response.contains("Nodes searched: 20"), "got: {}", response);
        // sanity check one known move is present
        assert!(response.contains("e2e4:"), "got: {}", response);
    }

    #[test]
    fn go_perft_depth_2_prints_correct_total() {
        let input = b"position startpos\ngo perft 2\nquit\n";
        let mut output = Vec::new();
        run_uci_loop(std::io::BufReader::new(input.as_ref()), &mut output);
        let response = String::from_utf8(output).unwrap();
        assert!(response.contains("Nodes searched: 400"), "got: {}", response);
    }
}
