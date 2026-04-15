# UCI Spec Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two panic-inducing bugs in UCI input parsing, fix the `bestmove` output routing bug, and add comprehensive test coverage for the entire UCI protocol spec.

**Architecture:** All changes are confined to `src/uci.rs` and `src/main.rs`. The output writer is promoted from a `&mut impl Write` parameter to an `Arc<Mutex<Box<dyn Write + Send>>>` stored on `UciState`, so the background search thread can write `bestmove` through the same writer that the main loop uses. Bug fixes are applied to `parse_position` and `parse_uci_move_string`. Tests are organized into parser unit tests (pure functions, no I/O) and integration tests (driving `run_uci_loop` end-to-end).

**Tech Stack:** Rust 1.94, `std::sync::{Arc, Mutex}`, `std::io::{Write, BufRead}`

---

## Known bugs to fix

### Bug 1 — `bestmove` printed to stdout, not to the output writer

`handle_uci_line` spawns a background thread for `Go`. That thread uses `println!("bestmove ...")` (line 329), which goes to real stdout regardless of what writer was passed to `run_uci_loop`. Any test that injects a `Vec<u8>` as the output will never see `bestmove`.

### Bug 2 — `position startpos fen <fen>` panics

`parse_position` checks `remainder.strip_prefix("startpos")`. It succeeds, leaving `" fen rnbqkbnr/..."` as `moves_section`. The code then splits that string on whitespace and treats every token as a UCI move string. `parse_uci_move_string` indexes `bytes[3]` unconditionally; a token like `"fen"` (3 bytes) causes an out-of-bounds panic.

### Bug 3 — `parse_uci_move_string` panics on strings shorter than 4 bytes

The function indexes `bytes[0]..bytes[3]` with no bounds check. Any move string shorter than 4 bytes causes a panic. This can be triggered by Bug 2 and also by any other path that passes a malformed move token.

---

## File map

| File | Change |
|------|--------|
| `src/uci.rs` | Fix output routing; fix parse_position; fix parse_uci_move_string; replace all existing tests; add ~50 new tests |
| `src/main.rs` | Update `run_uci_loop` call to match new signature |

---

## Task 1: Fix `bestmove` output routing

**Files:** Modify `src/uci.rs`, `src/main.rs`

The fix promotes the output writer into `UciState` as `Arc<Mutex<Box<dyn Write + Send>>>`. `handle_uci_line` no longer takes an `output` parameter — it uses `state.output`. The background search thread clones the Arc and writes `bestmove` through it.

- [ ] **Step 1: Write the failing test**

Add this test inside the existing `#[cfg(test)] mod tests` block in `src/uci.rs`. This test will fail before the fix because `bestmove` goes to real stdout, not to the captured writer.

```rust
// Helper used throughout the integration tests — add once at the top of the test module.
use std::io::Write;
use std::sync::{Arc, Mutex};

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

fn run_and_capture(input: &[u8]) -> String {
    let (capture, buffer) = OutputCapture::new();
    run_uci_loop(std::io::BufReader::new(input), capture);
    String::from_utf8(buffer.lock().unwrap().clone()).unwrap()
}

#[test]
fn go_depth_1_produces_bestmove_in_output() {
    let response = run_and_capture(b"position startpos\ngo depth 1\nquit\n");
    assert!(response.contains("bestmove"), "bestmove not found in output: {}", response);
}
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cargo test -p turbowhale go_depth_1_produces_bestmove_in_output -- --nocapture
```

Expected: test fails — the output string does not contain `"bestmove"`.

- [ ] **Step 3: Rewrite `UciState`, `handle_uci_line`, and `run_uci_loop` in `src/uci.rs`**

Replace the `UciState` struct and its `impl` block:

```rust
struct UciState {
    current_position: Position,
    debug_mode: bool,
    stop_flag: Arc<AtomicBool>,
    transposition_table: Arc<Mutex<TranspositionTable>>,
    search_thread: Option<std::thread::JoinHandle<()>>,
    output: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl UciState {
    fn new(output: Arc<Mutex<Box<dyn Write + Send>>>) -> Self {
        UciState {
            current_position: start_position(),
            debug_mode: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
            transposition_table: Arc::new(Mutex::new(TranspositionTable::new(16))),
            search_thread: None,
            output,
        }
    }

    fn stop_search(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.search_thread.take() {
            handle.join().ok();
        }
    }
}
```

Replace the `handle_uci_line` signature and all usages of the `output` parameter. Remove the `output` parameter entirely — the function now reads `state.output`:

```rust
#[instrument(skip(state))]
fn handle_uci_line(line: &str, state: &mut UciState) -> LineOutcome {
    let command = parse_uci_command(line);

    match command {
        UciCommand::Uci => {
            let mut output = state.output.lock().unwrap();
            writeln!(output, "id name {} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")).unwrap();
            writeln!(output, "id author {}", env!("CARGO_PKG_AUTHORS")).unwrap();
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

            let handle = std::thread::spawn(move || {
                let mut tt = tt_arc.lock().unwrap();
                let chosen = select_move(&position, &parameters, &mut tt, &stop_flag);
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
```

Replace `run_uci_loop`:

```rust
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
```

Add `use std::io::Write;` at the top of the file if not already present.

- [ ] **Step 4: Update `src/main.rs`**

The call site passes `&mut stdout`. Change it to pass `stdout` by value (owned):

```rust
// old:
uci::run_uci_loop(stdin, &mut stdout);

// new:
uci::run_uci_loop(stdin, stdout);
```

Also remove `let mut` from the stdout binding since it is no longer mutably borrowed:

```rust
let stdout = std::io::stdout();
uci::run_uci_loop(stdin, stdout);
```

- [ ] **Step 5: Build to confirm no compilation errors**

```bash
cargo build 2>&1
```

Expected: compiles cleanly.

- [ ] **Step 6: Run the test to confirm it passes**

```bash
cargo test -p turbowhale go_depth_1_produces_bestmove_in_output -- --nocapture
```

Expected: PASS — output contains `"bestmove"`.

- [ ] **Step 7: Run the full test suite to confirm no regressions**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/uci.rs src/main.rs
git commit -m "$(cat <<'EOF'
fix: route bestmove output through the injected writer instead of stdout

The search thread now writes bestmove via an Arc<Mutex<Box<dyn Write + Send>>>
stored on UciState, making the output testable and consistent with all other
UCI responses.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Fix `parse_position` crash on malformed `startpos fen …` input

**Files:** Modify `src/uci.rs`

When the remainder after `"position "` starts with `"startpos"` but is followed by `" fen ..."` instead of `" moves ..."`, the current code treats the whole tail as the moves section. The tokens `"fen"`, `"rnbqkbnr/..."`, etc., are passed to `parse_uci_move_string` which panics on strings shorter than 4 bytes. The fix makes `parse_position` only honour the moves section when it actually starts with `"moves"`.

- [ ] **Step 1: Write the failing test**

Add this to the parser unit tests section of `#[cfg(test)] mod tests`:

```rust
#[test]
fn parse_position_startpos_followed_by_fen_keyword_does_not_crash() {
    // Malformed command per the UCI spec; must not panic.
    // Acceptable outcomes: Position with startpos FEN and no moves, or Unknown.
    let command = parse_uci_command(
        "position startpos fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    );
    match command {
        UciCommand::Position { fen, moves } => {
            assert_eq!(fen, START_POSITION_FEN, "FEN must be startpos");
            assert!(moves.is_empty(), "malformed tail must not produce moves: {:?}", moves);
        }
        UciCommand::Unknown(_) => {}
        other => panic!("Unexpected variant: {:?}", other),
    }
}
```

- [ ] **Step 2: Run the test to confirm it panics**

```bash
cargo test -p turbowhale parse_position_startpos_followed_by_fen_keyword_does_not_crash 2>&1
```

Expected: test panics / fails.

- [ ] **Step 3: Fix `parse_position` in `src/uci.rs`**

Replace the `startpos` branch inside `parse_position`:

```rust
fn parse_position(remainder: &str) -> UciCommand {
    let (fen, moves_section) = if let Some(after_startpos) = remainder.strip_prefix("startpos") {
        let after_startpos = after_startpos.trim();
        // Only treat the tail as a moves section if it actually starts with "moves".
        // Any other content (e.g. a stray "fen …") is silently ignored.
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
```

- [ ] **Step 4: Run the test to confirm it passes**

```bash
cargo test -p turbowhale parse_position_startpos_followed_by_fen_keyword_does_not_crash
```

Expected: PASS.

- [ ] **Step 5: Run the full test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/uci.rs
git commit -m "$(cat <<'EOF'
fix: parse_position ignores non-moves tail after startpos

Prevents panic when input like "position startpos fen ..." is received;
the extra content after startpos is silently ignored unless it starts with
the "moves" keyword.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Fix `parse_uci_move_string` panic on short strings

**Files:** Modify `src/uci.rs`

`parse_uci_move_string` indexes `bytes[0]..bytes[3]` with no length check. Any string shorter than 4 bytes causes an index-out-of-bounds panic. The function's return type changes from `Move` to `Option<Move>`. The single call site in `handle_uci_line` is already updated in Task 1 to handle `Option<Move>` (it was changed to use `if let Some(...)`). This task adds the length guard to the function itself.

- [ ] **Step 1: Write the failing test**

```rust
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
    // Position with a pawn on e7 ready to promote.
    let position = crate::board::from_fen("8/4P3/8/8/8/8/8/4K1k1 w - - 0 1");
    assert!(parse_uci_move_string("e7e8q", &position).is_some());
}
```

- [ ] **Step 2: Run the tests to confirm they fail or panic**

```bash
cargo test -p turbowhale parse_uci_move_string 2>&1
```

Expected: the first test panics on `"e2e"`.

- [ ] **Step 3: Change `parse_uci_move_string` to return `Option<Move>`**

Replace the function signature and add the length guard:

```rust
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
```

- [ ] **Step 4: Run the tests to confirm they pass**

```bash
cargo test -p turbowhale parse_uci_move_string
```

Expected: all three tests PASS.

- [ ] **Step 5: Run the full test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/uci.rs
git commit -m "$(cat <<'EOF'
fix: parse_uci_move_string returns Option and guards against short strings

Prevents index-out-of-bounds panic on move strings shorter than 4 bytes.
The Position handler already skips None moves from Task 1.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Comprehensive parser unit tests

**Files:** Modify `src/uci.rs` (tests module only)

These tests target `parse_uci_command` and `parse_go_parameters` directly — no I/O, no engine state. They cover every command and every `go` sub-parameter in the UCI spec, plus the malformed-input cases required for robustness.

- [ ] **Step 1: Add all parser tests to the `#[cfg(test)] mod tests` block**

```rust
// ── uci ─────────────────────────────────────────────────────────────────────

#[test]
fn parse_uci_returns_uci_variant() {
    assert_eq!(parse_uci_command("uci"), UciCommand::Uci);
}

// ── debug ────────────────────────────────────────────────────────────────────

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

// ── isready ──────────────────────────────────────────────────────────────────

#[test]
fn parse_isready_returns_isready_variant() {
    assert_eq!(parse_uci_command("isready"), UciCommand::IsReady);
}

// ── setoption ─────────────────────────────────────────────────────────────────

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

// ── register (not in enum — must not crash, must return Unknown) ──────────────

#[test]
fn parse_register_returns_unknown() {
    match parse_uci_command("register later") {
        UciCommand::Unknown(_) => {}
        other => panic!("Expected Unknown, got {:?}", other),
    }
}

#[test]
fn parse_register_name_returns_unknown() {
    match parse_uci_command("register name Stefan MK code 4359874324") {
        UciCommand::Unknown(_) => {}
        other => panic!("Expected Unknown, got {:?}", other),
    }
}

// ── ucinewgame ────────────────────────────────────────────────────────────────

#[test]
fn parse_ucinewgame_returns_ucinewgame_variant() {
    assert_eq!(parse_uci_command("ucinewgame"), UciCommand::UciNewGame);
}

// ── position ─────────────────────────────────────────────────────────────────

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
    // Malformed per spec; acceptable: Position{startpos, no moves} or Unknown.
    let command = parse_uci_command(
        "position startpos fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    );
    match command {
        UciCommand::Position { fen, moves } => {
            assert_eq!(fen, START_POSITION_FEN, "FEN must be startpos");
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

// ── go ────────────────────────────────────────────────────────────────────────

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

// ── stop ──────────────────────────────────────────────────────────────────────

#[test]
fn parse_stop_returns_stop_variant() {
    assert_eq!(parse_uci_command("stop"), UciCommand::Stop);
}

// ── ponderhit ────────────────────────────────────────────────────────────────

#[test]
fn parse_ponderhit_returns_ponderhit_variant() {
    assert_eq!(parse_uci_command("ponderhit"), UciCommand::PonderHit);
}

// ── quit ─────────────────────────────────────────────────────────────────────

#[test]
fn parse_quit_returns_quit_variant() {
    assert_eq!(parse_uci_command("quit"), UciCommand::Quit);
}

// ── unknown / malformed ───────────────────────────────────────────────────────

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
```

- [ ] **Step 2: Run the parser tests**

```bash
cargo test -p turbowhale -- uci::tests 2>&1 | grep -E "FAILED|ok|error"
```

Expected: all tests pass. If any fail, investigate and fix `parse_uci_command` for that case.

- [ ] **Step 3: Commit**

```bash
git add src/uci.rs
git commit -m "$(cat <<'EOF'
test: add comprehensive parser unit tests covering the full UCI spec

Every command (uci, debug, isready, setoption, register, ucinewgame,
position, go, stop, ponderhit, quit, unknown) and every go sub-parameter
now has a dedicated test. Malformed inputs are also covered.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Comprehensive integration tests

**Files:** Modify `src/uci.rs` (tests module only)

These tests drive `run_uci_loop` end-to-end using `run_and_capture` (defined in Task 1). They verify that every command is accepted without crashing and that the commands which produce output do so correctly. The `go` tests wait for the search thread to finish (via `quit` calling `stop_search()` which joins the thread) before asserting output.

- [ ] **Step 1: Add all integration tests to the `#[cfg(test)] mod tests` block**

```rust
// ── uci handshake ─────────────────────────────────────────────────────────────

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

// ── isready ───────────────────────────────────────────────────────────────────

#[test]
fn isready_produces_readyok() {
    let response = run_and_capture(b"isready\nquit\n");
    assert!(response.contains("readyok"), "missing 'readyok' in: {}", response);
}

// ── debug ─────────────────────────────────────────────────────────────────────

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

// ── setoption ─────────────────────────────────────────────────────────────────

#[test]
fn setoption_is_accepted_silently() {
    let response = run_and_capture(b"setoption name Hash value 128\nquit\n");
    assert!(response.is_empty(), "setoption must produce no output, got: {}", response);
}

// ── ucinewgame ────────────────────────────────────────────────────────────────

#[test]
fn ucinewgame_is_accepted_silently() {
    let response = run_and_capture(b"ucinewgame\nquit\n");
    assert!(response.is_empty(), "ucinewgame must produce no output, got: {}", response);
}

// ── position ─────────────────────────────────────────────────────────────────

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

// ── go / bestmove ─────────────────────────────────────────────────────────────

#[test]
fn go_depth_1_produces_bestmove_in_output() {
    let response = run_and_capture(b"position startpos\ngo depth 1\nquit\n");
    assert!(response.contains("bestmove"), "bestmove not found in output: {}", response);
}

#[test]
fn go_depth_produces_bestmove_with_valid_move_format() {
    let response = run_and_capture(b"position startpos\ngo depth 1\nquit\n");
    // bestmove line must match "bestmove <4-or-5-char move>"
    let bestmove_line = response
        .lines()
        .find(|line| line.starts_with("bestmove"))
        .expect("no bestmove line found");
    let move_token = bestmove_line.split_whitespace().nth(1).expect("no move after bestmove");
    assert!(
        move_token.len() == 4 || move_token.len() == 5,
        "move token '{}' has unexpected length",
        move_token
    );
}

#[test]
fn stop_after_go_infinite_produces_bestmove() {
    // stop_search() joins the thread, so bestmove is written before run_uci_loop returns.
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

// ── ponderhit ────────────────────────────────────────────────────────────────

#[test]
fn ponderhit_is_accepted_silently() {
    let response = run_and_capture(b"ponderhit\nquit\n");
    assert!(response.is_empty(), "ponderhit must produce no output, got: {}", response);
}

// ── quit ─────────────────────────────────────────────────────────────────────

#[test]
fn quit_causes_loop_to_exit_cleanly() {
    // If quit did not exit the loop, this would block forever reading stdin.
    run_and_capture(b"quit\n");
}

// ── unknown commands ─────────────────────────────────────────────────────────

#[test]
fn unknown_command_outside_debug_mode_produces_no_output() {
    let response = run_and_capture(b"this_is_not_a_uci_command\nquit\n");
    assert!(response.is_empty(), "unknown command must produce no output, got: {}", response);
}

// ── multi-game session ────────────────────────────────────────────────────────

#[test]
fn full_game_session_sequence_does_not_crash() {
    run_and_capture(
        b"uci\nisready\nucinewgame\nposition startpos\ngo depth 1\nstop\n\
          ucinewgame\nposition startpos moves e2e4\ngo depth 1\nstop\nquit\n",
    );
}
```

- [ ] **Step 2: Run the integration tests**

```bash
cargo test -p turbowhale -- uci::tests 2>&1 | grep -E "FAILED|ok|error|panicked"
```

Expected: all tests pass. If any fail, investigate and fix accordingly.

- [ ] **Step 3: Run the complete test suite**

```bash
cargo test
```

Expected: all tests pass (including perft correctness tests).

- [ ] **Step 4: Commit**

```bash
git add src/uci.rs
git commit -m "$(cat <<'EOF'
test: add comprehensive UCI protocol integration tests

Every protocol command is now exercised end-to-end through run_uci_loop
with a captured writer. Tests cover correct output (uciok, readyok,
bestmove), silent acceptance of state-mutating commands, no-crash
guarantees for malformed inputs, and a multi-game session sequence.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Self-review against spec

| UCI command | Parser test | Integration test |
|-------------|-------------|-----------------|
| `uci` | ✓ | ✓ id name, id author, uciok ordering |
| `debug on/off` | ✓ | ✓ no output |
| `isready` | ✓ | ✓ readyok |
| `setoption name … value …` | ✓ (name+value, name-only, multiword, no-name) | ✓ silent |
| `register` | ✓ Unknown | — (no behaviour to test) |
| `ucinewgame` | ✓ | ✓ silent |
| `position startpos` | ✓ (no moves, moves kw only, 1 move, many moves) | ✓ silent |
| `position fen …` | ✓ (no moves, 1 move, many moves) | ✓ silent |
| `position` malformed | ✓ startpos+fen, no-args, bad-keyword | ✓ no crash |
| `go infinite` | ✓ | ✓ bestmove via stop |
| `go ponder` | ✓ | — |
| `go wtime/btime/winc/binc` | ✓ | — |
| `go movestogo` | ✓ | — |
| `go depth` | ✓ | ✓ bestmove in output |
| `go nodes` | ✓ | — |
| `go mate` | ✓ | — |
| `go movetime` | ✓ | — |
| `go searchmoves` | ✓ (with moves, empty) | — |
| `go perft` | ✓ | ✓ node counts |
| `stop` | ✓ | ✓ bestmove after infinite |
| `ponderhit` | ✓ | ✓ silent |
| `quit` | ✓ | ✓ loop exits |
| unknown / empty | ✓ | ✓ no output |
| `parse_uci_move_string` short | ✓ (3, 2, 0 bytes) | via malformed position test |
