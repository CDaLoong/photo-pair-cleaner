# Rating Rules And Read-Only Plan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a third cleanup-workbench task where users can build persistent rating rules and inspect an immutable, conflict-aware file-operation simulation without moving, copying, or deleting any photo.

**Architecture:** Rust owns rule validation, versioned persistence, rating resolution, canonical destination checks, member/target mapping, collision detection, file snapshots, and the read-only plan store. React owns templates and editing ergonomics, but it sends complete rules back to Rust and renders only the returned plan. Phase three deliberately exposes no execution command; phase four will consume the stored backend plan.

**Tech Stack:** Rust 2024, Serde/serde_json, tempfile, Tauri 2, React 19, TypeScript 7, Vite, Node test runner.

---

## File Map

- `src-tauri/src/rating_rules.rs`: rule types, condition evaluation, structural validation, versioned persistence, JSON import/export.
- `src-tauri/src/operation_plan.rs`: immutable read-only operation plan, canonical path validation, member mapping, rule/file conflict detection, summaries, plan store.
- `src-tauri/src/lib.rs`: Tauri command wiring and shared plan/rule state only.
- `src-tauri/tests/rating_rules.rs`: rule validation and persistence integration tests.
- `src-tauri/tests/operation_plan.rs`: planning, conflicts, target mapping, snapshots, and no-write integration tests.
- `src/features/rating-rules/types.ts`: frontend IPC contracts.
- `src/features/rating-rules/ratingRuleUtils.ts`: editable templates, default rule creation, labels, client-side draft validation, plan filtering.
- `src/features/rating-rules/RatingRuleCard.tsx`: one rule's condition, member scope, action, destination, path mode, ordering, and removal controls.
- `src/features/rating-rules/RatingRulesWorkspace.tsx`: root selection/drop, templates, persistence/import/export, optional sync preview, plan generation, status coordination.
- `src/features/rating-rules/OperationPlanReview.tsx`: summary, action filters, expandable group/member details, and conflict display.
- `src/features/cleanup/TaskTypeSelector.tsx`: third task selector.
- `src/features/cleanup/CleanupModule.tsx`: coordinate independent state for all three tasks.
- `src/features/cleanup/CleanupGuideDialog.tsx`: rating-rule-specific mask guide.
- `src/styles.css`: compact desktop workbench layout and 860x620 responsive rules/review behavior.
- `tests/frontend-utils.test.mjs`: frontend rule/template/filter regression coverage.
- `README.md`, `docs/TECHNICAL-SOLUTION.md`, design spec: phase-three boundary and status.

### Task 1: Rating Rule Domain And Validation

**Files:**
- Create: `src-tauri/src/rating_rules.rs`
- Create: `src-tauri/tests/rating_rules.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing condition and validation tests**

Create tests covering unrated/equal/at-least/at-most/between boundaries, duplicate IDs, blank names, empty/duplicate member scopes, invalid 0-5 bounds, missing copy/move destinations, and unexpected destinations on keep/cleanup actions.

```rust
#[test]
fn conditions_match_only_the_configured_zero_to_five_range() {
    assert!(RatingCondition::Unrated.matches(0));
    assert!(RatingCondition::AtLeast { rating: 4 }.matches(5));
    assert!(!RatingCondition::Between { minimum: 2, maximum: 4 }.matches(5));
}

#[test]
fn rules_reject_duplicate_ids_and_missing_move_destinations() {
    let rules = vec![move_rule("same", ""), move_rule("same", "")];
    let error = validate_rule_set(&rules).unwrap_err();
    assert!(error.contains("规则 ID 重复"));
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rating_rules --offline
```

Expected: FAIL because `rating_rules.rs` and its types do not exist.

- [ ] **Step 3: Implement the rule domain**

Implement Serde camel-case contracts and pure validation:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum RatingCondition {
    Unrated,
    Equal { rating: u8 },
    AtLeast { rating: u8 },
    AtMost { rating: u8 },
    Between { minimum: u8, maximum: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RuleMemberKind { Jpeg, Raw, Xmp }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RuleAction { Keep, Copy, Move, Cleanup }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RatingRule {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) condition: RatingCondition,
    pub(crate) member_scope: Vec<RuleMemberKind>,
    pub(crate) action: RuleAction,
    pub(crate) destination: Option<String>,
    pub(crate) preserve_relative_path: bool,
}
```

Keep rule order unchanged. Reject more than 100 rules and names over 80 characters. A rule's member scope must be unique and non-empty. Copy/move require a non-blank destination; keep/cleanup reject a destination.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run the Task 1 command. Expected: all `rating_rules` tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/rating_rules.rs src-tauri/tests/rating_rules.rs src-tauri/src/lib.rs
git commit -m "feat: validate rating organization rules"
```

### Task 2: Versioned Rule Persistence And JSON Transfer

**Files:**
- Modify: `src-tauri/src/rating_rules.rs`
- Modify: `src-tauri/tests/rating_rules.rs`

- [ ] **Step 1: Write failing persistence tests**

Add tests proving defaults load from an absent database, valid order round-trips, corrupt/oversized/symlink databases are rejected without overwrite, imported JSON rejects unknown fields/versions/invalid rules, and export requires a `.json` path and never follows a symlink.

```rust
#[test]
fn versioned_rules_round_trip_in_user_order() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("rating-rules.json");
    let saved = save_rules(&database, &[cleanup_rule("low"), move_rule("high", "/archive")]).unwrap();
    assert_eq!(load_rules(&database).unwrap().rules, saved);
}
```

- [ ] **Step 2: Run and verify RED**

Run the Task 1 test command. Expected: FAIL because persistence APIs are missing.

- [ ] **Step 3: Implement safe persistence/import/export**

Use a version-1 envelope with `#[serde(deny_unknown_fields)]`, a 4 MiB limit, `symlink_metadata`, `NamedTempFile` in the destination directory, `sync_all`, and atomic replacement. Export serializes the same envelope; import validates before returning rules and never modifies the app database by itself.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RatingRuleDatabase {
    version: u8,
    rules: Vec<RatingRule>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingRuleState {
    pub(crate) rules: Vec<RatingRule>,
}
```

- [ ] **Step 4: Run and verify GREEN**

Run the Task 1 test command. Expected: all tests pass with no warnings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/rating_rules.rs src-tauri/tests/rating_rules.rs
git commit -m "feat: persist rating organization rules"
```

### Task 3: Read-Only Rule Matching And Conflict Plan

**Files:**
- Create: `src-tauri/src/operation_plan.rs`
- Create: `src-tauri/tests/operation_plan.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing rule-plan tests**

Build temporary JPG/RAW/XMP groups and apply FramePair ratings. Test one matching rule, no matching rule, disabled rules, missing requested formats, two matching terminal rules, and a default conflict-policy rating source conflict.

```rust
#[test]
fn repeated_terminal_matches_are_conflicts_not_first_rule_wins() {
    let index = rated_index(&[("A.JPG", 4), ("A.NEF", 4)]);
    let plan = build_operation_plan(&index, request(vec![move_rule("one"), cleanup_rule("two")]), "plan-1".into()).unwrap();
    assert_eq!(plan.summary().items[0].status, OperationPlanStatus::Conflict);
    assert!(plan.summary().items[0].issues.iter().any(|issue| issue.contains("命中多条")));
}
```

- [ ] **Step 2: Run and verify RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test operation_plan --offline
```

Expected: FAIL because `operation_plan.rs` does not exist.

- [ ] **Step 3: Implement immutable plan types and matching**

Define `OperationPlanRequest`, `OperationPlanSummary`, `OperationPlanItem`, `PlannedMember`, `OperationPlanStatus`, `PlannedSyncAction`, and `OperationPlanStore`. Resolve the work score with `rating_sync::resolve_rating`. Evaluate all enabled rules; zero matches are skipped, one match is planned, and more than one match is a blocking conflict even when actions are identical.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OperationPlanStatus { Ready, Keep, Skipped, Conflict }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationPlanItem {
    pub(crate) group_id: String,
    pub(crate) relative_stem: String,
    pub(crate) rating: Option<u8>,
    pub(crate) matched_rule_ids: Vec<String>,
    pub(crate) terminal_action: Option<RuleAction>,
    pub(crate) status: OperationPlanStatus,
    pub(crate) members: Vec<PlannedMember>,
    pub(crate) missing_kinds: Vec<RuleMemberKind>,
    pub(crate) sync_actions: Vec<PlannedSyncAction>,
    pub(crate) issues: Vec<String>,
}
```

The store exposes `replace` and read-only `current_summary`; there is no `take` or execute command in phase three.

- [ ] **Step 4: Run and verify GREEN**

Run the Task 3 command. Expected: all matching/conflict tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/operation_plan.rs src-tauri/tests/operation_plan.rs src-tauri/src/lib.rs
git commit -m "feat: plan rating rule matches"
```

### Task 4: Canonical Targets, Snapshots, And Sync Preview

**Files:**
- Modify: `src-tauri/src/operation_plan.rs`
- Modify: `src-tauri/tests/operation_plan.rs`

- [ ] **Step 1: Write failing path and snapshot tests**

Test preserved relative layout, flat layout, existing target rejection, two sources flattening to one target, root/destination equality or nesting, per-member size/modified snapshots, and zero filesystem writes. Test optional RAW/JPG sync previews, JPG confirmation, cleanup-before-sync, and hard metadata conflicts.

```rust
#[test]
fn flat_target_collisions_block_every_affected_group_without_writing() {
    let source = fixture_with(&["day/A.JPG", "other/A.JPG"]);
    let target = tempfile::tempdir().unwrap();
    let before = directory_entries(target.path());
    let plan = plan_flat_copy(&source, target.path());
    assert_eq!(plan.summary().conflicts, 2);
    assert_eq!(directory_entries(target.path()), before);
}
```

- [ ] **Step 2: Run and verify RED**

Run the Task 3 command. Expected: FAIL on missing target mapping/collision behavior.

- [ ] **Step 3: Implement safe read-only target planning**

Canonicalize root and every enabled copy/move destination. Reject equal, ancestor, or descendant roots. Preserve mode appends each source relative path; flat mode appends only the file name. Use `symlink_metadata` to reject occupied targets and mark every duplicate planned target as conflict. Store source size and modified time but do not create directories or files.

Optional sync settings use the phase-two conflict policy and targets. Preview source metadata writes for keep, destination metadata writes for moved/copied members, and before-cleanup writes only when explicitly enabled. RAW original files are never sync targets; JPG sync remains gated by confirmation.

- [ ] **Step 4: Run and verify GREEN**

Run the Task 3 command. Expected: all plan tests pass and fixture target directories remain unchanged.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/operation_plan.rs src-tauri/tests/operation_plan.rs
git commit -m "feat: simulate safe rating file operations"
```

### Task 5: Tauri Rule And Plan Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/rating_rules.rs`
- Modify: `src-tauri/tests/operation_plan.rs`

- [ ] **Step 1: Write failing serialization and store tests**

Verify request/response camel-case fields, persisted rules remain independent from a generated plan, replacing a plan invalidates the previous in-memory summary, and no operation execution command is registered.

- [ ] **Step 2: Run and verify RED**

Run both backend integration test targets. Expected: FAIL on missing command-facing functions/store behavior.

- [ ] **Step 3: Wire commands**

Add commands:

```rust
get_rating_rules() -> RatingRuleState
save_rating_rules(rules: Vec<RatingRule>) -> RatingRuleState
import_rating_rules(path: String) -> RatingRuleState
export_rating_rules(path: String, rules: Vec<RatingRule>) -> String
generate_operation_plan(request: OperationPlanRequest) -> OperationPlanSummary
```

Use the app-data `rating-rules.json`, the existing rating database overlay, background blocking tasks, and `OperationPlanStore`. Do not add an operation execution command.

- [ ] **Step 4: Run full Rust tests and verify GREEN**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Expected: all Rust tests pass with no warnings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/rating_rules.rs src-tauri/src/operation_plan.rs src-tauri/tests/rating_rules.rs src-tauri/tests/operation_plan.rs
git commit -m "feat: expose read-only rating operation plans"
```

### Task 6: Frontend Rule Contracts, Templates, And Draft Validation

**Files:**
- Create: `src/features/rating-rules/types.ts`
- Create: `src/features/rating-rules/ratingRuleUtils.ts`
- Modify: `tests/frontend-utils.test.mjs`

- [ ] **Step 1: Write failing frontend utility tests**

Import `ratingRuleUtils.ts` and test four templates, default move/group/preserved-path values, unique IDs, condition labels, destination validation, duplicate member rejection, plan filters, and conflict rows never being selectable.

```js
test("new rating rules use the safe agreed defaults", () => {
  const rule = ratingRuleUtils.createRatingRule("rule-1");
  assert.equal(rule.action, "move");
  assert.deepEqual(rule.memberScope, ["jpeg", "raw", "xmp"]);
  assert.equal(rule.preserveRelativePath, true);
});
```

- [ ] **Step 2: Run and verify RED**

```bash
npm run test:frontend
```

Expected: FAIL because the frontend rating-rule files do not exist.

- [ ] **Step 3: Implement types and pure utilities**

Mirror Rust camel-case contracts exactly. Templates fill editable rules but leave destination strings empty for copy/move. Utilities return Chinese labels and actionable validation messages without touching React state or Tauri.

- [ ] **Step 4: Run and verify GREEN**

Run the Task 6 command. Expected: all frontend tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/features/rating-rules/types.ts src/features/rating-rules/ratingRuleUtils.ts tests/frontend-utils.test.mjs
git commit -m "feat: define rating rule editor behavior"
```

### Task 7: Rating Rule Editor And Read-Only Review UI

**Files:**
- Create: `src/features/rating-rules/RatingRuleCard.tsx`
- Create: `src/features/rating-rules/RatingRulesWorkspace.tsx`
- Create: `src/features/rating-rules/OperationPlanReview.tsx`
- Modify: `src/features/cleanup/TaskTypeSelector.tsx`
- Modify: `src/features/cleanup/CleanupModule.tsx`
- Modify: `src/features/cleanup/CleanupGuideDialog.tsx`
- Modify: `src/features/rating-sync/types.ts`
- Modify: `src/styles.css`
- Modify: `tests/frontend-utils.test.mjs`

- [ ] **Step 1: Add failing static workflow tests**

Test that the task default remains pair cleanup, the third task label is Chinese, templates do not execute, destination is required for copy/move, conflict rows are read-only, and the UI safety copy says phase three never moves/copies/cleans photos.

- [ ] **Step 2: Run and verify RED**

Run `npm run test:frontend`. Expected: FAIL because the third workflow and safety copy are absent.

- [ ] **Step 3: Build the editor and review components**

Add the third `ratingRules` task. Provide root click/drop selection, template menu, add/reorder/enable/remove rule controls, condition controls, member checkboxes, icon actions, target selection, preserve/flat segmented control, optional sync preview settings, save/import/export, and generate-plan command.

The review shows summary counts and bytes, action/status filters, expandable member source/target snapshots, rule IDs/names, rating sources, missing member kinds, and Chinese conflicts. It contains no execute button and displays:

> 当前仅生成只读模拟计划，不会移动、复制或清理照片。

Keep each task's draft state mounted independently. Use stable callbacks/state derivation, direct icon imports, and memoized filtered plan rows rather than recomputing target lists in render loops.

- [ ] **Step 4: Add a rating-rule mask guide**

Guide order: choose task, choose root/template, edit rules and destinations, configure optional sync, inspect read-only plan. Explain that execution starts only in phase four and templates never create directories.

- [ ] **Step 5: Run and verify GREEN**

```bash
npm run test:frontend
npm run build
```

Expected: all frontend tests and TypeScript production build pass.

- [ ] **Step 6: Commit**

```bash
git add src/features/rating-rules src/features/cleanup/TaskTypeSelector.tsx src/features/cleanup/CleanupModule.tsx src/features/cleanup/CleanupGuideDialog.tsx src/features/rating-sync/types.ts src/styles.css tests/frontend-utils.test.mjs
git commit -m "feat: add rating rule simulation workspace"
```

### Task 8: Document, Desktop-Test, And Ship Phase Three

**Files:**
- Modify: `README.md`
- Modify: `docs/TECHNICAL-SOLUTION.md`
- Modify: `docs/superpowers/specs/2026-07-18-rating-driven-photo-workflow-design.md`

- [ ] **Step 1: Document the implemented boundary**

Document rule persistence/import/export, four editable templates, default move/group/preserved-path values, canonical destination and collision checks, immutable read-only plan, and the explicit absence of move/copy/cleanup execution until phases four and five.

- [ ] **Step 2: Run pristine verification**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
npm run build
npm run test:frontend
cargo test --manifest-path src-tauri/Cargo.toml --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
git diff --check
```

Expected: every command exits 0 with no failures or new warnings.

- [ ] **Step 3: Desktop QA**

Restart the latest Tauri binary and verify task switching, template editing, destination creation/selection, drag/drop root, save/import/export, conflict and collision rows, expanded members, and no execution control. Inspect 1180x780, 860x620, both left sidebars collapsed, and 200% zoom. Confirm existing preview, rating sync, and pair-cleanup setup still open without overlap.

- [ ] **Step 4: Commit, push current main, and watch CI**

```bash
git add README.md docs/TECHNICAL-SOLUTION.md docs/superpowers/specs/2026-07-18-rating-driven-photo-workflow-design.md
git commit -m "docs: document read-only rating rules"
git push origin main
gh run watch <new-run-id> --exit-status
```

Keep the verified desktop application running for user acceptance.

---

## Phase-Three Completion Gate

- The cleanup workbench has exactly three independent task drafts and still defaults to pair cleanup.
- Rules cover all five rating condition forms, custom JPG/RAW/XMP scopes, four terminal actions, custom destinations, and preserve/flat layouts.
- Templates populate editable drafts only; they never create directories or generate plans automatically.
- Rust rejects structurally invalid rules, unsafe/overlapping destinations, existing targets, flat collisions, rating conflicts, and repeated terminal matches.
- Plans bind canonical root, complete ordered rule snapshot, sync preference snapshot, source file size/modified snapshots, and explicit source/target paths.
- The frontend can filter and expand the returned plan but cannot change paths/actions or execute it.
- Plan generation creates, moves, copies, deletes, or renames zero user files.
- Existing preview, rating sync, pair cleanup, quarantine, restore, and audit tests remain green.
