# Chess Engine: PVS + Transposition Table Design

**Date:** 2026-04-13  
**Scope:** Replace random-move engine with NegaMax/PVS search, transposition table, quiescence search, and iterative deepening.  
**Approach:** Lean & correct (Approach A) — material-only eval, clean architecture extensible to piece-square tables and Lazy SMP later.

---

## 1. Module Structure

Four files are touched:

| File | Role |
|---|---|
| `src/tt.rs` | New — Zobrist hashing, transposition table, entry types |
| `src/eval.rs` | New — static evaluation (material counting) |
| `src/engine.rs` | Rewritten — PVS search, iterative deepening, time management |
| `src/uci.rs` | Modified — 2-thread architecture, pass TT and limits into search |

`src/main.rs` registers the two new modules (`mod tt; mod eval;`).

---

## 2. Transposition Table (`src/tt.rs`)

### Zobrist Hashing

At startup, generate a fixed set of random `u64` values (seeded deterministically for reproducibility):

- 12 × 64 values: one per (piece type × color, square) combination
- 1 value: side to move is Black
- 16 values: castling rights (one per bitmask value 0–15)
- 8 values: en passant file (one per file a–h; only used when en passant is available)

The hash for a position is the XOR of all applicable values. Computed fresh from `Position` on each call; incremental updates are a future performance optimization.

### Table Structure

```rust
pub struct TranspositionTable {
    entries: Vec<Option<TtEntry>>,
    size: usize,  // always a power of two
}

pub struct TtEntry {
    pub hash: u64,       // full hash — used to detect collisions
    pub depth: u8,       // depth at which this position was searched
    pub score: i32,      // score in centipawns
    pub best_move: Move, // best move found from this position
    pub node_type: NodeType,
}

pub enum NodeType {
    Exact,       // score is exact (PV node)
    LowerBound,  // score >= beta (we caused a cutoff; actual score may be higher)
    UpperBound,  // score <= alpha (all moves were worse; actual score may be lower)
}
```

Default size: 16 MB (~1M entries). Indexed by `hash % size`. Replacement strategy: always replace (simplest; revisit if needed).

### Using TT Entries in Search

At the top of each search node:
- Look up `hash % size`
- Verify `entry.hash == position_hash` (collision check)
- If `entry.depth >= current_depth`:
  - `Exact` → return `entry.score` directly
  - `LowerBound` → raise alpha: `alpha = max(alpha, entry.score)`
  - `UpperBound` → lower beta: `beta = min(beta, entry.score)`
  - If after adjustment `alpha >= beta`, return `entry.score` (cutoff)
- Always use `entry.best_move` for move ordering, even if depth is insufficient for score reuse.

---

## 3. Search Algorithm (`src/engine.rs`)

### SearchContext

Passed by mutable reference through every recursive call. Designed so that adding Lazy SMP later only requires wrapping the stop flag and TT in `Arc<>`:

```rust
struct SearchContext<'a> {
    transposition_table: &'a mut TranspositionTable,
    stop_flag: &'a AtomicBool,   // shared with UCI I/O thread
    limits: SearchLimits,
    start_time: Instant,
    nodes_searched: u64,
}

enum SearchLimits {
    Depth(u32),
    MoveTime(Duration),
    Infinite,
    Clock { budget: Duration },  // computed from wtime/btime/winc/binc
}
```

### `select_move` (public entry point)

Called by the UCI search thread. Runs iterative deepening:

```
best_move = first legal move  // fallback in case depth-1 finishes but nothing stored
for depth in 1..=max_depth:
    score = negamax_pvs(position, depth, -INF, +INF, ply=0, &mut context)
    if stop_flag set and depth > 1:
        break  // incomplete iteration — discard, keep previous best
    best_move = TT entry at root (best_move field)
return best_move
```

The root position's TT entry always holds the best move from the most recently completed iteration.

### `negamax_pvs`

```
fn negamax_pvs(position, depth, alpha, beta, ply, context) -> i32:

1. Increment nodes_searched. Every 1024 nodes check stop_flag and time budget;
   if limit exceeded, set stop_flag = true and return 0 (result discarded by caller).

2. Check 50-move rule (halfmove_clock >= 100) → return 0  (cheap early exit before TT)

3. TT lookup — use score/move as described in Section 2

4. Generate legal moves
   - If empty and in check  → return -MATE_SCORE + ply  (mated; + ply prefers faster mates)
   - If empty and not in check → return 0  (stalemate)
   (Must happen before depth==0 check so checkmate/stalemate is detected correctly at leaf nodes)

5. If depth == 0 → return quiescence_search(position, alpha, beta, context)

6. Order moves:
   a. TT best move (if any) — first
   b. Captures — ordered by MVV-LVA score
   c. Quiet moves — unordered

7. For each move (first move gets full window; rest get PVS):
   First move:
     score = -negamax_pvs(child, depth-1, -beta, -alpha, ply+1, context)

   Remaining moves (PVS — null-window first, re-search only if it raises alpha):
     score = -negamax_pvs(child, depth-1, -alpha-1, -alpha, ply+1, context)
     if score > alpha and score < beta and beta - alpha > 1:
         score = -negamax_pvs(child, depth-1, -beta, -alpha, ply+1, context)
     (beta - alpha > 1 guard: skip re-search when window is already null — CPW)

   After each move (fail-hard — check score against beta BEFORE updating alpha):
     if score >= beta:
         store TT entry: score=beta, flag=LowerBound, best_move=current_move, depth=depth
         return beta   ← fail-hard cutoff
     if score > alpha:
         alpha = score
         best_move = current_move   ← track for TT storage at step 8

8. Store in TT (only reached when no beta cutoff occurred):
    - alpha improved over original_alpha → flag=Exact, score=alpha, best_move=best_move
    - alpha never improved → flag=UpperBound, score=alpha, best_move=best_move (may be null/first move)
    (LowerBound case is handled by the early return in step 7; never reached here)

9. Return alpha
```

**Score constants:**
```rust
const MATE_SCORE: i32 = 100_000;
const INF: i32 = 200_000;
```

Scores outside `±50_000` are treated as mate scores by the UCI output layer.

### `quiescence_search`

Prevents the horizon effect by continuing to search captures after depth 0:

```
fn quiescence_search(position, alpha, beta, context) -> i32:

1. Increment nodes_searched; check stop_flag on the same 1024-node cadence as negamax_pvs.

2. Check if side to move is in check (is_square_attacked on king square).

3. If NOT in check — stand-pat:
     score = evaluate(position)
     if score >= beta → return beta
     if score > alpha: alpha = score
   If IN CHECK — skip stand-pat entirely; must search all evasions.
   (CPW: stand-pat is invalid when in check — the null-move assumption doesn't hold)

4. Generate moves:
   - If NOT in check: generate captures only (CAPTURE flag), order by MVV-LVA
   - If IN CHECK: generate all legal evasions (captures + quiet moves), order captures first

5. For each move:
   score = -quiescence_search(child, -beta, -alpha, context)
   if score >= beta → return beta
   if score > alpha: alpha = score

6. If in check and no moves found → return -MATE_SCORE + ply  (checkmate in quiescence)

7. Return alpha
```

Quiescence search does not use the TT (adds complexity for marginal gain at this stage).

### Time Management

Budget formula for clock-based search:
```
budget = remaining_time / 30 + increment / 2
```
This is a simple, robust formula that works well across typical time controls.

Stop check: `context.nodes_searched % 1024 == 0` → check `start_time.elapsed() >= budget` → set stop flag.

---

## 4. Threading & UCI Architecture (`src/uci.rs`)

### Two Threads

```
UCI I/O thread (main thread)
  ├─ reads stdin line by line (blocking)
  ├─ on "ucinewgame": clears TT via Arc<Mutex<TranspositionTable>>
  ├─ on "go": clears stop_flag, spawns search thread, stores JoinHandle
  ├─ on "stop": sets stop_flag = true, joins search thread
  └─ on "quit": sets stop_flag, joins search thread, exits

Search thread (spawned per "go" command)
  ├─ locks TT mutex for full search duration (no contention during search)
  ├─ runs select_move with position, limits, stop_flag, TT
  ├─ when done (stop or limits reached): prints "bestmove <move>" to stdout
  └─ releases TT mutex and exits
```

### Shared State

```rust
Arc<AtomicBool>              // stop_flag — written by UCI thread, read by search thread
Arc<Mutex<TranspositionTable>>  // TT — cleared by UCI thread, used exclusively by search thread during search
```

**Stop latency:** sub-millisecond. The search checks the atomic flag every 1024 nodes; at typical node rates (~1M nodes/sec) this is a check every ~1ms.

**Lazy SMP upgrade path:** to add N search threads, clone the `Arc<AtomicBool>` and `Arc<Mutex<TranspositionTable>>` into N threads each calling `select_move`. The `SearchContext` struct requires no changes — just replace `&AtomicBool` with `Arc<AtomicBool>`.

---

## 5. Static Evaluation (`src/eval.rs`)

```rust
pub fn evaluate(position: &Position) -> i32
```

Returns score in centipawns from the perspective of `position.side_to_move`. NegaMax uses this directly with no sign flip.

### Piece Values

```
Pawn   = 100
Knight = 320
Bishop = 330
Rook   = 500
Queen  = 900
King   = 0  (never captured; not counted)
```

Score = (own material total) − (opponent material total).

### Extensibility

The function signature never changes. Future improvements are purely additive inside `eval.rs`:

- **Piece-square tables**: add 6 × 64 lookup tables, sum during material count
- **Mobility**: call existing `movegen` attack functions, count reachable squares  
- **Pawn structure**: bitboard analysis of isolated/doubled/passed pawns
- **Tapered eval**: blend middlegame/endgame weights by remaining material
- **NNUE**: drop-in replacement behind the same interface

Incremental evaluation (maintaining a running score through `apply_move`) is a future performance optimization only — not required for correctness.

---

## 6. MVV-LVA Move Ordering

Captures are ordered by Most Valuable Victim / Least Valuable Attacker:

```
mvv_lva_score = victim_value * 10 - attacker_value
```

Piece values for ordering (same as eval): P=100, N=320, B=330, R=500, Q=900.

Multiplying victim by 10 ensures victim identity dominates — PxQ (9000-100=8900) scores far above QxQ (9000-900=8100), which scores far above QxP (1000-900=100). Winning captures are searched first, maximizing alpha-beta cutoffs.

**Note on operator precedence:** the formula must be `(victim_value * 10) - attacker_value`, not `victim_value - (attacker_value / 10)`, which would produce incorrect ordering due to integer division.

---

## 7. Performance Notes

**Move list allocation:** use `Vec::with_capacity(48)` (typical max ~45 legal moves) to avoid reallocations in the hot path. Avoids heap churn in the deepest nodes where move generation happens most.

**Branch prediction:** CPW confirms >90% of cut-nodes fail-high on the first move. This means the beta-cutoff branch in the move loop is almost always taken early — the CPU branch predictor will learn this quickly. The main implication is: **move ordering quality matters more than any micro-optimization**. Getting the TT best move first is the single most impactful thing.

**TT probe cost:** the TT lookup involves one array index + one full hash comparison. Keep the `TtEntry` size small (fits in a cache line = 64 bytes) so lookups stay in L1/L2 cache. Our current entry is ~32 bytes — well within budget.

**Avoid legal move generation at every node:** `generate_legal_moves` calls `apply_move` for every pseudo-legal move to filter check — expensive. For quiescence (captures-only), use `generate_pseudo_legal_moves` filtered to captures, then validate each with `apply_move` individually. For the main search, full legal generation is unavoidable but worth noting as a future optimization target (pin detection, check evasion generators).

---

## 8. Out of Scope

The following are explicitly deferred:

- Lazy SMP / multi-threaded search
- Piece-square tables
- Killer move heuristic
- History heuristic
- Null move pruning / LMR / futility pruning
- Opening book
- Endgame tablebases
- Incremental Zobrist hashing
- Incremental evaluation
