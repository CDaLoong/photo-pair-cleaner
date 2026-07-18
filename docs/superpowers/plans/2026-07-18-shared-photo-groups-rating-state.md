# Shared Photo Groups And Rating State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish one read-only photo-group and multi-source rating model shared by preview and future cleanup workflows without changing current user-facing preview or pair-cleanup behavior.

**Architecture:** Move logical photo grouping out of `preview.rs` into a focused `photo_groups.rs` domain module, centralize photo keys and XMP rating parsing, then enrich each group with JPG/RAW/XMP members and read-only rating-source state. Keep the existing scalar `rating` field as the resolved FramePair score for compatibility while adding structured `ratingState`, so phase two can implement synchronization without another index rewrite.

**Tech Stack:** Rust 2024, Tauri 2, `image` 0.25 decoder metadata APIs, `quick-xml` 0.38, React 19, TypeScript 7, Node test runner.

**Execution Constraint:** Execute inline on the current `main` branch and current workspace. Do not create a worktree or run implementation tasks concurrently.

---

## File Map

### New Backend Units

- `src-tauri/src/rating_metadata.rs`: bounded, read-only XMP/JPG rating parsing.
- `src-tauri/src/photo_groups.rs`: shared photo-group indexing, members, rating state, and FramePair overlay.
- `src-tauri/tests/rating_metadata.rs`: metadata parsing and JPEG XMP extraction integration tests.
- `src-tauri/tests/photo_groups.rs`: grouping, sidecar association, external rating, and conflict tests.

### Existing Backend Units

- `src-tauri/src/formats.rs`: shared normalized photo-group key.
- `src-tauri/src/reference.rs`: consume the shared rating parser instead of owning one.
- `src-tauri/src/preview.rs`: retain only preview path validation, thumbnail decoding, orientation, and cache behavior.
- `src-tauri/src/lib.rs`: register new modules, use the shared index, and overlay FramePair ratings.
- `src-tauri/tests/preview_index.rs`: retain thumbnail tests and remove grouping ownership.
- `src-tauri/tests/photo_ratings.rs`: verify FramePair overlay remains keyed by the same group ID.

### Frontend Units

- `src/features/preview/types.ts`: add member and structured rating-state types.
- `src/features/preview/previewUtils.ts`: keep optimistic rating updates internally consistent.
- `src/features/preview/PreviewModule.tsx`: use the rating-state helper while preserving current rendering and filters.
- `tests/frontend-utils.test.mjs`: cover nested rating-state updates and unchanged filtering.

### Documentation

- `docs/TECHNICAL-SOLUTION.md`: document the shared group boundary and read-only rating sources.

---

### Task 1: Centralize Photo Group Keys

**Files:**
- Modify: `src-tauri/src/formats.rs:27-44`
- Modify: `src-tauri/src/lib.rs:200-211`
- Test: `src-tauri/src/formats.rs:54-78`

- [ ] **Step 1: Write the failing key tests**

Add tests for a regular photo and both XMP naming forms:

```rust
#[test]
fn photo_group_keys_are_stable_and_optionally_case_sensitive() {
    assert_eq!(photo_group_key(Path::new("Day/A.NEF"), false), "day/a");
    assert_eq!(photo_group_key(Path::new("Day/A.JPG"), false), "day/a");
    assert_eq!(photo_group_key(Path::new("Day/A.NEF"), true), "Day/A");
}

#[test]
fn sidecars_resolve_to_the_same_logical_photo_key() {
    assert_eq!(sidecar_match_keys(Path::new("Day/A.xmp"), false), vec!["day/a"]);
    assert_eq!(
        sidecar_match_keys(Path::new("Day/A.NEF.xmp"), false),
        vec!["day/a.nef", "day/a"],
    );
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml formats::tests --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: FAIL because `photo_group_key` is not defined.

- [ ] **Step 3: Implement the shared key**

Add to `formats.rs`:

```rust
pub(crate) fn normalized_path_key(path: &Path, case_sensitive: bool) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if case_sensitive { value } else { value.to_lowercase() }
}

pub(crate) fn photo_group_key(path: &Path, case_sensitive: bool) -> String {
    normalized_path_key(&path.with_extension(""), case_sensitive)
}
```

Update `sidecar_match_keys()` to call `normalized_path_key()`. Remove `match_key()` from `lib.rs` and replace its call sites with `formats::photo_group_key(relative, request.case_sensitive)`.

- [ ] **Step 4: Run key and pair-scan regressions**

Run the focused formats test, then:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test safety_logic --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: PASS with unchanged pair matching.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/formats.rs src-tauri/src/lib.rs
git commit -m "refactor: share photo group keys"
```

---

### Task 2: Extract The Shared XMP Rating Parser

**Files:**
- Create: `src-tauri/src/rating_metadata.rs`
- Create: `src-tauri/tests/rating_metadata.rs`
- Modify: `src-tauri/src/lib.rs:1-8`
- Modify: `src-tauri/src/reference.rs:126-186`

- [ ] **Step 1: Write failing parser tests**

Create `src-tauri/tests/rating_metadata.rs` with the module path and tests:

```rust
#[path = "../src/rating_metadata.rs"]
mod rating_metadata;

#[test]
fn reads_attribute_and_element_ratings() {
    assert_eq!(rating_metadata::xmp_rating(br#"<rdf:Description xmp:Rating="5"/>"#).unwrap(), Some(5));
    assert_eq!(rating_metadata::xmp_rating(br#"<xmp:Rating>4</xmp:Rating>"#).unwrap(), Some(4));
}

#[test]
fn accepts_rejected_and_absent_external_states() {
    assert_eq!(rating_metadata::xmp_rating(br#"<xmp:Rating>-1</xmp:Rating>"#).unwrap(), Some(-1));
    assert_eq!(rating_metadata::xmp_rating(b"<x:xmpmeta/>").unwrap(), None);
}

#[test]
fn rejects_invalid_or_duplicate_ratings() {
    assert!(rating_metadata::xmp_rating(br#"<xmp:Rating>9</xmp:Rating>"#).is_err());
    assert!(rating_metadata::xmp_rating(br#"<xmp:Rating>4</xmp:Rating><xmp:Rating>5</xmp:Rating>"#).is_err());
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rating_metadata --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: FAIL because `src/rating_metadata.rs` does not exist.

- [ ] **Step 3: Move parsing into the focused module**

Move `parse_rating()` and `xmp_rating()` from `reference.rs` into `rating_metadata.rs`. Keep the current `quick_xml::Reader` implementation, the `-1..=5` validation, duplicate detection, and Chinese errors. Export only:

```rust
pub(crate) fn xmp_rating(input: &[u8]) -> Result<Option<i8>, String>
```

Declare `mod rating_metadata;` in `lib.rs`. Replace `reference.rs` calls with `crate::rating_metadata::xmp_rating(&input)` and remove its duplicate parser.

- [ ] **Step 4: Run metadata and reference tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test rating_metadata --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
cargo test --manifest-path src-tauri/Cargo.toml reference::tests --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: PASS with identical XMP reference-source behavior.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/rating_metadata.rs src-tauri/tests/rating_metadata.rs src-tauri/src/reference.rs src-tauri/src/lib.rs
git commit -m "refactor: share XMP rating parsing"
```

---

### Task 3: Extract The Shared Photo Group Index

**Files:**
- Create: `src-tauri/src/photo_groups.rs`
- Create: `src-tauri/tests/photo_groups.rs`
- Modify: `src-tauri/src/preview.rs:1-183`
- Modify: `src-tauri/src/lib.rs:1-8,893-915`
- Modify: `src-tauri/tests/preview_index.rs:1-96`

- [ ] **Step 1: Write failing group-shape tests**

Create `src-tauri/tests/photo_groups.rs` with explicit module paths and define the intended domain shape:

```rust
#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/rating_metadata.rs"]
mod rating_metadata;
#[path = "../src/photo_groups.rs"]
mod photo_groups;

#[test]
fn groups_jpeg_raw_and_sidecar_members_by_relative_stem() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    std::fs::create_dir_all(root.join("day")).unwrap();
    std::fs::write(root.join("day/A.JPG"), b"jpeg").unwrap();
    std::fs::write(root.join("day/A.NEF"), b"raw").unwrap();
    std::fs::write(root.join("day/A.xmp"), br#"<xmp:Rating>4</xmp:Rating>"#).unwrap();

    let index = photo_groups::index_directory(&root).unwrap();
    let group = &index.assets[0];
    assert_eq!(group.id, "day/a");
    assert_eq!(group.jpeg_paths, ["day/A.JPG"]);
    assert_eq!(group.raw_paths, ["day/A.NEF"]);
    assert_eq!(group.xmp_paths, ["day/A.xmp"]);
    assert_eq!(group.members.len(), 3);
}
```

Add a second test proving `A.NEF.xmp` joins `A` while retaining its exact path.

- [ ] **Step 2: Run and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test photo_groups --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: FAIL because `photo_groups.rs` and the new fields do not exist.

- [ ] **Step 3: Define the shared structs**

Create serializable structs in `photo_groups.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PhotoMemberKind { Jpeg, Raw, Xmp }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhotoMemberSnapshot {
    pub(crate) kind: PhotoMemberKind,
    pub(crate) relative_path: String,
    pub(crate) size_bytes: u64,
    pub(crate) modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingState {
    pub(crate) frame_pair: u8,
    pub(crate) jpeg_metadata: Option<i8>,
    pub(crate) raw_xmp: Option<i8>,
    pub(crate) resolved: u8,
    pub(crate) conflict: bool,
}
```

Move `PhotoAsset`, `PhotoIndex`, the builder, `now_ms`, `modified_ms`, `display_path`, `extension_label`, `finalize_asset`, and `index_directory` from `preview.rs`. Extend `PhotoAsset` with:

```rust
pub(crate) xmp_paths: Vec<String>,
pub(crate) members: Vec<PhotoMemberSnapshot>,
pub(crate) rating_state: RatingState,
pub(crate) rating_issues: Vec<String>,
```

Keep `rating: u8`, `jpeg_paths`, and `raw_paths` for frontend compatibility. Build image groups in a first pass, collect XMP paths separately, then use `formats::sidecar_match_keys(relative, false)` to attach each sidecar once to an existing image-group key. Multiple valid target keys are recorded as ambiguity. Build members from all exact paths. Preserve the legacy `size_bytes` as JPG/RAW combination size so the current UI does not change; XMP size remains available on its member snapshot for later plans.

- [ ] **Step 4: Rewire preview ownership**

Declare `mod photo_groups;` in `lib.rs`. Change `index_photo_directory` to return `photo_groups::PhotoIndex` and call `photo_groups::index_directory()`.

Remove grouping code from `preview.rs`; retain preview validation and thumbnail code. Update `preview_index.rs` to import `photo_groups.rs`, call it for grouping tests, and keep `preview.rs` calls only for thumbnail/path tests.

- [ ] **Step 5: Run photo-group and preview tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test photo_groups --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
cargo test --manifest-path src-tauri/Cargo.toml --test preview_index --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: PASS. Existing JPG/RAW counts and thumbnail behavior remain unchanged; new XMP member assertions pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/photo_groups.rs src-tauri/src/preview.rs src-tauri/src/lib.rs src-tauri/tests/photo_groups.rs src-tauri/tests/preview_index.rs
git commit -m "refactor: introduce shared photo groups"
```

---

### Task 4: Read External Rating Sources Without Writing Files

**Files:**
- Modify: `src-tauri/src/rating_metadata.rs`
- Modify: `src-tauri/src/photo_groups.rs`
- Modify: `src-tauri/tests/rating_metadata.rs`
- Modify: `src-tauri/tests/photo_groups.rs`

- [ ] **Step 1: Add failing bounded-file and JPEG XMP tests**

Add a helper in the test that inserts a standard XMP APP1 segment after JPEG SOI:

```rust
fn add_xmp(path: &Path, xml: &[u8]) {
    const PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let mut jpeg = std::fs::read(path).unwrap();
    let mut payload = PREFIX.to_vec();
    payload.extend_from_slice(xml);
    let length = u16::try_from(payload.len() + 2).unwrap().to_be_bytes();
    let mut app1 = vec![0xff, 0xe1, length[0], length[1]];
    app1.extend_from_slice(&payload);
    jpeg.splice(2..2, app1);
    std::fs::write(path, jpeg).unwrap();
}
```

Test `read_jpeg_rating()` returns 4 from embedded XMP and `read_sidecar_rating()` rejects a symlink, malformed XML, or a file larger than 4 MiB.

- [ ] **Step 2: Run metadata tests and verify RED**

Run the `rating_metadata` integration test. Expected: FAIL because file and JPEG readers do not exist.

- [ ] **Step 3: Implement bounded read-only metadata adapters**

In `rating_metadata.rs`, add:

```rust
const MAX_XMP_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) fn read_sidecar_rating(path: &Path) -> Result<Option<i8>, String>;
pub(crate) fn read_jpeg_rating(path: &Path) -> Result<Option<i8>, String>;
```

`read_sidecar_rating()` must use `symlink_metadata`, reject symlinks/non-files/oversized files, read bytes, then call `xmp_rating()`.

`read_jpeg_rating()` must open `ImageReader`, guess the format, create the decoder, call `decoder.xmp_metadata()`, enforce the same 4 MiB limit, and parse returned XML with `xmp_rating()`. It must not decode pixels or modify the JPEG.

- [ ] **Step 4: Add failing rating-state tests**

Extend `photo_groups` tests:

```rust
#[test]
fn reports_external_ratings_without_changing_the_resolved_framepair_score() {
    // A.JPG embeds 4 stars and A.xmp stores 5 stars.
    let group = &photo_groups::index_directory(&root).unwrap().assets[0];
    assert_eq!(group.rating_state.jpeg_metadata, Some(4));
    assert_eq!(group.rating_state.raw_xmp, Some(5));
    assert_eq!(group.rating_state.frame_pair, 0);
    assert_eq!(group.rating_state.resolved, 0);
    assert!(group.rating_state.conflict);
    assert_eq!(group.rating, 0);
}
```

Add tests for equal external ratings, duplicate XMP paths, malformed XMP, and external `-1`. Equal valid sources do not conflict; duplicate targets, parse errors, and `-1` add `rating_issues` and set conflict.

- [ ] **Step 5: Aggregate source state in photo groups**

For each group:

- Read all JPG embedded ratings and all attached XMP ratings.
- Collapse equal values to one source value.
- Preserve parse/ambiguity errors in `rating_issues` rather than failing the whole directory index.
- Set `conflict` when valid non-empty source values disagree, when duplicate writable XMP targets exist, when `-1` appears, or when any source cannot be parsed safely.
- Leave `frame_pair`, `resolved`, and scalar `rating` at zero until the FramePair database overlay runs.

- [ ] **Step 6: Run metadata and group tests**

Run both integration tests. Expected: PASS with no file writes.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/rating_metadata.rs src-tauri/src/photo_groups.rs src-tauri/tests/rating_metadata.rs src-tauri/tests/photo_groups.rs
git commit -m "feat: read photo rating sources"
```

---

### Task 5: Overlay FramePair Ratings And Keep Frontend State Consistent

**Files:**
- Modify: `src-tauri/src/photo_groups.rs`
- Modify: `src-tauri/src/lib.rs:893-915`
- Modify: `src-tauri/tests/photo_ratings.rs`
- Modify: `src/features/preview/types.ts:5-26`
- Modify: `src/features/preview/previewUtils.ts`
- Modify: `src/features/preview/PreviewModule.tsx:143-151,431-451`
- Modify: `tests/frontend-utils.test.mjs`

- [ ] **Step 1: Write a failing backend overlay test**

Add a pure overlay test:

```rust
#[test]
fn framepair_overlay_updates_legacy_and_structured_rating_fields() {
    let mut index = photo_groups::index_directory(&root).unwrap();
    let ratings = HashMap::from([("day/a".to_string(), 4)]);
    photo_groups::apply_framepair_ratings(&mut index, &ratings);
    let group = &index.assets[0];
    assert_eq!(group.rating, 4);
    assert_eq!(group.rating_state.frame_pair, 4);
    assert_eq!(group.rating_state.resolved, 4);
}
```

Also assert an external 5-star value produces `conflict: true`, while an equal external 4-star value does not.

- [ ] **Step 2: Verify backend RED and implement overlay**

Add:

```rust
pub(crate) fn apply_framepair_ratings(
    index: &mut PhotoIndex,
    ratings: &HashMap<String, u8>,
) {
    for group in &mut index.assets {
        let rating = ratings.get(&group.id).copied().unwrap_or_default();
        group.rating = rating;
        group.rating_state.frame_pair = rating;
        group.rating_state.resolved = rating;
        group.rating_state.conflict = calculate_rating_conflict(&group.rating_state)
            || !group.rating_issues.is_empty();
    }
}
```

Update `index_photo_directory` in `lib.rs` to call this function instead of assigning `asset.rating` directly.

- [ ] **Step 3: Write a failing frontend consistency test**

Extend test fixtures with `xmpPaths`, `members`, `ratingIssues`, and `ratingState`, then test:

```js
test("optimistic ratings update scalar and structured FramePair state", () => {
  const updated = previewUtils.withFramePairRating(previewAssets[1], 5);
  assert.equal(updated.rating, 5);
  assert.equal(updated.ratingState.framePair, 5);
  assert.equal(updated.ratingState.resolved, 5);
  assert.equal(updated.ratingState.rawXmp, null);
});
```

- [ ] **Step 4: Add frontend types and pure helper**

Add:

```ts
export type PhotoMemberKind = "jpeg" | "raw" | "xmp";

export interface PhotoMemberSnapshot {
  kind: PhotoMemberKind;
  relativePath: string;
  sizeBytes: number;
  modifiedMs: number | null;
}

export interface RatingState {
  framePair: number;
  jpegMetadata: number | null;
  rawXmp: number | null;
  resolved: number;
  conflict: boolean;
}
```

Extend `PhotoAsset` with `xmpPaths`, `members`, `ratingState`, and `ratingIssues`.

Implement `withFramePairRating(asset, rating)` as an immutable helper that updates `rating`, `ratingState.framePair`, and `ratingState.resolved`. Recalculate conflict against available external values without applying a conflict resolution policy.

- [ ] **Step 5: Use the helper in optimistic rendering**

Change `ratedAssets` in `PreviewModule.tsx` to call `withFramePairRating()` instead of spreading only the scalar `rating`. Keep the current ratings map, filter logic, badges, loupe text, keyboard shortcuts, and save API unchanged.

- [ ] **Step 6: Run frontend and backend focused tests**

Run:

```bash
npm run test:frontend
cargo test --manifest-path src-tauri/Cargo.toml --test photo_ratings --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
cargo test --manifest-path src-tauri/Cargo.toml --test photo_groups --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: PASS. Current preview rating and filtering behavior remains scalar-compatible.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/photo_groups.rs src-tauri/src/lib.rs src-tauri/tests/photo_ratings.rs src/features/preview/types.ts src/features/preview/previewUtils.ts src/features/preview/PreviewModule.tsx tests/frontend-utils.test.mjs
git commit -m "feat: expose structured photo ratings"
```

---

### Task 6: Document And Verify Phase One

**Files:**
- Modify: `docs/TECHNICAL-SOLUTION.md`

- [ ] **Step 1: Document the shared read-only model**

Add a section explaining:

```markdown
## Shared photo groups and rating sources

- A photo group is keyed by relative path without the final media extension.
- JPG, RAW, `photo.xmp`, and `photo.RAW.xmp` remain exact members of the group.
- FramePair, JPG XMP, and RAW XMP ratings are read separately.
- Phase one never writes external metadata and keeps FramePair as the resolved preview score.
- Conflicts and malformed metadata are reported per group without aborting the directory index.
```

- [ ] **Step 2: Run formatting and static checks**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
npm run build
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 3: Run the complete regression suite**

Run:

```bash
npm run test:frontend
cargo test --manifest-path src-tauri/Cargo.toml --config 'source.crates-io.replace-with="rsproxy"' --config 'source.rsproxy.registry="sparse+https://rsproxy.cn/index/"' --offline
```

Expected: all frontend, backend unit, backend integration, and doc tests pass.

- [ ] **Step 4: Run desktop smoke QA**

Start Vite and the compiled Tauri debug binary. Verify:

- Photo directory loads with JPG-only, RAW-only, paired, and XMP-containing groups.
- Existing star values, rating filter, keyboard rating, grid, loupe, preload, filmstrip sync, and both collapsible sidebars behave as before.
- Pair cleanup still scans the same fixture counts and does not expose rating-source members as new cleanup candidates.
- No relevant console errors, framework overlay, clipping, overlap, or page-level overflow at 1440x900 and 860x620.

- [ ] **Step 5: Commit documentation and any QA-only fixes**

```bash
git add docs/TECHNICAL-SOLUTION.md
git commit -m "docs: describe shared photo rating model"
```

- [ ] **Step 6: Push and wait for CI**

```bash
git push origin main
gh run list --commit "$(git rev-parse HEAD)" --limit 3
```

Expected: `main` is synchronized with `origin/main`; any triggered workflow completes successfully before phase one is declared complete.

---

## Phase-One Completion Gate

Do not begin rating writes or phase-two UI until all conditions hold:

- Existing preview and pair-cleanup behavior has no regression.
- One backend index exposes exact JPG/RAW/XMP members and structured source ratings.
- FramePair remains the only resolved preview score.
- Malformed or ambiguous external metadata is isolated to its group.
- Full frontend and Rust suites pass.
- Desktop smoke QA passes at both required viewports.
- The user reviews phase-one behavior before the phase-two plan is written.
