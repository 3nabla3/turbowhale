use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::instrument;

use crate::board::{apply_move, try_from_fen, start_position, Position};
use crate::engine::select_move;
use crate::movegen::generate_legal_moves;
use crate::perft::perft_divide;
use crate::tt::ShardedTranspositionTable;

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
    // value_separator is the raw start of " value " (used as the name's end boundary).
    // value_start skips past " value " to reach the actual value text.
    let value_separator = remainder.find(" value ");
    let value_start = value_separator.map(|index| index + 7);

    let name = match (name_start, value_separator) {
        (Some(start), Some(separator)) => remainder[start..separator].trim().to_string(),
        (Some(start), None)            => remainder[start..].trim().to_string(),
        _                              => return UciCommand::Unknown(format!("setoption {}", remainder)),
    };

    let value = value_start.map(|start| remainder[start..].trim().to_string());

    UciCommand::SetOption { name, value }
}

fn parse_position(remainder: &str) -> UciCommand {
    let (fen, moves_section) = if let Some(after_startpos) = remainder.strip_prefix("startpos") {
        let after_startpos = after_startpos.trim();
        // Only treat the tail as a moves section if it actually begins with the "moves" keyword.
        // Any other content (e.g. a stray "fen …") is silently ignored per the UCI spec.
        let moves_section = if after_startpos.starts_with("moves") {
            after_startpos
        } else {
            ""
        };
        (START_POSITION_FEN.to_string(), moves_section)
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
/// Returns None if the string is too short to be a valid UCI move (fewer than 4 bytes).
pub fn parse_uci_move_string(
    move_string: &str,
    position: &crate::board::Position,
) -> Option<crate::board::Move> {
    use crate::board::{MoveFlags, PieceType};

    if move_string.len() < 4 {
        return None;
    }

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

    Some(crate::board::Move {
        from_square,
        to_square,
        promotion_piece,
        move_flags,
    })
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
    transposition_table: Arc<ShardedTranspositionTable>,
    search_thread: Option<std::thread::JoinHandle<()>>,
    output: Arc<Mutex<Box<dyn Write + Send>>>,
    thread_count: usize,
}

impl UciState {
    fn new(output: Arc<Mutex<Box<dyn Write + Send>>>) -> Self {
        UciState {
            current_position: start_position(),
            debug_mode: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
            transposition_table: Arc::new(ShardedTranspositionTable::new(16)),
            search_thread: None,
            output,
            thread_count: 1,
        }
    }

    fn stop_search(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.search_thread.take() {
            handle.join().ok();
        }
    }
}

#[instrument(skip(state))]
fn handle_uci_line(line: &str, state: &mut UciState) -> LineOutcome {
    let command = parse_uci_command(line);

    match command {
        UciCommand::Uci => {
            let mut output = state.output.lock().unwrap();
            writeln!(output, "id name {} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")).unwrap();
            writeln!(output, "id author {}", env!("CARGO_PKG_AUTHORS")).unwrap();
            writeln!(output, "option name Threads type spin default 1 min 1 max 64").unwrap();
            writeln!(output, "uciok").unwrap();
            output.flush().unwrap();
        }

        UciCommand::Debug(enabled) => {
            state.debug_mode = enabled;
        }

        UciCommand::IsReady => {
            let mut output = state.output.lock().unwrap();
            writeln!(output, "readyok").unwrap();
            output.flush().unwrap();
        }

        UciCommand::SetOption { name, value } => {
            if name == "Threads"
                && let Some(value_string) = value
                && let Ok(count) = value_string.parse::<usize>() {
                    state.thread_count = count.clamp(1, 64);
                }
        }

        UciCommand::UciNewGame => {
            state.stop_search();
            state.current_position = start_position();
            state.stop_flag.store(false, Ordering::Relaxed);
            state.transposition_table.clear();
        }

        UciCommand::Position { fen, moves } => {
            let parsed_position = match try_from_fen(&fen) {
                Ok(position) => position,
                Err(_) => return LineOutcome::Continue,
            };
            state.current_position = parsed_position;
            for uci_move_string in &moves {
                if let Some(chess_move) = parse_uci_move_string(uci_move_string, &state.current_position) {
                    state.current_position = apply_move(&state.current_position, chess_move);
                }
            }
        }

        UciCommand::Go(parameters) => {
            if let Some(depth) = parameters.perft_depth {
                let divide = perft_divide(&state.current_position, depth);
                let total: u64 = divide.iter().map(|(_, count)| count).sum();
                let mut output = state.output.lock().unwrap();
                for (chess_move, count) in divide {
                    writeln!(output, "{}: {}", move_to_uci_string(chess_move), count).unwrap();
                }
                writeln!(output).unwrap();
                writeln!(output, "Nodes searched: {}", total).unwrap();
                output.flush().unwrap();
                return LineOutcome::Continue;
            }

            state.stop_search();
            state.stop_flag.store(false, Ordering::Relaxed);

            let legal_moves = generate_legal_moves(&state.current_position);
            if legal_moves.is_empty() {
                let mut output = state.output.lock().unwrap();
                writeln!(output, "bestmove 0000").unwrap();
                output.flush().unwrap();
                return LineOutcome::Continue;
            }

            let position = state.current_position.clone();
            let stop_flag = Arc::clone(&state.stop_flag);
            let tt_arc = Arc::clone(&state.transposition_table);
            let output_arc = Arc::clone(&state.output);
            let thread_count = state.thread_count;

            let handle = std::thread::spawn(move || {
                let chosen = select_move(&position, &parameters, tt_arc, stop_flag, thread_count);
                let mut output = output_arc.lock().unwrap();
                writeln!(output, "bestmove {}", move_to_uci_string(chosen)).unwrap();
                output.flush().unwrap();
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
pub fn run_uci_loop(input: impl BufRead, output: impl Write + Send + 'static) {
    let output: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(output)));
    let mut state = UciState::new(output);

    for line in input.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("Error reading UCI input: {}", error);
                break;
            }
        };

        if matches!(handle_uci_line(&line, &mut state), LineOutcome::Quit) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────────────

    /// Captures all bytes written through it into a shared buffer so tests can
    /// inspect the UCI output after `run_uci_loop` returns.
    struct OutputCapture {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl OutputCapture {
        fn new() -> (Self, Arc<Mutex<Vec<u8>>>) {
            let buffer = Arc::new(Mutex::new(Vec::new()));
            (Self { buffer: Arc::clone(&buffer) }, Arc::clone(&buffer))
        }
    }

    impl Write for OutputCapture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.buffer.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    /// Drives `run_uci_loop` with the given input bytes and returns everything
    /// written to the output as a UTF-8 string.
    fn run_and_capture(input: &[u8]) -> String {
        let (capture, buffer) = OutputCapture::new();
        run_uci_loop(std::io::BufReader::new(input), capture);
        String::from_utf8(buffer.lock().unwrap().clone()).unwrap()
    }

    // ── Parser unit tests: uci ────────────────────────────────────────────────

    #[test]
    fn parse_uci_returns_uci_variant() {
        assert_eq!(parse_uci_command("uci"), UciCommand::Uci);
    }

    // ── Parser unit tests: debug ──────────────────────────────────────────────

    #[test]
    fn parse_debug_on_returns_debug_true() {
        assert_eq!(parse_uci_command("debug on"), UciCommand::Debug(true));
    }

    #[test]
    fn parse_debug_off_returns_debug_false() {
        assert_eq!(parse_uci_command("debug off"), UciCommand::Debug(false));
    }

    #[test]
    fn parse_debug_with_no_argument_defaults_to_false() {
        assert_eq!(parse_uci_command("debug"), UciCommand::Debug(false));
    }

    // ── Parser unit tests: isready ────────────────────────────────────────────

    #[test]
    fn parse_isready_returns_isready_variant() {
        assert_eq!(parse_uci_command("isready"), UciCommand::IsReady);
    }

    // ── Parser unit tests: setoption ──────────────────────────────────────────

    #[test]
    fn parse_setoption_with_name_and_integer_value() {
        assert_eq!(
            parse_uci_command("setoption name Hash value 128"),
            UciCommand::SetOption { name: "Hash".to_string(), value: Some("128".to_string()) },
        );
    }

    #[test]
    fn parse_setoption_with_name_only() {
        assert_eq!(
            parse_uci_command("setoption name OwnBook"),
            UciCommand::SetOption { name: "OwnBook".to_string(), value: None },
        );
    }

    #[test]
    fn parse_setoption_with_multiword_name_and_value() {
        assert_eq!(
            parse_uci_command("setoption name Skill Level value 10"),
            UciCommand::SetOption {
                name: "Skill Level".to_string(),
                value: Some("10".to_string()),
            },
        );
    }

    #[test]
    fn parse_setoption_without_name_keyword_returns_unknown() {
        match parse_uci_command("setoption") {
            UciCommand::Unknown(_) => {}
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    // ── Parser unit tests: register (not in enum — must return Unknown) ───────

    #[test]
    fn parse_register_later_returns_unknown() {
        match parse_uci_command("register later") {
            UciCommand::Unknown(_) => {}
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn parse_register_name_and_code_returns_unknown() {
        match parse_uci_command("register name Stefan MK code 4359874324") {
            UciCommand::Unknown(_) => {}
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    // ── Parser unit tests: ucinewgame ─────────────────────────────────────────

    #[test]
    fn parse_ucinewgame_returns_ucinewgame_variant() {
        assert_eq!(parse_uci_command("ucinewgame"), UciCommand::UciNewGame);
    }

    // ── Parser unit tests: position ───────────────────────────────────────────

    #[test]
    fn parse_position_startpos_with_no_moves() {
        assert_eq!(
            parse_uci_command("position startpos"),
            UciCommand::Position { fen: START_POSITION_FEN.to_string(), moves: vec![] },
        );
    }

    #[test]
    fn parse_position_startpos_with_moves_keyword_and_no_move_list() {
        assert_eq!(
            parse_uci_command("position startpos moves"),
            UciCommand::Position { fen: START_POSITION_FEN.to_string(), moves: vec![] },
        );
    }

    #[test]
    fn parse_position_startpos_with_single_move() {
        assert_eq!(
            parse_uci_command("position startpos moves e2e4"),
            UciCommand::Position {
                fen: START_POSITION_FEN.to_string(),
                moves: vec!["e2e4".to_string()],
            },
        );
    }

    #[test]
    fn parse_position_startpos_with_multiple_moves() {
        assert_eq!(
            parse_uci_command("position startpos moves e2e4 e7e5 g1f3"),
            UciCommand::Position {
                fen: START_POSITION_FEN.to_string(),
                moves: vec!["e2e4".to_string(), "e7e5".to_string(), "g1f3".to_string()],
            },
        );
    }

    #[test]
    fn parse_position_fen_with_no_moves() {
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        assert_eq!(
            parse_uci_command(&format!("position fen {}", fen)),
            UciCommand::Position { fen: fen.to_string(), moves: vec![] },
        );
    }

    #[test]
    fn parse_position_fen_with_single_move() {
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        assert_eq!(
            parse_uci_command(&format!("position fen {} moves e7e5", fen)),
            UciCommand::Position {
                fen: fen.to_string(),
                moves: vec!["e7e5".to_string()],
            },
        );
    }

    #[test]
    fn parse_position_fen_with_multiple_moves() {
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        assert_eq!(
            parse_uci_command(&format!("position fen {} moves e7e5 g1f3", fen)),
            UciCommand::Position {
                fen: fen.to_string(),
                moves: vec!["e7e5".to_string(), "g1f3".to_string()],
            },
        );
    }

    #[test]
    fn parse_position_startpos_followed_by_fen_keyword_does_not_crash() {
        // Malformed per spec; the engine must not panic. Acceptable outcomes:
        // Position with the startpos FEN and no moves, or Unknown.
        let command = parse_uci_command(
            "position startpos fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        );
        match command {
            UciCommand::Position { fen, moves } => {
                assert_eq!(fen, START_POSITION_FEN, "FEN must be the startpos FEN");
                assert!(moves.is_empty(), "malformed tail must not produce moves: {:?}", moves);
            }
            UciCommand::Unknown(_) => {}
            other => panic!("Unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn parse_position_with_no_arguments_returns_unknown() {
        match parse_uci_command("position") {
            UciCommand::Unknown(_) => {}
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn parse_position_with_unrecognised_keyword_returns_unknown() {
        match parse_uci_command("position custompos") {
            UciCommand::Unknown(_) => {}
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    // ── Parser unit tests: go ─────────────────────────────────────────────────

    #[test]
    fn parse_go_with_no_parameters_returns_default() {
        assert_eq!(parse_uci_command("go"), UciCommand::Go(GoParameters::default()));
    }

    #[test]
    fn parse_go_infinite() {
        match parse_uci_command("go infinite") {
            UciCommand::Go(params) => assert!(params.infinite),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_ponder() {
        match parse_uci_command("go ponder") {
            UciCommand::Go(params) => assert!(params.ponder),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_movetime() {
        match parse_uci_command("go movetime 5000") {
            UciCommand::Go(params) => assert_eq!(params.move_time_ms, Some(5000)),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_depth() {
        match parse_uci_command("go depth 10") {
            UciCommand::Go(params) => assert_eq!(params.depth, Some(10)),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_nodes() {
        match parse_uci_command("go nodes 1000000") {
            UciCommand::Go(params) => assert_eq!(params.nodes, Some(1_000_000)),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_mate() {
        match parse_uci_command("go mate 3") {
            UciCommand::Go(params) => assert_eq!(params.mate_in_moves, Some(3)),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_movestogo() {
        match parse_uci_command("go movestogo 40") {
            UciCommand::Go(params) => assert_eq!(params.moves_to_go, Some(40)),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_all_time_controls() {
        match parse_uci_command("go wtime 60000 btime 45000 winc 1000 binc 500") {
            UciCommand::Go(params) => {
                assert_eq!(params.white_time_remaining_ms, Some(60000));
                assert_eq!(params.black_time_remaining_ms, Some(45000));
                assert_eq!(params.white_increment_ms, Some(1000));
                assert_eq!(params.black_increment_ms, Some(500));
            }
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_searchmoves_collects_all_trailing_moves() {
        match parse_uci_command("go searchmoves e2e4 d2d4") {
            UciCommand::Go(params) => {
                assert_eq!(params.search_moves, vec!["e2e4".to_string(), "d2d4".to_string()]);
            }
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_searchmoves_with_no_trailing_moves_produces_empty_list() {
        match parse_uci_command("go searchmoves") {
            UciCommand::Go(params) => assert!(params.search_moves.is_empty()),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_with_non_numeric_depth_produces_none() {
        match parse_uci_command("go depth abc") {
            UciCommand::Go(params) => assert_eq!(params.depth, None),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    #[test]
    fn parse_go_perft_sets_perft_depth() {
        match parse_uci_command("go perft 5") {
            UciCommand::Go(params) => assert_eq!(params.perft_depth, Some(5)),
            other => panic!("Expected Go, got {:?}", other),
        }
    }

    // ── Parser unit tests: stop ───────────────────────────────────────────────

    #[test]
    fn parse_stop_returns_stop_variant() {
        assert_eq!(parse_uci_command("stop"), UciCommand::Stop);
    }

    // ── Parser unit tests: ponderhit ──────────────────────────────────────────

    #[test]
    fn parse_ponderhit_returns_ponderhit_variant() {
        assert_eq!(parse_uci_command("ponderhit"), UciCommand::PonderHit);
    }

    // ── Parser unit tests: quit ───────────────────────────────────────────────

    #[test]
    fn parse_quit_returns_quit_variant() {
        assert_eq!(parse_uci_command("quit"), UciCommand::Quit);
    }

    // ── Parser unit tests: unknown / malformed ────────────────────────────────

    #[test]
    fn parse_unrecognised_command_returns_unknown_with_full_text() {
        match parse_uci_command("foobar baz") {
            UciCommand::Unknown(text) => assert_eq!(text, "foobar baz"),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn parse_empty_string_returns_unknown() {
        match parse_uci_command("") {
            UciCommand::Unknown(_) => {}
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn parse_whitespace_only_returns_unknown() {
        match parse_uci_command("   ") {
            UciCommand::Unknown(_) => {}
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    // ── Parser unit tests: parse_uci_move_string ──────────────────────────────

    #[test]
    fn parse_uci_move_string_with_too_short_string_returns_none() {
        let position = crate::board::start_position();
        assert_eq!(parse_uci_move_string("e2e", &position), None);
        assert_eq!(parse_uci_move_string("e2", &position), None);
        assert_eq!(parse_uci_move_string("", &position), None);
    }

    #[test]
    fn parse_uci_move_string_with_valid_move_returns_some() {
        let position = crate::board::start_position();
        assert!(parse_uci_move_string("e2e4", &position).is_some());
    }

    #[test]
    fn parse_uci_move_string_with_valid_promotion_returns_some() {
        let position = crate::board::from_fen("8/4P3/8/8/8/8/8/4K1k1 w - - 0 1");
        assert!(parse_uci_move_string("e7e8q", &position).is_some());
    }

    // ── Integration tests: uci handshake ─────────────────────────────────────

    #[test]
    fn uci_response_contains_id_name_id_author_and_uciok() {
        let response = run_and_capture(b"uci\nquit\n");
        assert!(response.contains("id name"), "missing 'id name' in: {}", response);
        assert!(response.contains("id author"), "missing 'id author' in: {}", response);
        assert!(response.contains("uciok"), "missing 'uciok' in: {}", response);
    }

    #[test]
    fn uci_id_name_appears_before_uciok() {
        let response = run_and_capture(b"uci\nquit\n");
        let name_pos = response.find("id name").unwrap();
        let uciok_pos = response.find("uciok").unwrap();
        assert!(name_pos < uciok_pos, "'id name' must precede 'uciok'");
    }

    // ── Integration tests: isready ────────────────────────────────────────────

    #[test]
    fn isready_produces_readyok() {
        let response = run_and_capture(b"isready\nquit\n");
        assert!(response.contains("readyok"), "missing 'readyok' in: {}", response);
    }

    // ── Integration tests: debug ──────────────────────────────────────────────

    #[test]
    fn debug_on_is_accepted_without_output() {
        let response = run_and_capture(b"debug on\nquit\n");
        assert!(response.is_empty(), "debug on must produce no output, got: {}", response);
    }

    #[test]
    fn debug_off_is_accepted_without_output() {
        let response = run_and_capture(b"debug off\nquit\n");
        assert!(response.is_empty(), "debug off must produce no output, got: {}", response);
    }

    // ── Integration tests: setoption ──────────────────────────────────────────

    #[test]
    fn setoption_is_accepted_silently() {
        let response = run_and_capture(b"setoption name Hash value 128\nquit\n");
        assert!(response.is_empty(), "setoption must produce no output, got: {}", response);
    }

    // ── Integration tests: ucinewgame ─────────────────────────────────────────

    #[test]
    fn ucinewgame_is_accepted_silently() {
        let response = run_and_capture(b"ucinewgame\nquit\n");
        assert!(response.is_empty(), "ucinewgame must produce no output, got: {}", response);
    }

    // ── Integration tests: position ───────────────────────────────────────────

    #[test]
    fn position_startpos_is_accepted_silently() {
        let response = run_and_capture(b"position startpos\nquit\n");
        assert!(response.is_empty(), "position must produce no output, got: {}", response);
    }

    #[test]
    fn position_startpos_with_moves_is_accepted_silently() {
        let response = run_and_capture(b"position startpos moves e2e4 e7e5\nquit\n");
        assert!(response.is_empty(), "position with moves must produce no output, got: {}", response);
    }

    #[test]
    fn position_fen_is_accepted_silently() {
        let response = run_and_capture(
            b"position fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1\nquit\n",
        );
        assert!(response.is_empty(), "position fen must produce no output, got: {}", response);
    }

    #[test]
    fn position_fen_with_moves_is_accepted_silently() {
        let response = run_and_capture(
            b"position fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1 moves e7e5\nquit\n",
        );
        assert!(response.is_empty(), "position fen with moves must produce no output, got: {}", response);
    }

    #[test]
    fn malformed_position_startpos_fen_does_not_crash() {
        // This is the exact input that previously caused a panic (Bug 2 + Bug 3).
        run_and_capture(
            b"position startpos fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\nquit\n",
        );
    }

    // ── Integration tests: go / bestmove ─────────────────────────────────────

    #[test]
    fn go_depth_1_produces_bestmove_in_output() {
        let response = run_and_capture(b"position startpos\ngo depth 1\nquit\n");
        assert!(response.contains("bestmove"), "bestmove not found in output: {}", response);
    }

    #[test]
    fn go_depth_produces_bestmove_with_valid_move_format() {
        let response = run_and_capture(b"position startpos\ngo depth 1\nquit\n");
        let bestmove_line = response
            .lines()
            .find(|line| line.starts_with("bestmove"))
            .expect("no bestmove line found");
        let move_token = bestmove_line.split_whitespace().nth(1).expect("no move after bestmove");
        assert!(
            move_token.len() == 4 || move_token.len() == 5,
            "move token '{}' has unexpected length",
            move_token,
        );
    }

    #[test]
    fn stop_after_go_infinite_produces_bestmove() {
        // stop_search() joins the search thread, so bestmove is written before
        // run_uci_loop returns.
        let response = run_and_capture(b"position startpos\ngo infinite\nstop\nquit\n");
        assert!(response.contains("bestmove"), "bestmove not found in output: {}", response);
    }

    #[test]
    fn go_perft_depth_1_prints_node_count_of_20() {
        let response = run_and_capture(b"position startpos\ngo perft 1\nquit\n");
        assert!(response.contains("Nodes searched: 20"), "unexpected perft output: {}", response);
    }

    #[test]
    fn go_perft_depth_2_prints_node_count_of_400() {
        let response = run_and_capture(b"position startpos\ngo perft 2\nquit\n");
        assert!(response.contains("Nodes searched: 400"), "unexpected perft output: {}", response);
    }

    // ── Integration tests: ponderhit ──────────────────────────────────────────

    #[test]
    fn ponderhit_is_accepted_silently() {
        let response = run_and_capture(b"ponderhit\nquit\n");
        assert!(response.is_empty(),
            "ponderhit must produce no output, got: {}", response);
    }

    // ── Integration tests: quit ───────────────────────────────────────────────

    #[test]
    fn quit_causes_loop_to_exit_cleanly() {
        // If quit did not exit the loop, this call would block forever.
        run_and_capture(b"quit\n");
    }

    // ── Integration tests: setoption Threads ─────────────────────────────────

    #[test]
    fn setoption_threads_updates_thread_count() {
        // After setting Threads to 4, a subsequent go should use that count.
        // We verify indirectly: the option is accepted silently (no output).
        let response = run_and_capture(b"setoption name Threads value 4\nquit\n");
        assert!(response.is_empty(), "setoption Threads must produce no output, got: {}", response);
    }

    #[test]
    fn uci_response_advertises_threads_option() {
        let response = run_and_capture(b"uci\nquit\n");
        assert!(
            response.contains("option name Threads type spin default 1 min 1 max 64"),
            "uci response must advertise Threads option, got: {}",
            response,
        );
    }

    // ── Integration tests: unknown commands ───────────────────────────────────

    #[test]
    fn unknown_command_outside_debug_mode_produces_no_output() {
        let response = run_and_capture(b"this_is_not_a_uci_command\nquit\n");
        assert!(response.is_empty(), "unknown command must produce no output, got: {}", response);
    }

    // ── Integration tests: multi-game session ─────────────────────────────────

    #[test]
    fn full_game_session_sequence_does_not_crash() {
        run_and_capture(
            b"uci\nisready\nucinewgame\nposition startpos\ngo depth 1\nstop\n\
              ucinewgame\nposition startpos moves e2e4\ngo depth 1\nstop\nquit\n",
        );
    }
}
