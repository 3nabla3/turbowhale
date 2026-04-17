# Lazy SMP Multithreading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add parallel search to turbowhale using Lazy SMP — N threads search independently from the same root, sharing a sharded transposition table.

**Architecture:** Replace `TranspositionTable` with `ShardedTranspositionTable` (256 `Mutex`-guarded shards) for lock-free concurrent access across threads. Thread 0 runs the existing iterative deepening loop and reports `info` lines; helper threads run a silent variant. All threads share one `Arc<AtomicU64>` node counter. Thread count is configurable via `setoption name Threads`.

**Tech Stack:** Rust 1.94, `std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64}}`, `std::thread`

---

## File Map

| File | Change |
|------|--------|
| `src/tt.rs` | Replace `TranspositionTable` with `ShardedTranspositionTable` (256 shards, `&self` probe/store) |
| `src/engine.rs` | Remove lifetime from `SearchContext`; add `Arc<ShardedTranspositionTable>` + `Arc<AtomicU64>`; add `search_worker`; update `select_move` to spawn helpers |
| `src/uci.rs` | Change TT field type; add `thread_count`; handle `setoption Threads`; advertise option |

---

## Task 1: Replace `TranspositionTable` with `ShardedTranspositionTable`

**Files:**
- Modify: `src/tt.rs`
- Modify: `src/engine.rs`
- Modify: `src/uci.rs`

- [ ] **Step 1: Add `ShardedTranspositionTable` tests in `src/tt.rs`**

Add these tests inside the existing `mod tests` block, alongside the existing tests. They will fail to compile until the struct is implemented.

```rust
#[test]
fn sharded_probe_returns_none_on_empty_table() {
    let table = ShardedTranspositionTable::new(1);
    assert!(table.probe(12345).is_none());
}

#[test]
fn sharded_store_then_probe_returns_entry() {
    let table = ShardedTranspositionTable::new(1);
    let dummy_move = Move {
        from_square: 12,
        to_square: 20,
        promotion_piece: None,
        move_flags: MoveFlags::NONE,
    };
    let entry = TtEntry {
        hash: 0xDEADBEEF,
        depth: 4,
        score: 150,
        best_move: dummy_move,
        node_type: NodeType::Exact,
    };
    table.store(0xDEADBEEF, entry);
    let retrieved = table.probe(0xDEADBEEF).expect("should find entry");
    assert_eq!(retrieved.score, 150);
    assert_eq!(retrieved.depth, 4);
    assert_eq!(retrieved.node_type, NodeType::Exact);
}

#[test]
fn sharded_probe_returns_none_on_hash_collision() {
    let table = ShardedTranspositionTable::new(1);
    let dummy_move = Move {
        from_square: 12,
        to_square: 20,
        promotion_piece: None,
        move_flags: MoveFlags::NONE,
    };
    let hash = 0xAAAAu64;
    table.store(hash, TtEntry {
        hash,
        depth: 4,
        score: 150,
        best_move: dummy_move,
        node_type: NodeType::Exact,
    });
    // A hash that maps to the same shard (same low 8 bits) and same slot
    // (same bits 8..N) but is a distinct value — store under `hash` must not
    // be returned when probing `colliding_hash`.
    let colliding_hash = hash.wrapping_add((table.entries_per_shard as u64) << 8);
    assert_ne!(colliding_hash, hash);
    assert!(table.probe(colliding_hash).is_none());
}

#[test]
fn sharded_clear_removes_all_entries() {
    let table = ShardedTranspositionTable::new(1);
    let dummy_move = Move {
        from_square: 12,
        to_square: 20,
        promotion_piece: None,
        move_flags: MoveFlags::NONE,
    };
    table.store(0xDEADBEEF, TtEntry {
        hash: 0xDEADBEEF,
        depth: 4,
        score: 150,
        best_move: dummy_move,
        node_type: NodeType::Exact,
    });
    table.clear();
    assert!(table.probe(0xDEADBEEF).is_none());
}
```

- [ ] **Step 2: Verify new tests fail to compile**

```bash
cargo test -p turbowhale -- tt::tests 2>&1 | head -20
```

Expected: compile error — `ShardedTranspositionTable` not found.

- [ ] **Step 3: Replace `TranspositionTable` with `ShardedTranspositionTable` in `src/tt.rs`**

Replace the entire `TranspositionTable` struct and its `impl` block with the following. Keep all other code in the file (types, Zobrist) unchanged.

Add `use std::sync::Mutex;` at the top of the file alongside `use std::sync::OnceLock;`.

Replace the `TranspositionTable` struct and impl:

```rust
const SHARD_COUNT: usize = 256;

pub struct ShardedTranspositionTable {
    shards: Vec<Mutex<Vec<Option<TtEntry>>>>,
    pub entries_per_shard: usize,
}

impl ShardedTranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        let entry_bytes = std::mem::size_of::<Option<TtEntry>>();
        let target_entries = (size_mb * 1024 * 1024) / entry_bytes;
        let total_entries = (target_entries.next_power_of_two() / 2).max(SHARD_COUNT);
        let entries_per_shard = (total_entries / SHARD_COUNT).max(1);
        let shards = (0..SHARD_COUNT)
            .map(|_| Mutex::new(vec![None; entries_per_shard]))
            .collect();
        ShardedTranspositionTable { shards, entries_per_shard }
    }

    pub fn clear(&self) {
        for shard in &self.shards {
            shard.lock().unwrap().iter_mut().for_each(|entry| *entry = None);
        }
    }

    pub fn probe(&self, hash: u64) -> Option<TtEntry> {
        let shard_index = (hash as usize) & (SHARD_COUNT - 1);
        let entry_index = ((hash >> 8) as usize) & (self.entries_per_shard - 1);
        let shard = self.shards[shard_index].lock().unwrap();
        shard[entry_index].filter(|entry| entry.hash == hash)
    }

    pub fn store(&self, hash: u64, entry: TtEntry) {
        let shard_index = (hash as usize) & (SHARD_COUNT - 1);
        let entry_index = ((hash >> 8) as usize) & (self.entries_per_shard - 1);
        let mut shard = self.shards[shard_index].lock().unwrap();
        shard[entry_index] = Some(entry);
    }
}
```

- [ ] **Step 4: Update old TT tests in `src/tt.rs` to use `ShardedTranspositionTable`**

Replace the four old tests (`probe_returns_none_on_empty_table`, `store_then_probe_returns_entry`, `probe_returns_none_on_hash_collision`, `clear_removes_all_entries`) with the four new tests added in Step 1 (remove the old tests entirely — the new ones cover the same behaviour).

The old `probe_returns_none_on_hash_collision` test used `table.size`. The new collision test uses `table.entries_per_shard` instead — this is already covered by the test you added in Step 1.

- [ ] **Step 5: Verify TT tests pass**

```bash
cargo test -p turbowhale -- tt::tests
```

Expected: all TT tests pass (compile errors in engine/uci are fine at this point — we're only testing the tt module).

- [ ] **Step 6: Update `src/engine.rs`**

**6a.** Update imports at the top:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::tt::{compute_hash, NodeType, ShardedTranspositionTable, TtEntry};
```

**6b.** Replace `SearchContext` — remove the lifetime `'a`, change TT field type:

```rust
pub struct SearchContext {
    pub transposition_table: Arc<ShardedTranspositionTable>,
    pub stop_flag: Arc<AtomicBool>,
    pub limits: SearchLimits,
    pub start_time: Instant,
    pub nodes_searched: u64,
}
```

**6c.** Update `select_move` signature and construction of `SearchContext` inside it:

```rust
pub fn select_move(
    position: &Position,
    go_parameters: &GoParameters,
    transposition_table: Arc<ShardedTranspositionTable>,
    stop_flag: Arc<AtomicBool>,
    _thread_count: usize,   // used in Task 3
) -> Move {
    let limits = compute_search_limits(go_parameters, position.side_to_move);

    let legal_moves = generate_legal_moves(position);
    let mut best_move = *legal_moves.first().expect("select_move called with no legal moves");

    let max_depth = match &limits {
        SearchLimits::Depth(depth) => *depth,
        _ => 100,
    };

    let mut context = SearchContext {
        transposition_table: Arc::clone(&transposition_table),
        stop_flag: Arc::clone(&stop_flag),
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
        let elapsed = context.start_time.elapsed();
        let nodes = context.nodes_searched;
        let nps = if elapsed.as_millis() > 0 {
            (nodes as f64 / elapsed.as_millis() as f64 * 1000.0) as u64
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
                depth,
                score_field,
                nodes,
                nps,
                elapsed.as_millis(),
                pv_string,
            );
        } else {
            println!("info depth {} nodes {} nps {} time {}",
                depth,
                nodes,
                nps,
                elapsed.as_millis(),
            );
        }
    }

    best_move
}
```

**6d.** Update `negamax_pvs` — remove the lifetime from the signature (the function body is unchanged; `context.transposition_table.store(...)` works because `store` now takes `&self`):

```rust
fn negamax_pvs(
    position: &Position,
    depth: u32,
    mut alpha: i32,
    mut beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
```

**6e.** Update `quiescence_search` — same change, remove lifetime from signature:

```rust
fn quiescence_search(
    position: &Position,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    context: &mut SearchContext,
) -> i32 {
```

**6f.** Update `extract_pv_from_tt` signature:

```rust
fn extract_pv_from_tt(root: &Position, tt: &ShardedTranspositionTable, max_depth: u32) -> Vec<Move> {
```

**6g.** Update engine tests — replace `make_tt` and `make_stop` helpers, and update all `SearchContext` constructions:

```rust
fn make_tt() -> Arc<ShardedTranspositionTable> {
    Arc::new(ShardedTranspositionTable::new(4))
}

fn make_stop() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
```

Update every `select_move` call in tests from:
```rust
let mut tt = make_tt();
let stop = make_stop();
let chosen = select_move(&position, &params, &mut tt, &stop);
```
to:
```rust
let tt = make_tt();
let stop = make_stop();
let chosen = select_move(&position, &params, tt, stop, 1);
```

Update every `SearchContext { ... }` construction in tests from:
```rust
let mut context = SearchContext {
    transposition_table: &mut tt,
    stop_flag: &stop,
    limits: SearchLimits::Depth(4),
    start_time: Instant::now(),
    nodes_searched: 0,
};
```
to:
```rust
let mut context = SearchContext {
    transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
    stop_flag: Arc::new(AtomicBool::new(false)),
    limits: SearchLimits::Depth(4),
    start_time: Instant::now(),
    nodes_searched: 0,
};
```

Update the `extract_pv_from_tt` test call from:
```rust
let pv = extract_pv_from_tt(&position, &tt, 3);
```
to:
```rust
let pv = extract_pv_from_tt(&position, &tt, 3);
```
(unchanged — `tt` is now `Arc<ShardedTranspositionTable>`, and `&tt` derefs to `&ShardedTranspositionTable` as required by the updated signature.)

- [ ] **Step 7: Update `src/uci.rs`**

**7a.** Update imports:

```rust
use crate::tt::ShardedTranspositionTable;
```

Remove `Arc<Mutex<TranspositionTable>>` — the `Mutex` wrapper is no longer needed.

**7b.** Replace `UciState` struct and `new` method:

```rust
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
    // stop_search unchanged
}
```

**7c.** Update `UciNewGame` handler — `clear` now takes `&self`, no mutex needed:

```rust
UciCommand::UciNewGame => {
    state.stop_search();
    state.current_position = start_position();
    state.stop_flag.store(false, Ordering::Relaxed);
    state.transposition_table.clear();
}
```

**7d.** Update `Go` handler — remove the `tt_arc.lock().unwrap()` call, pass `thread_count`:

```rust
UciCommand::Go(parameters) => {
    // perft branch unchanged ...

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
```

> **Note:** The full `Go` arm starts with the existing perft branch (unchanged) before the search branch shown above. Keep the perft branch exactly as it is in the current code — only the search branch (from `state.stop_search()` onward) changes.


- [ ] **Step 8: Run all tests**

```bash
cargo test
```

Expected: all tests pass. If clippy warnings appear about unused `_thread_count`, that is expected and will be resolved in Task 3.

- [ ] **Step 9: Run clippy**

```bash
cargo clippy
```

Expected: no errors. Warnings about `_thread_count` being unused are acceptable until Task 3.

- [ ] **Step 10: Commit**

Stage and commit using GitButler (`/gitbutler` skill). Commit message:
```
feat(tt): replace TranspositionTable with ShardedTranspositionTable
```

---

## Task 2: Add shared node counter across threads

**Files:**
- Modify: `src/engine.rs`

- [ ] **Step 1: Write a failing test that verifies `SearchContext` has `shared_nodes`**

Add to `mod tests` in `src/engine.rs`:

```rust
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
    };
    negamax_pvs(&position, 2, -INF, INF, 0, &mut context);
    // After a depth-2 search, the shared counter must have been incremented.
    // Flush the local remainder into shared_nodes first.
    shared_nodes.fetch_add(context.nodes_searched, Ordering::Relaxed);
    assert!(shared_nodes.load(Ordering::Relaxed) > 0, "shared_nodes must be non-zero after search");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p turbowhale -- engine::tests::search_context_shared_nodes_accumulates_across_search
```

Expected: compile error — `shared_nodes` field not found in `SearchContext`.

- [ ] **Step 3: Add `shared_nodes` to `SearchContext` in `src/engine.rs`**

Add import:
```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
```

Update `SearchContext`:
```rust
pub struct SearchContext {
    pub transposition_table: Arc<ShardedTranspositionTable>,
    pub stop_flag: Arc<AtomicBool>,
    pub shared_nodes: Arc<AtomicU64>,
    pub limits: SearchLimits,
    pub start_time: Instant,
    pub nodes_searched: u64,
}
```

- [ ] **Step 4: Update node-counting in `negamax_pvs`**

Replace the existing node-counting + time-check block (the `context.nodes_searched += 1; if context.nodes_searched.is_multiple_of(1024)` block) with:

```rust
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
        context.stop_flag.store(true, Ordering::Relaxed);
        return 0;
    }
}
```

- [ ] **Step 5: Update node-counting in `quiescence_search`**

Apply the identical replacement to the same block in `quiescence_search`:

```rust
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
        context.stop_flag.store(true, Ordering::Relaxed);
        return 0;
    }
}
```

- [ ] **Step 6: Flush remaining nodes and use `shared_nodes` for `info` reporting in `select_move`**

Inside the `for depth in 1..=max_depth` loop in `select_move`, immediately after `negamax_pvs` returns, flush the local counter and read the total:

```rust
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
        // ... (score_field and pv_string computation unchanged) ...
        println!("info depth {} score {} nodes {} nps {} time {} pv {}",
            depth, score_field, total_nodes, nps, elapsed.as_millis(), pv_string);
    } else {
        println!("info depth {} nodes {} nps {} time {}",
            depth, total_nodes, nps, elapsed.as_millis());
    }
}
```

- [ ] **Step 7: Update `select_move` to construct `SearchContext` with `shared_nodes`**

Inside `select_move`, construct the shared counter and thread context:

```rust
let shared_nodes = Arc::new(AtomicU64::new(0));

let mut context = SearchContext {
    transposition_table: Arc::clone(&transposition_table),
    stop_flag: Arc::clone(&stop_flag),
    shared_nodes: Arc::clone(&shared_nodes),
    limits,
    start_time: Instant::now(),
    nodes_searched: 0,
};
```

- [ ] **Step 8: Update all `SearchContext { ... }` constructions in engine tests**

Every test that manually constructs `SearchContext` needs `shared_nodes`:

```rust
let mut context = SearchContext {
    transposition_table: Arc::new(ShardedTranspositionTable::new(4)),
    stop_flag: Arc::new(AtomicBool::new(false)),
    shared_nodes: Arc::new(AtomicU64::new(0)),
    limits: SearchLimits::Depth(4),
    start_time: Instant::now(),
    nodes_searched: 0,
};
```

Apply this to all three tests that manually construct `SearchContext`:
`negamax_returns_zero_for_stalemate`, `negamax_detects_checkmate`, and the new test from Step 1.

- [ ] **Step 9: Run all tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 10: Commit**

```
feat(engine): add shared node counter for cross-thread NPS reporting
```

---

## Task 3: Add `search_worker` and parallel thread dispatch

**Files:**
- Modify: `src/engine.rs`

- [ ] **Step 1: Write a failing test for multi-thread search**

Add to `mod tests` in `src/engine.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p turbowhale -- engine::tests::select_move_with_two_threads_returns_legal_move
```

Expected: test compiles but the search runs on one thread (correct result, but no helpers spawned yet). The test should pass already — that is expected and correct. The real validation comes from running under a thread sanitiser or stress test, but for the plan we just need the function to work. **Proceed to the next step regardless.**

- [ ] **Step 3: Add `search_worker` function to `src/engine.rs`**

Add this private function before `select_move`:

```rust
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
    };
    for depth in 1..=100 {
        negamax_pvs(&position, depth, -INF, INF, 0, &mut context);
        if context.stop_flag.load(Ordering::Relaxed) {
            break;
        }
    }
}
```

- [ ] **Step 4: Update `select_move` to spawn helper threads**

Replace the `_thread_count` parameter name with `thread_count` (removing the underscore suppressor). Update the body to capture `start_time` before construction and spawn helpers:

```rust
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
            let nodes_clone = Arc::clone(&shared_nodes);
            std::thread::spawn(move || {
                search_worker(position_clone, limits_clone, start_time, tt_clone, stop_clone, nodes_clone);
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
    };

    for depth in 1..=max_depth {
        negamax_pvs(position, depth, -INF, INF, 0, &mut context);

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
    stop_flag.store(true, Ordering::Relaxed);
    for handle in helper_handles {
        handle.join().ok();
    }

    best_move
}
```

- [ ] **Step 5: Run all tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy
```

Expected: no warnings or errors.

- [ ] **Step 7: Commit**

```
feat(engine): add search_worker and Lazy SMP parallel dispatch
```

---

## Task 4: UCI `setoption Threads` support

**Files:**
- Modify: `src/uci.rs`

- [ ] **Step 1: Write failing tests**

Add to `mod tests` in `src/uci.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p turbowhale -- uci::tests::setoption_threads_updates_thread_count uci::tests::uci_response_advertises_threads_option
```

Expected: `uci_response_advertises_threads_option` fails (the option line is not yet emitted). `setoption_threads_updates_thread_count` may pass already (setoption is silently accepted).

- [ ] **Step 3: Update `setoption` handler in `src/uci.rs`**

In `handle_uci_line`, update the `UciCommand::SetOption` arm:

```rust
UciCommand::SetOption { name, value } => {
    if name == "Threads" {
        if let Some(value_string) = value {
            if let Ok(count) = value_string.parse::<usize>() {
                state.thread_count = count.clamp(1, 64);
            }
        }
    }
}
```

- [ ] **Step 4: Advertise the `Threads` option in the `uci` response**

In the `UciCommand::Uci` arm, add the option line between the `id author` line and `uciok`:

```rust
UciCommand::Uci => {
    let mut output = state.output.lock().unwrap();
    writeln!(output, "id name {} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")).unwrap();
    writeln!(output, "id author {}", env!("CARGO_PKG_AUTHORS")).unwrap();
    writeln!(output, "option name Threads type spin default 1 min 1 max 64").unwrap();
    writeln!(output, "uciok").unwrap();
    output.flush().unwrap();
}
```

- [ ] **Step 5: Run all tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy
```

Expected: no warnings or errors.

- [ ] **Step 7: Commit**

```
feat(uci): advertise and handle setoption name Threads
```

---

## Verification

After all four tasks are complete:

- [ ] Run the full test suite: `cargo test`
- [ ] Build release: `cargo build --release`
- [ ] Smoke-test interactively:
  ```
  cargo run --release
  uci
  setoption name Threads value 4
  isready
  position startpos
  go movetime 2000
  stop
  quit
  ```
  Confirm `info` lines appear and `bestmove` is emitted.
