# Rating Cleanup Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Execute rating-rule cleanup groups through either the recoverable FramePair quarantine or the system trash, with one-use authorization, group preflight, receipts, and safe quarantine restore.

**Architecture:** Extend the existing rating operation selection and history instead of creating a second rule engine. The backend remains authoritative: it accepts only ready plan groups, requires one cleanup destination for every selected cleanup group, derives every source and quarantine path from the stored plan, and records `quarantine` or `trash` as the actual action. Quarantine reuses `.framepair-quarantine/<operation-id>/` and the organizer recovery pipeline; system trash is intentionally non-recoverable inside FramePair.

**Tech Stack:** Rust 2024, Tauri 2, `trash` crate, serde history manifests, SHA-256 fingerprints, React 19, TypeScript, Node test runner, CSS.

---

### Task 1: Cleanup-Aware One-Use Authorization

**Files:**
- Modify: `src-tauri/src/operation_plan.rs`
- Test: `src-tauri/src/operation_plan.rs`

- [x] **Step 1: Write failing authorization tests**

Add focused tests proving that the selection below authorizes ready cleanup groups, defaults nowhere in the backend, rejects cleanup without a destination, rejects a cleanup destination when no cleanup group is selected, and still consumes the plan only after successful validation.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CleanupExecutionDestination {
    Quarantine,
    Trash,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecutionSelection {
    pub(crate) plan_id: String,
    pub(crate) root: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) cleanup_destination: Option<CleanupExecutionDestination>,
}
```

- [x] **Step 2: Run the focused tests and verify RED**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml operation_plan::tests::cleanup_execution --offline`

Expected: FAIL because cleanup groups remain forbidden and the selection has no cleanup destination.

- [x] **Step 3: Implement cleanup authorization**

Allow only ready `copy`, `move`, or `cleanup` items. Compute `contains_cleanup` from the selected stored items and require `cleanup_destination.is_some() == contains_cleanup`. Store the validated destination on `AuthorizedOperationPlan`; do not accept paths or actions from the frontend.

- [x] **Step 4: Run operation-plan tests and verify GREEN**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml operation_plan --offline`

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/operation_plan.rs
git commit -m "feat: authorize rating cleanup plans"
```

### Task 2: Cleanup Actions In Immutable History

**Files:**
- Modify: `src-tauri/src/operation_history.rs`
- Test: `src-tauri/src/operation_history.rs`

- [x] **Step 1: Write failing history tests**

Cover `OrganizerAction::Quarantine`, `OrganizerAction::Trash`, and `RecoveryKind::RestoreQuarantine`. Assert quarantine groups count as recoverable, trash groups never do, a trash recovery record is rejected, and older copy/move manifests still deserialize.

```rust
pub(crate) enum OrganizerAction {
    Copy,
    Move,
    Quarantine,
    Trash,
}

pub(crate) enum RecoveryKind {
    RestoreMove,
    UndoCopy,
    RestoreQuarantine,
}
```

- [x] **Step 2: Run the focused tests and verify RED**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml operation_history --offline`

Expected: FAIL because cleanup actions and quarantine recovery do not exist.

- [x] **Step 3: Implement action-aware recovery validation**

Change `expected_recovery_kind` to return `Option<RecoveryKind>`: copy maps to undo, move maps to restore move, quarantine maps to restore quarantine, and trash maps to `None`. Count only groups with a recovery kind and a successful/partial result as recoverable.

- [x] **Step 4: Run history tests and verify GREEN**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml operation_history --offline`

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/operation_history.rs
git commit -m "feat: record rating cleanup history"
```

### Task 3: Recoverable Quarantine Transactions

**Files:**
- Modify: `src-tauri/src/file_organizer.rs`
- Modify: `src-tauri/src/quarantine.rs`
- Test: `src-tauri/src/file_organizer.rs`

- [x] **Step 1: Write failing quarantine transaction tests**

Test a complete JPG/RAW/XMP group moving to `.framepair-quarantine/<operation-id>/<relative-path>`, source drift blocking the whole group, an occupied quarantine target blocking the whole group, rollback after a later rename failure, and history persistence failure restoring unchanged files to their original paths.

- [x] **Step 2: Run focused tests and verify RED**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml file_organizer::tests::quarantine --offline`

Expected: FAIL because organizer cleanup execution does not exist.

- [x] **Step 3: Implement trusted quarantine target derivation**

Expose a crate-private helper in `quarantine.rs` that validates the generated operation ID and returns the trusted operation root without accepting a frontend path. In `file_organizer.rs`, derive each target from the stored `source_relative_path`, validate every source snapshot before moving any group member, create trusted parents inside the operation root, and reuse the rename rollback machinery with `OrganizerAction::Quarantine`.

- [x] **Step 4: Implement quarantine rollback and restore trust**

On manifest failure, restore unchanged quarantine targets to missing source paths. For history recovery, trust quarantine targets only when they are inside `root/.framepair-quarantine/<manifest.operation_id>/`; require every member to pass source-vacancy and fingerprint checks before restoring the group.

- [x] **Step 5: Run focused tests and verify GREEN**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml file_organizer --offline`

Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add src-tauri/src/file_organizer.rs src-tauri/src/quarantine.rs
git commit -m "feat: quarantine rating cleanup groups"
```

### Task 4: System Trash And Cleanup-Before-Sync

**Files:**
- Modify: `src-tauri/src/file_organizer.rs`
- Test: `src-tauri/src/file_organizer.rs`

- [x] **Step 1: Write failing trash and sync tests**

Assert all group members are preflighted before the first trash call, successful members are recorded without a recovery snapshot, a later trash failure reports `partial`, cleanup-before XMP/JPG sync runs only when the plan contains `BeforeCleanup` actions, sync failure prevents that group from entering quarantine/trash, and a newly created cleanup XMP is included in the terminal cleanup result. Extend the existing private `ExecutionOptions` with a test-only trash-delete mode and failure index so tests remove only disposable fixture files instead of sending them to the machine's real trash.

- [x] **Step 2: Run focused tests and verify RED**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml file_organizer::tests::cleanup --offline`

Expected: FAIL because trash cleanup and source sync are unsupported.

- [x] **Step 3: Implement cleanup-before sync**

Validate every sync target against the canonical photo root and require `SyncTiming::BeforeCleanup`. Write metadata through `rating_sync::write_rating_to_validated_path`; refresh any affected planned member snapshot before cleanup and add a newly created XMP to the validated group so it cannot be orphaned by the same cleanup transaction.

- [x] **Step 4: Implement system trash receipts**

After whole-group preflight and optional sync, call `trash::delete` per validated member. Record `OrganizerAction::Trash`, absolute source paths, success/failure messages, no target path, and no target fingerprint. Return `success` only when every member entered the system trash; otherwise return `partial` without pretending FramePair can restore it.

- [x] **Step 5: Run organizer tests and verify GREEN**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml file_organizer --offline`

Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add src-tauri/src/file_organizer.rs
git commit -m "feat: execute rating cleanup safely"
```

### Task 5: Tauri Cleanup And Restore Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`
- Test: `tests/frontend-utils.test.mjs`

- [x] **Step 1: Write failing command-registration tests**

Require `execute_operation_plan` to pass the authorized cleanup destination into the organizer, register `restore_rating_quarantine`, retain `open_system_trash`, and assert there is still no path-based `execute_rating_cleanup` command.

- [x] **Step 2: Run tests and verify RED**

Run: `npm run test:frontend`

Expected: FAIL because quarantine restore is not registered and the phase-five boundary assertion is absent.

- [x] **Step 3: Implement the command boundary**

Add `restore_rating_quarantine` with the same app-data-only `OrganizerRecoveryRequest` used by move restore and copy undo. Keep the unified execution command; its request contains only plan ID, canonical root, selected group IDs, and the destination enum.

- [x] **Step 4: Run command and backend tests**

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml --offline`

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs tests/frontend-utils.test.mjs
git commit -m "feat: expose rating cleanup recovery"
```

### Task 6: Cleanup Selection And Confirmation UI

**Files:**
- Modify: `src/features/rating-rules/types.ts`
- Modify: `src/features/rating-rules/ratingRuleUtils.ts`
- Modify: `src/features/rating-rules/OperationPlanReview.tsx`
- Modify: `src/features/rating-rules/OperationExecuteDialog.tsx`
- Modify: `src/features/rating-rules/RatingRulesWorkspace.tsx`
- Modify: `src/styles.css`
- Test: `tests/frontend-utils.test.mjs`

- [x] **Step 1: Write failing frontend behavior tests**

Test that ready cleanup groups are selectable and counted separately, the default cleanup destination is `quarantine`, a selection without cleanup sends `cleanupDestination: null`, a cleanup selection sends the chosen enum, the dialog changes warnings and path preview for quarantine versus trash, and cleanup is no longer labeled as a future phase.

- [x] **Step 2: Run frontend tests and verify RED**

Run: `npm run test:frontend`

Expected: FAIL because cleanup is disabled in the review and absent from the selection summary.

- [x] **Step 3: Implement cleanup-aware review and dialog**

Extend the selection summary with `cleanupGroups` and `cleanupBytes`. In the dialog, show a two-option segmented control only when cleanup groups are selected, default it to “FramePair 隔离区（可恢复）”, offer “系统回收站（应用内不可恢复）”, preview source paths plus the quarantine pattern, and keep the explicit acknowledgment checkbox.

- [x] **Step 4: Implement the command payload and messages**

Store the dialog choice in workspace state, send it as `cleanupDestination`, clear the one-use plan after any execution attempt, refresh preview/history after success or partial success, and describe quarantine restore versus system trash honestly in the completion notice.

- [x] **Step 5: Run frontend tests and build**

Run: `npm run test:frontend`

Run: `npm run build`

Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add src/features/rating-rules src/styles.css tests/frontend-utils.test.mjs
git commit -m "feat: confirm rating cleanup destination"
```

### Task 7: Quarantine Recovery And Trash History UI

**Files:**
- Modify: `src/features/rating-rules/OperationHistoryPanel.tsx`
- Modify: `src/features/rating-rules/RatingRulesWorkspace.tsx`
- Modify: `src/features/rating-rules/types.ts`
- Modify: `src/styles.css`
- Test: `tests/frontend-utils.test.mjs`

- [x] **Step 1: Write failing history UI tests**

Require quarantine groups to expose “恢复隔离”, trash groups to expose “打开系统回收站” but no recovery action, completed quarantine recovery to disappear, and mixed histories to retain move restore and copy undo.

- [x] **Step 2: Run frontend tests and verify RED**

Run: `npm run test:frontend`

Expected: FAIL because the history UI knows only copy and move.

- [x] **Step 3: Implement action-specific history controls**

Map `restoreQuarantine` to `restore_rating_quarantine`, keep the historical root refresh behavior, and invoke `open_system_trash` only from an explicit history button. Use Chinese action/status labels and do not show a recoverable count for trash groups.

- [x] **Step 4: Run frontend tests and build**

Run: `npm run test:frontend`

Run: `npm run build`

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src/features/rating-rules src/styles.css tests/frontend-utils.test.mjs
git commit -m "feat: recover quarantined rating groups"
```

### Task 8: Documentation And End-To-End Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/specs/2026-07-18-rating-driven-photo-workflow-design.md`
- Modify: `docs/superpowers/plans/2026-07-18-rating-cleanup-execution.md`

- [ ] **Step 1: Document the shipped phase-five boundary**

Document default quarantine, explicit trash selection, full-group preflight, cleanup-before sync, quarantine recovery, non-recoverable system trash, and the remaining phase-six onboarding/polish scope.

- [ ] **Step 2: Run all automated checks**

Run: `rustfmt --edition 2024 --check $(rg --files src-tauri/src src-tauri/tests | rg '\.rs$')`

Run: `cargo clippy --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --manifest-path src-tauri/Cargo.toml --all-targets --offline -- -D warnings`

Run: `cargo --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path src-tauri/Cargo.toml --offline`

Run: `npm run test:frontend`

Run: `npm run build`

Expected: all checks PASS.

- [ ] **Step 3: Run desktop/browser smoke testing**

Start the current Tauri app and verify cleanup selection, default quarantine, trash warning, confirmation gating, history controls, narrow/wide layouts, and preview refresh. Use disposable fixtures for real quarantine/restore and system trash only when native desktop interaction is available; if macOS is locked, record the native limitation instead of claiming it passed.

- [ ] **Step 4: Commit, push, and watch CI**

```bash
git add README.md docs/superpowers/specs/2026-07-18-rating-driven-photo-workflow-design.md docs/superpowers/plans/2026-07-18-rating-cleanup-execution.md
git commit -m "docs: document rating cleanup execution"
git push origin main
gh run watch --exit-status
```
