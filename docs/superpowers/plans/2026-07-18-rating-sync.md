# Rating Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe manual and automatic synchronization of FramePair ratings to RAW XMP sidecars and optionally JPG embedded XMP, with explicit conflict handling, read-only plans, persistent preferences, and retryable failures.

**Architecture:** Keep `photo_groups.rs` as the read-only source index and add a focused `rating_sync.rs` domain module for rating resolution, plan authorization, settings, pending failures, and execution. Extend `rating_metadata.rs` with pure XMP/JPEG transformations, then write verified bytes through same-directory temporary files. Both the preview current-photo flow and the cleanup module batch flow call the same Tauri plan/execute commands; automatic mode reuses the same one-photo planner after the FramePair rating database commit succeeds.

**Tech Stack:** Rust 2024, Tauri 2, `quick-xml`, `image`, `tempfile`, React 19, TypeScript 7, Vite 8, Node test runner.

**Execution constraints:** Work serially in the existing checkout and current `main` branch. Do not create a worktree or delegate tasks. This phase only writes rating metadata; it must never copy, move, quarantine, trash, or delete photos.

---

## File Map

- `src-tauri/src/rating_metadata.rs`: bounded parsing plus pure XMP and JPEG rating transformations.
- `src-tauri/src/rating_sync.rs`: conflict resolution, plan snapshots, settings/pending persistence, authorized execution, and automatic one-photo synchronization.
- `src-tauri/src/lib.rs`: Tauri command wiring and shared synchronization lock/state.
- `src-tauri/tests/rating_metadata.rs`: byte-preservation and malformed metadata tests.
- `src-tauri/tests/rating_sync.rs`: resolution, plan, execution, persistence, stale snapshot, and path-safety tests.
- `src/features/rating-sync/types.ts`: shared IPC types for preview and cleanup.
- `src/features/rating-sync/ratingSyncUtils.ts`: pure labels, summaries, and validation helpers.
- `src/features/rating-sync/RatingSyncDialog.tsx`: current-photo manual sync and shared automatic settings.
- `src/features/rating-sync/RatingSyncWorkspace.tsx`: batch configuration, plan review, selection, and execution.
- `src/features/preview/PreviewModule.tsx`: current-photo entry, automatic outcome, pending indicator, and index refresh.
- `src/features/preview/PhotoContextMenu.tsx`: Chinese “同步评分” current-photo command.
- `src/features/cleanup/CleanupModule.tsx`: two-task coordinator for existing pair cleanup and batch rating sync.
- `src/features/cleanup/TaskTypeSelector.tsx`: pair cleanup/rating sync selection without adding a new app sidebar module.
- `src/styles.css`: restrained task selector, sync dialog, batch plan, conflict, and responsive states.
- `tests/frontend-utils.test.mjs`: pure frontend behavior and copy tests.
- `docs/TECHNICAL-SOLUTION.md`: phase-two write boundaries and recovery behavior.

---

### Task 1: Resolve Rating Sources And Build Read-Only Sync Items

**Files:**
- Create: `src-tauri/src/rating_sync.rs`
- Create: `src-tauri/tests/rating_sync.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing conflict-resolution tests**

Create integration tests that import `formats`, `photo_groups`, `rating_metadata`, and the new `rating_sync` module. Exercise this public domain API:

```rust
let state = photo_groups::RatingState {
    frame_pair: 3,
    jpeg_metadata: Some(4),
    raw_xmp: Some(5),
    resolved: 0,
    conflict: true,
};

assert_eq!(
    rating_sync::resolve_rating(&state, &[], RatingConflictPolicy::Skip),
    RatingResolution::Conflict,
);
assert_eq!(
    rating_sync::resolve_rating(&state, &[], RatingConflictPolicy::FramePair),
    RatingResolution::Ready(3),
);
assert_eq!(
    rating_sync::resolve_rating(&state, &[], RatingConflictPolicy::Highest),
    RatingResolution::Ready(5),
);
```

Also verify:

- `External` uses the one external source or equal JPG/RAW sources.
- `External` blocks when JPG and RAW external sources disagree.
- `Skip` accepts equal non-empty sources and uses FramePair when no external rating exists.
- any `-1` or existing `rating_issues` is a hard conflict for every strategy.
- zero is a valid resolved rating and means “clear/unrated”, not “missing plan result”.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rating_sync --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: compilation fails because `rating_sync.rs`, `RatingConflictPolicy`, `RatingResolution`, and `resolve_rating` do not exist.

- [ ] **Step 3: Implement minimal resolution types**

Add serializable camel-case enums and targets:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RatingConflictPolicy {
    Skip,
    FramePair,
    External,
    Highest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RatingResolution {
    Ready(u8),
    Conflict,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncTargets {
    pub(crate) raw_xmp: bool,
    pub(crate) jpeg_metadata: bool,
}
```

Implement `resolve_rating(state, issues, policy)` using only 0-5 values. Treat `-1`, parse/target issues, or incompatible external values under `External` as conflict. Do not mutate files.

- [ ] **Step 4: Add failing plan-item tests**

Create a directory containing paired JPG/RAW groups, apply FramePair ratings, and call:

```rust
let plan = rating_sync::build_plan(
    &index,
    &RatingSyncPlanRequest {
        root: root.to_string_lossy().into_owned(),
        minimum_rating: 1,
        maximum_rating: 5,
        asset_ids: vec![],
        targets: RatingSyncTargets { raw_xmp: true, jpeg_metadata: false },
        conflict_policy: RatingConflictPolicy::Skip,
        jpeg_write_confirmed: false,
    },
    "plan-1".to_string(),
)?;
```

Assert deterministic item order, `ready/unchanged/conflict` counts, target relative paths, and that new RAW sidecars use `relativeStem.xmp`. Assert the planner rejects empty targets, invalid rating ranges, JPG writing without confirmation, a root mismatch, duplicate XMP targets, and unknown asset IDs.

- [ ] **Step 5: Verify plan tests RED, implement, and re-run GREEN**

Implement serializable `RatingSyncPlanSummary`, `RatingSyncPlanItem`, `RatingSyncWrite`, and internal snapshots. The planner must:

- never write files;
- include only requested IDs, or all groups inside the inclusive rating range when IDs are empty;
- mark target values already equal as unchanged;
- block a group instead of silently choosing among duplicate JPG/XMP targets;
- bind every existing target to size and modified time, and every new sidecar to an expected-absent snapshot.

Run the focused command until all task-one tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/rating_sync.rs src-tauri/tests/rating_sync.rs src-tauri/src/lib.rs
git commit -m "feat: plan rating synchronization"
```

---

### Task 2: Rewrite XMP Ratings Without Losing Other Metadata

**Files:**
- Modify: `src-tauri/src/rating_metadata.rs`
- Modify: `src-tauri/tests/rating_metadata.rs`

- [ ] **Step 1: Write failing XMP transformation tests**

Add tests for:

```rust
let input = br#"<?xpacket begin='x'?><x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><rdf:Description xmlns:xmp='http://ns.adobe.com/xap/1.0/' xmp:Label='Green'><xmp:CreatorTool>FramePair test</xmp:CreatorTool><xmp:Rating>2</xmp:Rating></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end='w'?>"#;
let output = rating_metadata::rewrite_xmp_rating(Some(input), 5)?;
assert_eq!(rating_metadata::xmp_rating(&output)?, Some(5));
assert!(String::from_utf8_lossy(&output).contains("FramePair test"));
assert!(String::from_utf8_lossy(&output).contains("Green"));
```

Cover attribute-form ratings, element-form ratings, no existing Rating, rating 0, and creation from `None`. Assert malformed XML, duplicate Rating, `-1`, and output above 4 MiB are rejected.

- [ ] **Step 2: Run RED**

Run the `rating_metadata` integration test. Expected: `rewrite_xmp_rating` is missing.

- [ ] **Step 3: Implement the parser-backed rewrite**

Implement:

```rust
pub(crate) fn rewrite_xmp_rating(input: Option<&[u8]>, rating: u8) -> Result<Vec<u8>, String>
```

Rules:

- validate `rating <= 5`;
- call `xmp_rating` before rewriting existing XML so malformed/duplicate ratings fail;
- stream events through `quick_xml::Writer`;
- replace the existing Rating attribute or element text exactly once;
- if absent, add `xmp:Rating` to the first `rdf:Description`;
- if there is no description, reject rather than inventing an unsafe insertion point;
- for `None`, emit a minimal UTF-8 XMP packet with `x`, `rdf`, and `xmp` namespaces;
- parse the output again and require the requested rating before returning.

- [ ] **Step 4: Run GREEN and commit**

Run focused tests, `cargo fmt`, then:

```bash
git add src-tauri/src/rating_metadata.rs src-tauri/tests/rating_metadata.rs
git commit -m "feat: preserve XMP while updating ratings"
```

---

### Task 3: Rewrite JPG Embedded XMP While Preserving JPEG Bytes

**Files:**
- Modify: `src-tauri/src/rating_metadata.rs`
- Modify: `src-tauri/tests/rating_metadata.rs`

- [ ] **Step 1: Write failing JPEG segment tests**

Build a small valid JPEG with EXIF orientation, an unrelated APP2 segment, and optional Adobe XMP APP1. Test:

```rust
let output = rating_metadata::rewrite_jpeg_rating(&jpeg, 4)?;
fs::write(&path, &output)?;
assert_eq!(rating_metadata::read_jpeg_rating(&path)?, Some(4));
assert!(output.windows(app2.len()).any(|window| window == app2));
assert_eq!(image::load_from_memory(&output)?.dimensions(), (8, 8));
```

Assert the entropy-coded image tail from SOS onward is byte-identical, EXIF remains present, a missing XMP segment is inserted, and only one standard Adobe XMP APP1 remains. Reject malformed JPEG lengths, multiple standard XMP APP1 segments, invalid embedded XMP, and APP1 output over 65,533 payload bytes.

- [ ] **Step 2: Run RED**

Expected: `rewrite_jpeg_rating` is missing.

- [ ] **Step 3: Implement structured JPEG segment rewriting**

Implement:

```rust
pub(crate) fn rewrite_jpeg_rating(input: &[u8], rating: u8) -> Result<Vec<u8>, String>
```

Parse marker boundaries from SOI to SOS. Preserve every non-XMP segment and the complete SOS tail. Replace one APP1 payload beginning with `http://ns.adobe.com/xap/1.0/\0`, or insert it immediately after SOI when absent. Generate XML through `rewrite_xmp_rating`, enforce the 16-bit JPEG segment limit, then decode metadata from the output and require the requested rating.

- [ ] **Step 4: Run GREEN and commit**

```bash
git add src-tauri/src/rating_metadata.rs src-tauri/tests/rating_metadata.rs
git commit -m "feat: preserve JPEG data while updating ratings"
```

---

### Task 4: Execute Authorized Sync Plans With Snapshot Revalidation

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/rating_sync.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/rating_sync.rs`

- [ ] **Step 1: Write failing execution safety tests**

Test real temporary directories and a stored plan:

- create a new `photo.xmp` with the planned rating;
- update an existing XMP and preserve unrelated fields;
- update a confirmed JPG target and preserve decoded pixels/EXIF;
- reject a plan ID/root mismatch, unplanned asset ID, changed target snapshot, newly appeared sidecar, symlink target, path traversal, and consumed plan;
- continue independent groups after one group fails and return per-target Chinese results;
- never modify RAW bytes.

- [ ] **Step 2: Run RED**

Expected: `RatingSyncPlan::execute` and plan store APIs are missing.

- [ ] **Step 3: Promote `tempfile` and implement verified replacement**

Move `tempfile = "3"` from dev-only to normal dependencies. For each write:

1. canonicalize and validate the photo root;
2. validate the relative target path contains only normal components;
3. re-read or confirm absence against the plan snapshot;
4. reject symbolic links and non-files;
5. write transformed bytes to `NamedTempFile::new_in(target_parent)`;
6. `write_all`, `sync_all`, and parse the temporary file to verify the requested rating;
7. persist over an existing target, or `persist_noclobber` for an expected-new sidecar;
8. sync the parent directory where supported.

Use `tempfile`'s platform implementation so Windows uses `MoveFileExW` replacement rather than delete-then-rename. Do not expose an arbitrary destination path to the frontend.

- [ ] **Step 4: Add Tauri plan storage and commands**

Add `RatingSyncPlanStore { current: Mutex<Option<RatingSyncPlan>> }` and commands:

```rust
generate_rating_sync_plan(app, rating_state, plan_state, request)
execute_rating_sync_plan(rating_state, plan_state, request)
```

The generate command loads the FramePair rating database, builds/overlays the shared photo index, creates a unique plan ID, and replaces any older sync plan. The execute command consumes the matching plan before writes begin and accepts only the planned ready asset IDs.

- [ ] **Step 5: Run GREEN, full Rust regression, and commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/rating_sync.rs src-tauri/src/lib.rs src-tauri/tests/rating_sync.rs
git commit -m "feat: execute safe rating sync plans"
```

---

### Task 5: Persist Automatic Settings And Pending Failures

**Files:**
- Modify: `src-tauri/src/rating_sync.rs`
- Modify: `src-tauri/src/ratings.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/rating_sync.rs`
- Modify: `src-tauri/tests/photo_ratings.rs`

- [ ] **Step 1: Write failing settings/pending tests**

Test a versioned application-data JSON document with defaults:

```rust
RatingSyncSettings {
    mode: RatingSyncMode::Manual,
    targets: RatingSyncTargets { raw_xmp: true, jpeg_metadata: false },
    conflict_policy: RatingConflictPolicy::Skip,
    jpeg_write_confirmed: false,
}
```

Verify validation rejects automatic mode without any target and JPG targets without confirmation. Verify damaged, oversized, symlinked, or unknown-version settings are not overwritten. Verify pending entries persist root, asset ID, requested rating, target list, error, and failure time; a later successful retry removes the exact pending entry.

- [ ] **Step 2: Run RED, implement persistence, and run GREEN**

Use the same bounded, temporary-file, sync, and replace pattern as the rating database. Add commands:

```rust
get_rating_sync_state(app, state)
save_rating_sync_settings(app, state, settings)
```

Return settings plus pending failures for the active root. Never store configuration in the photo directory.

- [ ] **Step 3: Write failing automatic-sync integration tests**

Extract a pure service function called by `set_photo_rating` and test:

- manual mode saves FramePair only;
- automatic RAW mode saves FramePair first, then writes/updates XMP;
- automatic conflict or write failure leaves FramePair saved and creates pending state;
- automatic mode never invokes cleanup, quarantine, copy, move, or trash code;
- rating 0 writes XMP Rating 0;
- independent subsequent rating saves remain usable after a pending failure.

- [ ] **Step 4: Implement automatic post-save synchronization**

Change the Tauri `set_photo_rating` command response to include:

```rust
pub(crate) struct AutoSyncOutcome {
    pub(crate) status: AutoSyncStatus, // disabled, unchanged, synced, pending
    pub(crate) message: Option<String>,
}
```

The command must commit `photo-ratings.json` before starting synchronization. Reindex the one root, build a one-asset plan with stored settings, execute only ready writes, and persist a pending failure instead of rolling back the FramePair rating.

- [ ] **Step 5: Run complete Rust tests and commit**

```bash
git add src-tauri/src/rating_sync.rs src-tauri/src/ratings.rs src-tauri/src/lib.rs src-tauri/tests/rating_sync.rs src-tauri/tests/photo_ratings.rs
git commit -m "feat: persist automatic rating sync"
```

---

### Task 6: Add Shared Frontend Sync Types, Settings, And Current-Photo Flow

**Files:**
- Create: `src/features/rating-sync/types.ts`
- Create: `src/features/rating-sync/ratingSyncUtils.ts`
- Create: `src/features/rating-sync/RatingSyncDialog.tsx`
- Modify: `src/features/preview/types.ts`
- Modify: `src/features/preview/PreviewModule.tsx`
- Modify: `src/features/preview/PhotoContextMenu.tsx`
- Modify: `tests/frontend-utils.test.mjs`
- Modify: `src/styles.css`

- [ ] **Step 1: Write failing frontend utility tests**

Test exact Chinese labels, target validation, conflict/ready summaries, automatic safety copy, and outcome messages:

```js
assert.equal(ratingSyncUtils.syncModeNotice("automatic"), "自动同步只更新评分元数据，不会复制、移动或清理照片。");
assert.deepEqual(ratingSyncUtils.validateSyncTargets({ rawXmp: false, jpegMetadata: false }, false), {
  valid: false,
  message: "请至少选择一个评分同步目标",
});
assert.equal(ratingSyncUtils.autoSyncOutcomeNotice({ status: "pending", message: "XMP 只读" }).tone, "warning");
```

- [ ] **Step 2: Run frontend RED and implement pure types/helpers**

Define IPC types matching the Rust camel-case JSON exactly. Re-run until utility tests pass.

- [ ] **Step 3: Add current-photo dialog**

`RatingSyncDialog` has stable `dialog` dimensions and three states:

1. settings: manual/automatic mode, RAW XMP checkbox, default-off JPG metadata checkbox, conflict policy;
2. plan: current source ratings, resolved target, exact files, unchanged/blocked reasons;
3. result: succeeded, unchanged, failed, and retry guidance.

Enabling JPG requires an explicit confirmation checkbox in the same dialog. Saving settings is separate from executing the current-photo plan. Fixed copy states automatic mode only updates rating metadata.

- [ ] **Step 4: Wire preview actions and outcomes**

Add “评分同步” in the preview header, loupe action bar, and Chinese context menu. Current-photo manual sync generates a one-asset plan before enabling execution. After execution, refresh the index so external source values and conflicts update. After rating, show a non-blocking synced/pending message from `autoSync`; never roll back the optimistic FramePair rating for an automatic sync failure.

- [ ] **Step 5: Build, test, and commit**

```bash
npm run test:frontend
npm run build
git add src/features/rating-sync src/features/preview tests/frontend-utils.test.mjs src/styles.css
git commit -m "feat: sync ratings from photo preview"
```

---

### Task 7: Add Batch Rating Sync Inside Pair Cleanup

**Files:**
- Create: `src/features/cleanup/TaskTypeSelector.tsx`
- Create: `src/features/rating-sync/RatingSyncWorkspace.tsx`
- Modify: `src/features/cleanup/CleanupModule.tsx`
- Modify: `src/features/cleanup/CleanupGuideDialog.tsx`
- Modify: `tests/frontend-utils.test.mjs`
- Modify: `src/styles.css`

- [ ] **Step 1: Write failing task/range tests**

Test pure helpers that:

- default to existing pair cleanup;
- preserve independent pair-cleanup and sync settings;
- accept inclusive 0-5 rating ranges and reject minimum greater than maximum;
- select only ready sync plan items;
- never classify unchanged/conflict items as executable.

- [ ] **Step 2: Run RED and implement the task selector**

Add a compact two-option segmented task selector above setup content:

- “配对清理”：existing workflow unchanged.
- “评分同步”：batch root/range/targets/policy workflow.

Keep the app sidebar unchanged. Switching tasks clears only the active review plan, not the other task's persisted directory settings.

- [ ] **Step 3: Implement batch configure/review/execute states**

The sync workspace must support directory selection and drag/drop, 0-5 inclusive range, targets, conflict policy, explicit JPG confirmation, and generation of a read-only plan. Review shows group, three source values, resolved value, exact targets, and Chinese status. Only ready rows are selectable. Execution uses the plan ID and selected asset IDs, then reindexes by generating a fresh plan.

- [ ] **Step 4: Update masked guide and responsive CSS**

Add sync-specific guide steps for task choice, target/range configuration, plan review, and safe execution. Ensure the selector, configuration controls, table, and dialogs fit at the existing 860x620 minimum and at 200% zoom without overlap.

- [ ] **Step 5: Run frontend checks and commit**

```bash
npm run test:frontend
npm run build
git add src/features/cleanup src/features/rating-sync/RatingSyncWorkspace.tsx tests/frontend-utils.test.mjs src/styles.css
git commit -m "feat: add batch rating sync workflow"
```

---

### Task 8: Document, Desktop-Test, And Ship Phase Two

**Files:**
- Modify: `docs/TECHNICAL-SOLUTION.md`
- Modify: `docs/superpowers/specs/2026-07-18-rating-driven-photo-workflow-design.md`

- [ ] **Step 1: Document the implemented boundary**

Document:

- FramePair is still committed first;
- RAW files are never modified;
- RAW ratings use `photo.xmp`, while dual sidecar naming is a hard conflict;
- JPG writing is off by default and requires confirmation;
- every manual write comes from a consumed read-only plan with snapshots;
- automatic failures create pending state and never trigger file operations;
- phase three rule editing/move/copy/cleanup remains unimplemented.

- [ ] **Step 2: Run pristine local verification**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
npm run build
npm run test:frontend
cargo test --manifest-path src-tauri/Cargo.toml --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
git diff --check
```

Expected: every command exits 0 with no test failures or new warnings.

- [ ] **Step 3: Restart and inspect the desktop app**

Start Vite on `http://localhost:1420`, run the Tauri binary with the same explicit rsproxy/offline Cargo configuration, and test:

- current-photo manual RAW XMP sync;
- automatic RAW XMP sync after rating;
- pending failure on a read-only/conflicting sidecar;
- JPG enable confirmation and metadata preservation;
- batch plan range, conflict rows, selection, execute, and refresh;
- pair cleanup still scans and remains unchanged;
- no UI overlap at 1180x780, 860x620, and 200% zoom.

- [ ] **Step 4: Commit, push current `main`, and watch CI**

```bash
git add docs/TECHNICAL-SOLUTION.md docs/superpowers/specs/2026-07-18-rating-driven-photo-workflow-design.md
git commit -m "docs: document safe rating synchronization"
git push origin main
gh run watch <new-run-id> --exit-status
```

Keep the verified local desktop app running for user acceptance.

---

## Phase-Two Completion Gate

- Manual current-photo and batch sync always expose a read-only plan before writing.
- Automatic mode runs only after FramePair save and can only update enabled rating metadata targets.
- RAW source bytes are never changed.
- XMP/JPG transformations preserve unrelated metadata and JPEG image data.
- JPG writing remains disabled until explicit confirmation.
- Duplicate targets, malformed metadata, symlinks, traversal, stale snapshots, and unsupported `-1` states are blocked per group.
- Automatic failures persist as retryable pending entries without rolling back FramePair ratings.
- Existing preview, rating, external editor, pair cleanup, quarantine, restore, and audit behavior remains green.
