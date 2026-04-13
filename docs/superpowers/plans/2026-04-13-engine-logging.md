# Engine Search Logging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace ad-hoc `eprintln!` debug calls with clean UCI `info` lines that include depth, score, node count, nps, time, and the full principal variation extracted from the transposition table.

**Architecture:** Remove all 8 `eprintln!` debug calls from `engine.rs`. Add a pure `extract_pv_from_tt` function that walks the TT from the root position up to `max_depth` steps, collecting moves. After each completed depth iteration in `select_move`, call it and emit a properly formatted UCI `info` line.

**Tech Stack:** Rust, `std::collections::HashSet` (for cycle detection in PV walk), existing `crate::tt::{compute_hash, TranspositionTable}`, `crate::board::{apply_move, Position}`, `crate::uci::move_to_uci_string`.

---

### Task 1: Remove all `eprintln!` debug calls

**Files:**
- Modify: `src/engine.rs`

- [ ] **Step 1: Remove the 8 `eprintln!` calls**

In `src/engine.rs`, remove these lines (do NOT remove surrounding logic, only the `eprintln!` lines):

```
eprintln!("Initial stop_flag = {}", context.stop_flag.load(Ordering::Relaxed));
eprintln!("Starting depth {}", depth);
eprintln!("Completed depth {}, stop_flag = {}", depth, context.stop_flag.load(Ordering::Relaxed));
eprintln!("Breaking due to stop flag");
eprintln!("Stop flag already set, returning 0");
eprintln!("Setting stop flag due to time limit");
eprintln!("quiescence_search: Stop flag already set, returning 0");
eprintln!("quiescence_search: Setting stop flag due to time limit");
```

- [ ] **Step 2: Run tests to confirm nothing broke**

```bash
cargo test
```

Expected: all tests pass, no compilation errors.

- [ ] **Step 3: Commit**

```bash
git add src/engine.rs
git commit -m "refactor: remove debug eprintln calls from engine"
```

---

### Task 2: Add `extract_pv_from_tt`

**Files:**
- Modify: `src/engine.rs`

- [ ] **Step 1: Write the failing test**

Add this test inside the `#[cfg(test)] mod tests` block in `src/engine.rs`:

```rust
#[test]
fn extract_pv_from_tt_returns_moves_up_to_depth() {
    let position = start_position();
    let mut tt = make_tt();
    let stop = make_stop();
    let params = GoParameters { depth: Some(3), ..Default::default() };
    // Run a search so the TT is populated
    select_move(&position, &params, &mut tt, &stop);
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test extract_pv_from_tt
```

Expected: compilation error — `extract_pv_from_tt` not found.

- [ ] **Step 3: Implement `extract_pv_from_tt`**

Add this function in `src/engine.rs`, before the `#[cfg(test)]` block:

```rust
fn extract_pv_from_tt(root: &Position, tt: &TranspositionTable, max_depth: u32) -> Vec<Move> {
    use std::collections::HashSet;
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
            Some(entry) => {
                pv.push(entry.best_move);
                current_position = crate::board::apply_move(&current_position, entry.best_move);
            }
            None => break,
        }
    }

    pv
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test extract_pv_from_tt
```

Expected: both tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/engine.rs
git commit -m "feat: add extract_pv_from_tt for principal variation extraction"
```

---

### Task 3: Emit full UCI `info` lines with PV

**Files:**
- Modify: `src/engine.rs`

- [ ] **Step 1: Write a failing test**

Add this test inside the `#[cfg(test)] mod tests` block in `src/engine.rs`:

```rust
#[test]
fn select_move_emits_uci_info_line_to_stdout() {
    // We can't easily capture println! in tests, so we verify by running a search
    // at depth 1 and confirming the UCI info line format compiles and runs without panic.
    // The real format verification is done via manual observation / GUI integration.
    let position = start_position();
    let mut tt = make_tt();
    let stop = make_stop();
    let params = GoParameters { depth: Some(2), ..Default::default() };
    // Should not panic — this is the main assertion
    let chosen = select_move(&position, &params, &mut tt, &stop);
    let legal_moves = generate_legal_moves(&position);
    assert!(legal_moves.contains(&chosen));
}
```

- [ ] **Step 2: Run test to confirm it currently passes (baseline)**

```bash
cargo test select_move_emits_uci_info_line_to_stdout
```

Expected: PASS (the function already exists, this is a smoke test).

- [ ] **Step 3: Replace the existing `info` output block in `select_move`**

In `src/engine.rs`, find the block after `negamax_pvs(...)` in the `for depth in 1..=max_depth` loop. It currently looks like this:

```rust
        let position_hash = compute_hash(position);
        
        // UCI logging - show search info for each depth
        let elapsed = context.start_time.elapsed();
        let nodes = context.nodes_searched;
        let nps = if elapsed.as_millis() > 0 {
            (nodes as f64 / elapsed.as_millis() as f64 * 1000.0) as u64
        } else {
            0
        };
        
        if let Some(tt_entry) = context.transposition_table.probe(position_hash) {
            best_move = tt_entry.best_move;
            
            // Handle mate scores specially
            let (score_type, score_value) = if tt_entry.score.abs() > MATE_SCORE / 2 {
                let moves_to_mate = (MATE_SCORE - tt_entry.score.abs() + 1) / 2;
                ("mate", moves_to_mate)
            } else {
                ("cp", tt_entry.score)
            };
            
            println!("info depth {} score {} {} nodes {} nps {} time {} pv {}",
                depth,
                score_type,
                score_value,
                nodes,
                nps,
                elapsed.as_millis(),
                move_to_uci_string(best_move)
            );
        } else {
            // Still log even if no TT entry
            println!("info depth {} nodes {} nps {} time {}",
                depth,
                nodes,
                nps,
                elapsed.as_millis()
            );
        }
```

Replace it with:

```rust
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

            let pv = extract_pv_from_tt(position, context.transposition_table, depth);
            let pv_string = pv.iter()
                .map(|&chess_move| move_to_uci_string(chess_move))
                .collect::<Vec<_>>()
                .join(" ");

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
```

- [ ] **Step 4: Run all tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 5: Manual smoke test — verify info line format**

```bash
echo -e "position startpos\ngo depth 4\nquit" | cargo run --release 2>/dev/null
```

Expected output (exact values will vary):
```
info depth 1 score cp 0 nodes 20 nps 400000 time 0 pv e2e4
info depth 2 score cp 0 nodes 420 nps 800000 time 0 pv e2e4 e7e5
info depth 3 score cp 0 nodes 9300 nps 750000 time 12 pv e2e4 e7e5 g1f3
info depth 4 score cp 30 nodes 85000 nps 720000 time 118 pv e2e4 e7e5 g1f3 b8c6
bestmove e2e4
```

Each `info` line must:
- Have `depth` matching the loop iteration
- Have `score cp <N>` or `score mate <N>`
- Have `pv` with at least 1 move, at most `depth` moves in UCI notation (e.g. `e2e4`)

- [ ] **Step 6: Commit**

```bash
git add src/engine.rs
git commit -m "feat: emit UCI info lines with full PV after each depth"
```
