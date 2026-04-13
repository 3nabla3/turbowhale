use std::io::{BufRead, Write};

use tracing::instrument;

use crate::board::{apply_move, from_fen, start_position, Position};
use crate::engine::select_move;
use crate::movegen::generate_legal_moves;

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
    use crate::board::{MoveFlags, PieceType};

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
                writeln!(output, "id name chess-engine").unwrap();
                writeln!(output, "id author chess-engine").unwrap();
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
                    current_position = apply_move(&current_position, chess_move);
                }
            }

            UciCommand::Go(_parameters) => {
                let legal_moves = generate_legal_moves(&current_position);
                if legal_moves.is_empty() {
                    writeln!(output, "bestmove 0000").unwrap();
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
        assert!(response.contains("id name chess-engine"));
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
}
