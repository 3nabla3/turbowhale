# Chess Engine Design Spec

**Date:** 2026-04-12
**Status:** Approved

## Overview

A UCI-compatible chess engine written in Rust that plays random legal moves. The primary goal of this initial version is to establish correct UCI communication, a bitboard-based board representation, and full legal move generation — instrumented with OpenTelemetry from the start so future optimization work has observability built in.

---

## Module Structure

```
src/
  main.rs        — initialize telemetry, load .env, start UCI loop, shutdown telemetry on quit
  board.rs       — bitboard position representation and move application
  movegen.rs     — legal move generation using bitboard techniques
  uci.rs         — UCI protocol parser and responder
  engine.rs      — move selection (random for now)
  telemetry.rs   — OpenTelemetry initialization and shutdown
.env             — OTEL_BACKEND_URL=http://localhost:4317
```

Each module's key functions are annotated with `#[instrument]` from the `tracing` crate, which automatically creates spans sent to the configured OTLP backend.

---

## Board Representation (`board.rs`)

### `Position` struct

12 bitboards (`u64` each), one per piece type per color:

- `white_pawns`, `white_knights`, `white_bishops`, `white_rooks`, `white_queens`, `white_king`
- `black_pawns`, `black_knights`, `black_bishops`, `black_rooks`, `black_queens`, `black_king`

Derived occupancy bitboards (computed from the above):

- `white_occupancy`, `black_occupancy`, `all_occupancy`

Position state fields:

- `side_to_move: Color`
- `castling_rights: u8` — 4 bits: white kingside, white queenside, black kingside, black queenside
- `en_passant_square: Option<u8>` — target square index, if any
- `halfmove_clock: u32`
- `fullmove_number: u32`

### `Move` struct

- `from_square: u8`
- `to_square: u8`
- `promotion_piece: Option<PieceType>`
- `move_flags: MoveFlags` — bitflags for castling, en passant, capture

### Key functions

- `#[instrument] fn from_fen(fen: &str) -> Position` — parse a FEN string into a `Position`
- `#[instrument] fn make_move(position: &mut Position, chess_move: Move)` — apply a move, updating all bitboards and state fields

---

## Move Generation (`movegen.rs`)

### Approach: pseudo-legal generation + legality filter

**Stage 1 — Pseudo-legal generation:**

Pre-computed attack tables (static arrays, initialized at startup):

- `KNIGHT_ATTACKS: [u64; 64]`
- `KING_ATTACKS: [u64; 64]`
- `PAWN_ATTACKS: [[u64; 64]; 2]` — indexed by `[color][square]`

Sliding pieces (rooks, bishops, queens) use **hyperbola quintessence**: a bitboard technique using `o^(o-2r)` to compute attacks along a ray through occupied squares. Simple to implement, correct, and replaceable with magic bitboards in a later optimization pass.

Special moves generated: castling (with path-clear checks), en passant, pawn promotion (all four promotion pieces).

**Stage 2 — Legality filter:**

Each pseudo-legal move is tested by calling `make_move` then checking whether the moving side's king is attacked. Moves that leave the king in check are discarded.

### Key function

- `#[instrument] fn generate_legal_moves(position: &Position) -> Vec<Move>`

---

## UCI Protocol (`uci.rs`)

Reads from `stdin` line by line in a loop. Parses each command and dispatches to a handler.

### Commands handled

**GUI → Engine:**

| Command | Response / Action |
|---|---|
| `uci` | Print `id name chess-engine`, `id author <author>`, `uciok` |
| `debug on\|off` | Toggle internal debug flag |
| `isready` | Print `readyok` |
| `setoption name <x> value <y>` | Store option; ignored for now |
| `ucinewgame` | Reset position to start position |
| `position startpos [moves ...]` | Build position from start, replay moves |
| `position fen <fen> [moves ...]` | Build position from FEN, replay moves |
| `go [ponder] [wtime x] [btime x] [winc x] [binc x] [movestogo x] [depth x] [nodes x] [mate x] [movetime x] [infinite] [searchmoves ...]` | Call `engine::select_move`, print `bestmove <move>` |
| `stop` | No-op for random mover |
| `ponderhit` | No-op for now |
| `quit` | Flush telemetry, exit process |

**Engine → GUI:**
- `bestmove <move>` in long algebraic notation (e.g. `e2e4`, `e7e8q`)
- No ponder move until pondering is implemented
- Minimal `info` output stubbed (can be extended later)

Move notation: long algebraic — `e2e4`, `e7e8q` for promotions.

### Key function

- `#[instrument] fn run_uci_loop()`

---

## Engine (`engine.rs`)

Single function that picks a random move from the list of legal moves using the `rand` crate.

- `#[instrument] fn select_move(position: &Position, legal_moves: Vec<Move>) -> Move`

---

## Telemetry (`telemetry.rs`)

Initializes the tracing stack at startup:

1. Load `.env` with `dotenvy`; read `OTEL_BACKEND_URL` (default: `http://localhost:4317`)
2. Build an OTLP gRPC exporter pointing at that URL
3. Set up an `opentelemetry_sdk::TracerProvider` with a batch span processor
4. Install a `tracing_subscriber` registry with the `OpenTelemetryLayer` bridge
5. Expose `shutdown()` to flush and export remaining spans before exit

Functions:
- `fn init()` — called once at the top of `main`
- `fn shutdown()` — called before process exit

---

## Dependencies

```toml
[dependencies]
rand = "0.9"
dotenvy = "0.15"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-opentelemetry = "0.29"
opentelemetry = "0.27"
opentelemetry-otlp = { version = "0.27", features = ["grpc-tonic"] }
opentelemetry_sdk = { version = "0.27", features = ["rt-tokio"] }
tokio = { version = "1", features = ["full"] }
```

> Note: the OTLP exporter uses tonic (gRPC) which requires tokio. The UCI loop itself is synchronous but runs inside a tokio runtime.

---

## Key Design Decisions

- **Hyperbola quintessence over magic bitboards** for sliding piece attacks: simpler to implement correctly, easy to swap out for magic bitboards in a targeted optimization pass later (the instrumentation will show if this is the bottleneck).
- **Pseudo-legal + filter** for move generation: straightforward to implement correctly; can be replaced with fully legal generation using pin/check masks if profiling shows it matters.
- **`tracing` + `#[instrument]`** as the instrumentation layer rather than direct OTel calls: keeps instrumentation non-invasive and makes it trivial to add spans to new functions.
- **Random move selection**: placeholder only. The `engine::select_move` interface is designed so search can be dropped in later without changing callers.
