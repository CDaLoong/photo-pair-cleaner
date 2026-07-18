# Rating Copy And Move Execution History Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the read-only rating organization plan into a safe copy/move workflow with per-group results, immutable operation history, move restore, and copy undo.

**Architecture:** Keep `operation_plan` responsible for simulation and one-use authorization, add `file_organizer` for preflight and group-level filesystem transactions, and add `operation_history` for versioned manifests and recovery markers under app data. The frontend selects only ready copy/move groups, requires explicit confirmation, displays partial failures without hiding them, and exposes only recovery actions that the backend can still prove safe.

**Tech Stack:** Rust 2024, Tauri 2 commands/state, serde JSON manifests, SHA-256 streaming verification, React 19, TypeScript, Vitest, CSS.

---

### Task 1: One-Use Plan Authorization

**Files:**
- Modify: `src-tauri/src/operation_plan.rs`
- Test: `src-tauri/src/operation_plan.rs`

- [ ] **Step 1: Write failing authorization tests**

Add tests that store a generated plan and assert `take_for_execution(plan_id, canonical_root, group_ids)` accepts only unique ready `copy`/`move` groups, rejects cleanup/conflict/unknown groups, rejects a changed root, and consumes the plan after the first successful take.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test operation_plan::tests::execution --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because `take_for_execution` and the execution selection type do not exist.

- [ ] **Step 3: Implement the authorization boundary**

Add `ExecutionSelection { plan_id, root, group_ids }`, an owned `AuthorizedOperationPlan`, and `OperationPlanStore::take_for_execution`. Canonicalize the request root, compare it to the stored summary root, validate every group before removing the stored plan, and return only selected ready copy/move items plus the immutable rule/sync snapshots.

- [ ] **Step 4: Run focused and module tests**

Run: `cargo test operation_plan --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/operation_plan.rs
git commit -m "feat: authorize rating file operations once"
```

### Task 2: Versioned Operation History

**Files:**
- Create: `src-tauri/src/operation_history.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/operation_history.rs`

- [ ] **Step 1: Write failing history tests**

Cover atomic manifest creation, newest-first listing, malformed/symlinked history rejection, immutable rule and sync snapshots, recovery markers, and refusal to update an unknown operation or group.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test operation_history --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because the module does not exist.

- [ ] **Step 3: Implement the history store**

Persist one `manifest.json` per operation beneath `app_data_dir/rating-operations/<operation-id>/`. Define serializable action/status/member/group/operation records, store preflight and final file snapshots including SHA-256, write through a temporary file followed by `persist_noclobber`, and append recovery records without rewriting the original execution snapshot.

- [ ] **Step 4: Run focused tests**

Run: `cargo test operation_history --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/operation_history.rs src-tauri/src/lib.rs
git commit -m "feat: persist rating operation history"
```

### Task 3: Verified Copy Transactions

**Files:**
- Create: `src-tauri/src/file_organizer.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/file_organizer.rs`

- [ ] **Step 1: Write failing copy transaction tests**

Test source size/mtime drift, source symlinks, untrusted target parents, existing targets, directory creation, byte-for-byte SHA-256 verification, successful multi-member copy, and rollback when a later member fails. Assert one group failure does not prevent an independent group from running.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test file_organizer::tests::copy --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because the organizer does not exist.

- [ ] **Step 3: Implement preflight and copy transaction**

Add `sha2 = "0.10"`. Revalidate source snapshots and target ancestry from the authorized plan, stage each member to a temporary file in its final parent, stream SHA-256 over source and staged copy, commit with no-clobber semantics, and remove unchanged staged/committed outputs if the group cannot complete.

- [ ] **Step 4: Run focused tests**

Run: `cargo test file_organizer::tests::copy --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/file_organizer.rs src-tauri/src/lib.rs
git commit -m "feat: execute verified rating copies"
```

### Task 4: Same-Volume And Cross-Volume Move Transactions

**Files:**
- Modify: `src-tauri/src/file_organizer.rs`
- Test: `src-tauri/src/file_organizer.rs`

- [ ] **Step 1: Write failing move tests**

Cover same-volume rename, rollback of earlier renames after a later failure, forced cross-volume copy/verify/commit/delete behavior, no source deletion until every destination verifies, and partial status when a source deletion fails after destination commit.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test file_organizer::tests::move --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because move execution is not implemented.

- [ ] **Step 3: Implement move transactions**

Prefer atomic rename when source and destination parents share a device. For cross-device groups, reuse verified staging and commit every destination before deleting any source. Roll back completed same-device renames in reverse order; preserve targets and record `partial` when deleting a cross-device source fails so recovery never loses the verified copy.

- [ ] **Step 4: Run focused tests**

Run: `cargo test file_organizer::tests::move --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/file_organizer.rs
git commit -m "feat: execute recoverable rating moves"
```

### Task 5: Destination Rating Sync And Recovery

**Files:**
- Modify: `src-tauri/src/rating_sync.rs`
- Modify: `src-tauri/src/file_organizer.rs`
- Modify: `src-tauri/src/operation_history.rs`
- Test: `src-tauri/src/file_organizer.rs`
- Test: `src-tauri/src/operation_history.rs`

- [ ] **Step 1: Write failing sync and recovery tests**

Assert destination XMP/JPG sync runs only after copy/move commit, copy undo deletes only unchanged created targets, move restore returns unchanged targets to missing original paths, and both operations refuse changed targets, occupied originals, missing files, and path escapes. Include partial move recovery.

- [ ] **Step 2: Run focused tests and verify RED**

Run: `cargo test recovery --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because destination sync and recovery APIs do not exist.

- [ ] **Step 3: Expose a trusted metadata write helper and implement recovery**

Reuse the existing XMP/JPG transformation functions behind a crate-private helper that accepts only organizer-validated paths. Record the post-sync target snapshot/digest. Restore or undo each history group independently, use rename when possible and verified copy/delete across devices, never overwrite, and append an immutable recovery result record.

- [ ] **Step 4: Run focused tests**

Run: `cargo test file_organizer operation_history --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/rating_sync.rs src-tauri/src/file_organizer.rs src-tauri/src/operation_history.rs
git commit -m "feat: restore rating file operations"
```

### Task 6: Tauri Execution And History Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing command-registration tests**

Replace the phase-three assertion that execution is absent with assertions for `execute_operation_plan`, `list_rating_operation_history`, `restore_rating_move`, and `undo_rating_copy`. Retain an assertion that no rating cleanup execution command is registered.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test frontend_exposes_rating_organizer_execution --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because commands are not registered.

- [ ] **Step 3: Implement commands and state locking**

Resolve the app-data history root in commands, consume the plan before spawning blocking filesystem work, pass an operation ID generated by the backend, return per-group `success`/`failed`/`partial`/`skipped` results, and expose list/restore/undo commands without accepting arbitrary paths from the frontend.

- [ ] **Step 4: Run command and backend tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: expose rating organizer commands"
```

### Task 7: Selection, Confirmation, Results, And History UI

**Files:**
- Create: `src/features/rating-rules/OperationExecuteDialog.tsx`
- Create: `src/features/rating-rules/OperationHistoryPanel.tsx`
- Modify: `src/features/rating-rules/OperationPlanReview.tsx`
- Modify: `src/features/rating-rules/RatingRulesWorkspace.tsx`
- Modify: `src/features/rating-rules/types.ts`
- Modify: `src/styles.css`
- Test: `src/App.test.tsx`
- Test: `src/features/rating-rules/ratingRuleUtils.test.ts`

- [ ] **Step 1: Write failing frontend tests**

Test default selection of ready copy/move groups, disabled selection for cleanup/conflict rows, selected group/file/byte summaries, acknowledgment gating, command payloads, Chinese per-group result labels, history listing, move restore, copy undo, and disabled recovery after success or when no longer recoverable.

- [ ] **Step 2: Run frontend tests and verify RED**

Run: `npm test -- --run`

Expected: FAIL because execution and history controls do not exist.

- [ ] **Step 3: Implement the execution workflow**

Add typed execution/history/recovery models. Change the plan title from read-only simulation to an executable review, add stable selection checkboxes only to ready copy/move rows, keep cleanup labeled “第五阶段开放”, and open a modal that repeats root, action counts, members, bytes, and non-overwrite/rollback guarantees before enabling confirmation.

- [ ] **Step 4: Implement results and history recovery UI**

After execution, show every group result and refresh history. Add a compact history section with action/status counts and explicit “恢复移动” or “撤销复制” commands; show recovery conflicts inline and regenerate the directory index after filesystem changes.

- [ ] **Step 5: Run frontend tests**

Run: `npm test -- --run`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/features/rating-rules src/styles.css src/App.test.tsx
git commit -m "feat: add rating organizer execution review"
```

### Task 8: Documentation And End-To-End Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/specs/2026-07-18-rating-driven-photo-workflow-design.md`
- Modify: `docs/superpowers/plans/2026-07-18-copy-move-execution-history.md`

- [ ] **Step 1: Document the shipped safety model**

Document one-use plans, group transactions, cross-volume verification, immutable history, conservative restore/undo rules, and the explicit phase-five boundary for cleanup execution.

- [ ] **Step 2: Run formatting and all automated checks**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml --check`

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Run: `npm test -- --run`

Run: `npm run build`

Expected: all commands PASS.

- [ ] **Step 3: Run desktop smoke testing with temporary copy/move directories**

Start the current Tauri app, generate a plan against disposable fixtures, verify copy, move, history, restore, undo, keyboard/focus behavior, and narrow/wide layouts. Keep the final app process running. If the desktop is locked, record that native visual QA remains blocked rather than claiming it passed.

- [ ] **Step 4: Commit and push**

```bash
git add README.md docs/superpowers/specs/2026-07-18-rating-driven-photo-workflow-design.md docs/superpowers/plans/2026-07-18-copy-move-execution-history.md
git commit -m "docs: document rating organizer execution"
git push origin main
```

- [ ] **Step 5: Watch CI to completion**

Run: `gh run watch --exit-status`

Expected: the pushed `main` workflow completes successfully.
