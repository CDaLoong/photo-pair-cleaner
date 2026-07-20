# Preview Engine V2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Replace the request-path JSON cache and unbounded speculative preview work with a scalable SQLite-backed cache, cost-aware memory retention, cancelable priority scheduling, and a virtualized loupe filmstrip.

**Architecture:** Keep React, Tauri, and Rust. Introduce a focused Rust cache metadata service using SQLite WAL and sharded cache files; keep binary Tauri responses for the current transport. On the frontend, reserve capacity for the selected photo, cancel obsolete speculative windows, limit memory by estimated decoded bytes, and render only the visible filmstrip range.

**Tech Stack:** Rust 2024, Tauri 2, rusqlite 0.39 with bundled SQLite, React 19, TypeScript 7, Node test runner.

---

## Scope And Performance Budget

This milestone must deliver:

- cache lookup and capacity accounting without scanning or summing every cache entry on each request;
- one SQLite transaction for batched access timestamps instead of rewriting a complete JSON manifest;
- sharded cache paths so a single directory does not accumulate 100,000 JPEGs;
- foreground work that is never queued behind more than one active speculative decode;
- cancellation of obsolete queued preload windows when selection or direction changes;
- memory retention bounded by estimated decoded bytes as well as entry count;
- a filmstrip DOM window bounded independently of library size;
- fresh automated coverage and a real-photo cold/warm benchmark.

Target budgets used by tests and diagnostics:

- warm disk lookup backend p95 target: 20 ms per image on local SSD;
- cached adjacent selection: no loading placeholder and no stale request winning;
- speculative decode concurrency: at most one background job;
- memory cache: 512 MiB estimated decoded pixels and 128 entries;
- filmstrip: at most viewport items plus ten overscan items in the DOM;
- cache maintenance: no full directory reconciliation in the foreground request path.

Out of scope for this milestone, but preserved as the next architectural boundary:

- macOS ImageIO and Windows WIC in-process decoder adapters;
- secure custom preview protocol replacing IPC byte copies;
- tiled full-resolution pyramids, 100% zoom, RAW embedded previews, and native GPU rendering.

## File Structure

- Create `src-tauri/src/preview_cache.rs`: SQLite schema, migration, access batching, totals, pruning, and cache statistics.
- Modify `src-tauri/src/preview.rs`: sharded cache keys, legacy flat-file promotion, decode/generate flow, and cache service calls.
- Modify `src-tauri/src/lib.rs`: register the cache module and preserve binary preview responses.
- Create `src-tauri/tests/preview_cache.rs`: behavior and scale-oriented cache tests.
- Modify `src-tauri/tests/preview_index.rs`: include the cache module used by `preview.rs` integration tests.
- Modify `src-tauri/tests/watermark_performance.rs`: add representative cache and real-photo timing assertions.
- Modify `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`: add bundled `rusqlite`.
- Modify `src/features/preview/previewCache.ts`: cost-aware URL LRU, queued cancellation, and foreground/background capacity separation.
- Create `src/features/preview/VirtualPhotoFilmstrip.tsx`: horizontally virtualized filmstrip.
- Modify `src/features/preview/previewUtils.ts`: pure virtual filmstrip window calculation.
- Modify `src/features/preview/PreviewModule.tsx`: direction-aware cancellable preload session and virtual filmstrip composition.
- Modify `src/styles.css`: stable virtual filmstrip geometry.
- Modify `tests/frontend-utils.test.mjs`: scheduler, byte-budget, cancellation, and virtualization tests.
- Modify `README.md` and `docs/TECHNICAL-SOLUTION.md`: document V2 boundaries and remaining native-decoder milestone.

### Task 1: Establish Cache V2 Behavior With Failing Tests

**Files:**
- Create: `src-tauri/tests/preview_cache.rs`
- Create: `src-tauri/src/preview_cache.rs`

- [x] **Step 1: Write the failing SQLite cache contract tests**

Define tests around this public crate API:

```rust
let cache = preview_cache::PreviewCache::open(root.path(), 1_000, 3)?;
cache.record_generated("aa/bb/one.jpg", 400, 512, 10)?;
cache.record_generated("aa/bb/two.jpg", 500, 1600, 20)?;
cache.record_access("aa/bb/one.jpg", 400, 512, 30)?;
cache.flush_accesses()?;
assert_eq!(cache.stats()?.entry_count, 2);
assert_eq!(cache.stats()?.size_bytes, 900);
```

Add separate tests proving:

- reopening preserves totals without a JPEG directory scan;
- access timestamps are flushed in a transaction;
- pruning deletes the oldest file and preserves the protected file;
- a missing cached file is removed from metadata without failing the request;
- legacy JSON metadata is imported once and renamed after successful migration.

- [x] **Step 2: Run the cache test and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test preview_cache
```

Expected: compilation fails because `preview_cache.rs` and `PreviewCache` do not exist.

- [x] **Step 3: Add `rusqlite` and implement the minimal cache service**

The implementation must create:

```sql
CREATE TABLE IF NOT EXISTS preview_entries (
  relative_path TEXT PRIMARY KEY,
  size_bytes INTEGER NOT NULL,
  max_edge INTEGER NOT NULL,
  last_access_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS preview_entries_lru
  ON preview_entries(last_access_ms, relative_path);
```

Configure `journal_mode=WAL`, `synchronous=NORMAL`, and a busy timeout. Maintain `entry_count` and `size_bytes` in the Rust service so normal access is `O(log N)` rather than `O(N)`. Batch access timestamps in memory and flush them in one transaction.

- [x] **Step 4: Run the cache tests and verify GREEN**

Run the same command and expect every cache contract test to pass.

### Task 2: Integrate Sharded Disk Cache And Legacy Promotion

**Files:**
- Modify: `src-tauri/src/preview.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/preview_index.rs`

- [x] **Step 1: Add failing preview integration tests**

Add assertions that a generated hash `aabbcc...` is stored as:

```text
aa/bb/aabbcc....jpg
```

Add a test that places the same key in the legacy flat cache root, requests it, and expects the file to be promoted without decoding the source again.

- [x] **Step 2: Run the focused test and verify RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test preview_index thumbnail_cache
```

Expected: the current flat cache location fails the path assertion.

- [x] **Step 3: Integrate `PreviewCache` into `load_thumbnail`**

Replace the JSON state and full-directory scan with:

```rust
let cache = preview_cache::cache_for(cache_root)?;
if cache_path.is_file() {
    cache.record_access(relative_cache_path, size, max_edge, now_ms())?;
    return fs::read(cache_path).map_err(...);
}
```

Create shard directories only for generated or promoted files. Keep atomic temporary writes, but remove per-entry JSON serialization and its second `sync_all`.

- [x] **Step 4: Run preview and cache tests and verify GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test preview_cache --test preview_index
```

### Task 3: Make Frontend Scheduling Cancelable And Cost-Aware

**Files:**
- Modify: `src/features/preview/previewCache.ts`
- Modify: `tests/frontend-utils.test.mjs`

- [x] **Step 1: Write failing scheduler and memory tests**

Add tests proving:

```js
const scheduler = new PreviewLoadScheduler(3, 1);
// One background task may run; two foreground tasks can start immediately.
```

Also prove a queued aborted task never executes, and a cache with a 70 MiB cost budget evicts an unleased 64 MiB 4096px proxy before retaining another one.

- [x] **Step 2: Run the frontend test and verify RED**

```bash
node --test tests/frontend-utils.test.mjs
```

Expected: constructor/API and cost assertions fail against the current scheduler and count-only cache.

- [x] **Step 3: Implement minimal scheduling and cost accounting**

Use this interface:

```ts
new PreviewUrlCache(release, {
  maxEntries: 128,
  maxCostBytes: 512 * 1024 * 1024,
});
new PreviewLoadScheduler(3, 1);
```

Estimate decoded cost from the requested tier (`maxEdge * maxEdge * 4`), pin active leases, skip or reject aborted queued tasks, and reserve all but one active slot for foreground work. A timeout may stop frontend waiting, but must not create a Blob for a result whose signal is already aborted.

- [x] **Step 4: Run the frontend tests and verify GREEN**

Run the same command and expect zero failures.

### Task 4: Add Direction-Aware Preload Sessions

**Files:**
- Modify: `src/features/preview/previewCache.ts`
- Modify: `src/features/preview/PreviewModule.tsx`
- Modify: `tests/frontend-utils.test.mjs`

- [x] **Step 1: Write the failing preload-order test**

Add a pure helper test for:

```ts
previewPreloadOffsets(1)  // [1, 2, 3, -1]
previewPreloadOffsets(-1) // [-1, -2, -3, 1]
previewPreloadOffsets(0)  // [1, -1]
```

- [x] **Step 2: Verify RED**

Run frontend tests and confirm the missing helper causes the expected failure.

- [x] **Step 3: Implement one preload session per selection**

Track the previous selected index, derive direction, create an `AbortController` in the selection effect, pass its signal to `preloadPhotoPreviewUrl`, and abort the complete old window in cleanup. Preloads acquire and release cache leases so queued work disappears when no consumer remains.

- [x] **Step 4: Verify GREEN**

Run frontend tests and confirm cancellation and order tests pass.

### Task 5: Virtualize The Loupe Filmstrip

**Files:**
- Create: `src/features/preview/VirtualPhotoFilmstrip.tsx`
- Modify: `src/features/preview/previewUtils.ts`
- Modify: `src/features/preview/PreviewModule.tsx`
- Modify: `src/styles.css`
- Modify: `tests/frontend-utils.test.mjs`

- [x] **Step 1: Write the failing window calculation test**

Define `virtualFilmstripWindow` so a 10,000-item filmstrip with a 950px viewport and 95px pitch returns no more than 20 items with five-item overscan.

- [x] **Step 2: Verify RED**

Run frontend tests and confirm the helper is missing.

- [x] **Step 3: Implement the pure helper and component**

Render a fixed-width inner spacer and absolutely positioned buttons only for `[startIndex, endIndex)`. Keep selected-item centering, ratings, context menus, and accessibility labels. Scroll changes update only the transient window state.

- [x] **Step 4: Verify GREEN and build TypeScript**

```bash
npm run test:frontend
npm run build
```

### Task 6: Add Scale Benchmarks And Documentation

**Files:**
- Modify: `src-tauri/tests/watermark_performance.rs`
- Modify: `README.md`
- Modify: `docs/TECHNICAL-SOLUTION.md`

- [x] **Step 1: Add ignored real-photo and non-ignored metadata scale checks**

Create 10,000 SQLite metadata rows without JPEG payloads and assert reopen plus stats does not enumerate the cache directory. Preserve the manual real-photo benchmark, but report cold generation, warm read, cache metadata, and per-tier output bytes separately.

- [x] **Step 2: Run focused benchmark tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_performance
```

Expected: non-ignored scale checks pass; real-photo tests remain explicitly ignored unless `FRAMEPAIR_BENCH_PHOTO_ROOT` is supplied.

- [x] **Step 3: Document current and next milestone boundaries**

State that Preview Engine V2 solves cache and scheduling scalability. Document ImageIO/WIC, the secure preview protocol, and tiled original rendering as Preview Engine V3 rather than implying the 4096px proxy is the original.

### Task 7: Full Verification And Desktop Regression

**Files:**
- No production file changes unless a failing verification first gains a regression test.

- [x] **Step 1: Run all automated checks**

```bash
npm run test:frontend
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
```

- [x] **Step 2: Run real-photo benchmark**

```bash
FRAMEPAIR_BENCH_PHOTO_ROOT=/Users/cdaloong/Downloads/photo \
  cargo test --release --manifest-path src-tauri/Cargo.toml \
  --test watermark_performance benchmarks_real_high_resolution_preview_tiers \
  -- --ignored --nocapture
```

- [x] **Step 3: Build and inspect the macOS application**

Build the debug bundle, open the 162-photo test directory, rapidly navigate at least 30 photos in both directions, and verify:

- selected image work is not blocked behind preloads;
- no persistent loading state remains;
- filmstrip DOM stays bounded;
- current and adjacent images remain sharp and layout stays confined;
- cache statistics remain internally consistent after restart.

## Self-Review

- Spec coverage: cache metadata, task scheduling, memory accounting, filmstrip scale, benchmarks, documentation, and desktop verification all have explicit tasks.
- Deferred scope is explicit: native in-process decoding, direct protocol transport, and tiled originals are Preview Engine V3, not hidden placeholders in V2.
- Type consistency: `PreviewCache`, `PreviewLoadScheduler(total, background)`, `previewPreloadOffsets`, and `virtualFilmstripWindow` retain the same names across tests and implementation steps.
- Placeholder scan: no task delegates unspecified behavior; every deferred subsystem is stated as out of scope rather than an unfinished V2 step.
