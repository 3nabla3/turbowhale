# Multithreading Design — Lazy SMP

**Date:** 2026-04-17
**Status:** Approved

## Overview

Add parallel search to turbowhale using Lazy SMP (Shared Memory Parallel search). All threads search the same root position independently using iterative deepening. They share a single transposition table so they benefit from each other's work even when searching overlapping nodes. Thread 0 is the "main" thread — it prints `info` lines and determines the best move. Helper threads search silently.

Thread count is configurable via the UCI `setoption name Threads value N` command, defaulting to 1 (single-threaded, identical to current behaviour).

## Architecture

### 1. `ShardedTranspositionTable` (replaces `TranspositionTable`)

**Location:** `src/tt.rs`

`TranspositionTable` is replaced by `ShardedTranspositionTable`. The structure holds 256 fixed shards, each a `Mutex<Vec<Option<TtEntry>>>`.

**Sizing:**
- Total entry count computed from `size_mb` exactly as today (power-of-two, rounded down).
- Divided evenly across 256 shards → `entries_per_shard = total_entries / 256`.
- Both `shard_count` (256) and `entries_per_shard` are powers of two — all indexing uses bitwise masking.

**Indexing:**
```
shard_index     = hash & 0xFF                           (low 8 bits)
slot_in_shard   = (hash >> 8) & (entries_per_shard - 1)
```

**API:**
- `probe(&self, hash: u64) -> Option<TtEntry>` — locks one shard, returns matching entry.
- `store(&self, hash: u64, entry: TtEntry)` — locks one shard, writes entry (always-replace).
- `clear(&self)` — locks all shards sequentially, zeroes them. Only called on `ucinewgame`.

Both `probe` and `store` take `&self` (interior mutability via `Mutex`). No exclusive borrow of the table is ever needed.

**Thread safety:** Two threads writing to different shards never block each other. With 256 shards and millions of entries, simultaneous shard contention is rare in practice.

### 2. `SearchContext` changes

**Location:** `src/engine.rs`

```rust
pub struct SearchContext {
    pub transposition_table: Arc<ShardedTranspositionTable>,
    pub stop_flag: Arc<AtomicBool>,
    pub shared_nodes: Arc<AtomicU64>,   // new — accumulates nodes across all threads
    pub limits: SearchLimits,
    pub start_time: Instant,
    pub nodes_searched: u64,            // per-thread local counter
}
```

The lifetime parameter `'a` is removed entirely — `Arc` ownership replaces the borrow.

**Node counting:** Each thread increments its local `nodes_searched` cheaply. Every 1024 nodes (same cadence as the time check), the thread flushes its local delta into `shared_nodes` via `fetch_add(Ordering::Relaxed)` and resets the local counter. The main thread reads `shared_nodes` when printing `info` lines, so reported NPS reflects all threads.

### 3. `select_move` and `search_worker`

**Location:** `src/engine.rs`

`select_move` gains a `num_threads: usize` parameter. Before starting its iterative deepening loop, it spawns `num_threads - 1` helper threads. Each helper runs a new private `search_worker` function:

```rust
fn search_worker(
    position: Position,
    limits: SearchLimits,
    transposition_table: Arc<ShardedTranspositionTable>,
    stop_flag: Arc<AtomicBool>,
    shared_nodes: Arc<AtomicU64>,
)
```

`search_worker` runs the same iterative deepening loop as `select_move` but does not print `info` lines and does not extract a best move — it simply searches until the stop flag is set.

When `num_threads = 1`, no helper threads are spawned and no `Arc<AtomicU64>` is allocated — behaviour is identical to today with zero overhead.

After the main thread's iterative deepening loop exits (time up, depth reached, or stop flag), it:
1. Sets `stop_flag` to `true` so helpers terminate promptly.
2. Joins all helper thread handles.
3. Returns the best move (from TT as today).

All helpers share the same `Arc<ShardedTranspositionTable>`, `Arc<AtomicBool>`, and `Arc<AtomicU64>`.

### 4. UCI changes

**Location:** `src/uci.rs`

**`UciState` gains:**
```rust
num_threads: usize,  // default: 1
```

**`setoption` handler** — recognises `name = "Threads"`, parses the value as `usize`, clamps to `1..=64`, stores in `state.num_threads`.

**`transposition_table` field type** changes from `Arc<Mutex<TranspositionTable>>` to `Arc<ShardedTranspositionTable>`. The search thread no longer needs to acquire a mutex before calling `select_move` — sharding provides the interior mutability directly.

**`go` handler** passes `state.num_threads` to `select_move`.

**`uci` response** advertises the Threads option to GUIs:
```
id name turbowhale v...
id author ...
option name Threads type spin default 1 min 1 max 64
uciok
```

## Data Flow

```
UCI "go" command
  → uci.rs spawns one OS thread (as today)
    → select_move(position, params, Arc<ShardedTT>, Arc<AtomicBool>, num_threads)
      → spawns num_threads-1 helper OS threads → search_worker(...)
      → main thread: iterative deepening + info printing
      → helpers: iterative deepening (silent)
      → all threads: probe/store ShardedTT, increment shared_nodes
      → time expires → stop_flag = true
      → helpers join
      → returns best move from TT
  → uci.rs prints "bestmove ..."
```

## What Does Not Change

- Move generation, board representation, evaluation — untouched.
- Iterative deepening and PVS logic — identical in both main thread and helpers.
- Stop flag mechanism — already `Arc<AtomicBool>`, works unchanged.
- `perft` — single-threaded, unaffected.
- `ucinewgame` — calls `tt.clear()`, which still works (sequential shard locks).

## Testing Strategy

- All existing engine tests continue to pass (they create a `ShardedTranspositionTable` with 1-shard equivalent and call `select_move` with `num_threads = 1`).
- Add a test: `select_move` with `num_threads = 2` on a known position returns a legal move and does not deadlock.
- Add a test: `setoption name Threads value 4` updates `UciState::num_threads` to 4.
- Manual verification: run `go depth 10` with 1 thread vs 4 threads — 4-thread version should reach the same or greater depth within the same wall time.
