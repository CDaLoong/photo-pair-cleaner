import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import {
  PreviewUrlCache,
  preloadPreviewRequests,
} from "../src/features/preview/previewCache.ts";
import * as previewUtils from "../src/features/preview/previewUtils.ts";
import * as ratingRuleUtils from "../src/features/rating-rules/ratingRuleUtils.ts";
import * as ratingSyncUtils from "../src/features/rating-sync/ratingSyncUtils.ts";
import * as utils from "../src/utils.ts";

test("phase four registers rating organizer execution and recovery without cleanup execution", () => {
  const source = fs.readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  for (const command of [
    "get_rating_rules",
    "save_rating_rules",
    "import_rating_rules",
    "export_rating_rules",
    "generate_operation_plan",
    "execute_operation_plan",
    "list_rating_operation_history",
    "restore_rating_move",
    "restore_rating_quarantine",
    "undo_rating_copy",
  ]) {
    assert.match(source, new RegExp(`async fn ${command}\\b`));
    assert.match(source, new RegExp(`\\n\\s*${command},?`));
  }
  assert.doesNotMatch(source, /async fn execute_rating_cleanup\b/);
});

test("phase five keeps cleanup inside the authorized organizer command", () => {
  const plan = fs.readFileSync(new URL("../src-tauri/src/operation_plan.rs", import.meta.url), "utf8");
  const organizer = fs.readFileSync(new URL("../src-tauri/src/file_organizer.rs", import.meta.url), "utf8");
  assert.match(plan, /cleanup_destination: Option<CleanupExecutionDestination>/);
  assert.match(organizer, /CleanupExecutionDestination::Quarantine/);
  assert.match(organizer, /CleanupExecutionDestination::Trash/);
  assert.doesNotMatch(organizer, /pub\(crate\) fn execute_rating_cleanup/);
});

test("new rating rules use the agreed move, group, and path defaults", () => {
  const rule = ratingRuleUtils.createRatingRule("rule-1");
  assert.equal(rule.action, "move");
  assert.deepEqual(rule.memberScope, ["jpeg", "raw", "xmp"]);
  assert.equal(rule.preserveRelativePath, true);
  assert.equal(rule.destination, null);
  assert.deepEqual(rule.condition, { type: "equal", rating: 3 });
});

test("rating rule templates create editable drafts without destinations", () => {
  const id = () => "template-rule";
  const curated = ratingRuleUtils.rulesForTemplate("curatedArchive", id);
  assert.equal(curated[0].action, "move");
  assert.deepEqual(curated[0].condition, { type: "atLeast", rating: 4 });
  assert.equal(curated[0].destination, null);

  const cleanup = ratingRuleUtils.rulesForTemplate("lowRatingCleanup", id);
  assert.equal(cleanup[0].action, "cleanup");
  assert.deepEqual(cleanup[0].condition, { type: "atMost", rating: 2 });

  const backup = ratingRuleUtils.rulesForTemplate("backupAll", id);
  assert.equal(backup[0].action, "copy");
  assert.deepEqual(backup[0].condition, { type: "between", minimum: 0, maximum: 5 });
  assert.equal(backup[0].destination, null);
  assert.deepEqual(ratingRuleUtils.rulesForTemplate("custom", id), []);
});

test("rating rule draft validation explains the first actionable problem", () => {
  assert.deepEqual(ratingRuleUtils.validateRatingRuleDrafts([]), {
    valid: false,
    message: "请至少创建一条评分规则",
  });
  const move = ratingRuleUtils.createRatingRule("move");
  assert.deepEqual(ratingRuleUtils.validateRatingRuleDrafts([move]), {
    valid: false,
    message: "规则“自定义规则”必须选择目标目录",
  });
  const cleanup = { ...move, id: "cleanup", action: "cleanup", destination: null };
  assert.deepEqual(ratingRuleUtils.validateRatingRuleDrafts([cleanup]), { valid: true });
  assert.deepEqual(ratingRuleUtils.validateRatingRuleDrafts([cleanup, { ...cleanup }]), {
    valid: false,
    message: "规则 ID 重复：cleanup",
  });
});

test("operation plan filters and Chinese labels remain stable", () => {
  const items = [
    { groupId: "a", terminalAction: "move", status: "ready", syncActions: [] },
    { groupId: "b", terminalAction: "cleanup", status: "ready", syncActions: [] },
    { groupId: "c", terminalAction: null, status: "conflict", syncActions: [] },
    { groupId: "d", terminalAction: "keep", status: "keep", syncActions: [{ target: "rawXmp" }] },
  ];
  assert.deepEqual(
    ratingRuleUtils.filterOperationPlanItems(items, "cleanup").map((item) => item.groupId),
    ["b"],
  );
  assert.deepEqual(
    ratingRuleUtils.filterOperationPlanItems(items, "sync").map((item) => item.groupId),
    ["d"],
  );
  assert.deepEqual(
    ratingRuleUtils.filterOperationPlanItems(items, "conflict").map((item) => item.groupId),
    ["c"],
  );
  assert.equal(ratingRuleUtils.ruleActionLabel("move"), "移动");
  assert.equal(ratingRuleUtils.operationStatusLabel("conflict"), "存在冲突");
  assert.equal(ratingRuleUtils.isExecutablePlanItem(items[0]), true);
  assert.equal(ratingRuleUtils.isExecutablePlanItem(items[1]), false);
  assert.equal(ratingRuleUtils.isExecutablePlanItem(items[2]), false);
  assert.deepEqual(ratingRuleUtils.defaultExecutableGroupIds(items), ["a"]);
  assert.equal(ratingRuleUtils.organizerGroupStatusLabel("partial"), "部分完成");
});

test("operation selection summary counts only selected executable groups", () => {
  const items = [
    {
      groupId: "copy",
      terminalAction: "copy",
      status: "ready",
      members: [{ sizeBytes: 10 }, { sizeBytes: 5 }],
    },
    {
      groupId: "move",
      terminalAction: "move",
      status: "ready",
      members: [{ sizeBytes: 20 }],
    },
    {
      groupId: "cleanup",
      terminalAction: "cleanup",
      status: "ready",
      members: [{ sizeBytes: 99 }],
    },
  ];
  assert.deepEqual(
    ratingRuleUtils.operationSelectionSummary(items, new Set(["copy", "cleanup"])),
    { groups: 1, copyGroups: 1, moveGroups: 0, files: 2, bytes: 15 },
  );
});

test("rating organization UI executes only copy and move plans", () => {
  const selector = fs.readFileSync(new URL("../src/features/cleanup/TaskTypeSelector.tsx", import.meta.url), "utf8");
  const workspacePath = new URL("../src/features/rating-rules/RatingRulesWorkspace.tsx", import.meta.url);
  assert.equal(fs.existsSync(workspacePath), true);
  const workspace = fs.readFileSync(workspacePath, "utf8");
  assert.match(selector, /评分整理/);
  const review = fs.readFileSync(new URL("../src/features/rating-rules/OperationPlanReview.tsx", import.meta.url), "utf8");
  assert.match(workspace, /execute_operation_plan/);
  assert.match(workspace, /list_rating_operation_history/);
  assert.match(review, /执行所选/);
  assert.match(review, /第五阶段开放/);
  assert.match(workspace, /OperationExecuteDialog/);
  assert.match(workspace, /OperationHistoryPanel/);
});

test("sidebar preferences collapse only when storage explicitly says true", () => {
  assert.equal(utils.storedBooleanPreference("true"), true);
  assert.equal(utils.storedBooleanPreference("false"), false);
  assert.equal(utils.storedBooleanPreference(null), false);
  assert.equal(utils.storedBooleanPreference("invalid"), false);
});

const candidates = [
  {
    id: "raw:1",
    matchStatus: "unmatched",
    kind: "raw",
    sizeBytes: 12,
    matchedPath: null,
  },
  {
    id: "sidecar:1",
    matchStatus: "unmatched",
    kind: "sidecar",
    sizeBytes: 3,
    matchedPath: null,
  },
  {
    id: "raw:2",
    matchStatus: "matched",
    kind: "raw",
    sizeBytes: 100,
    matchedPath: "day/one.JPG",
  },
];

test("reclaimable bytes exclude sidecars when the option is disabled", () => {
  assert.equal(typeof utils.reclaimableBytes, "function");
  assert.equal(utils.reclaimableBytes(candidates, false), 12);
});

test("reclaimable bytes include sidecars when the option is enabled", () => {
  assert.equal(typeof utils.reclaimableBytes, "function");
  assert.equal(utils.reclaimableBytes(candidates, true), 15);
});

test("a rescan failure keeps the completed deletion result", () => {
  assert.equal(typeof utils.noticeAfterRescanFailure, "function");
  assert.deepEqual(
    utils.noticeAfterRescanFailure(
      { tone: "success", title: "2 个文件已移入回收站/废纸篓" },
      "目录不可访问",
    ),
    {
      tone: "success",
      title: "2 个文件已移入回收站/废纸篓",
      detail: "清理已执行，但自动重新扫描失败：目录不可访问",
    },
  );
});

test("cleanable items hide sidecars when the option is disabled", () => {
  assert.deepEqual(
    utils.cleanableItems(candidates, false).map((item) => item.id),
    ["raw:1"],
  );
  assert.deepEqual(
    utils.cleanableItems(candidates, true).map((item) => item.id),
    ["raw:1", "sidecar:1"],
  );
});

test("selection breakdown distinguishes RAW and sidecar files", () => {
  assert.deepEqual(utils.selectionBreakdown(candidates.slice(0, 2)), {
    raw: 1,
    sidecar: 1,
    total: 2,
  });
});

test("decision reason explains keep, delete, and sidecar outcomes", () => {
  assert.equal(utils.decisionReason(candidates[2]), "匹配 JPG：day/one.JPG");
  assert.equal(utils.decisionReason(candidates[0]), "未找到同路径同名 JPG");
  assert.equal(utils.decisionReason(candidates[1]), "跟随未配对 RAW 处理");
});

test("duplicate reference keys block destructive execution", () => {
  assert.equal(utils.scanHasBlockingIssues(null), false);
  assert.equal(utils.scanHasBlockingIssues({ mode: "cleanupRaw", duplicateReferenceKeys: 0 }), false);
  assert.equal(utils.scanHasBlockingIssues({ mode: "cleanupRaw", duplicateReferenceKeys: 2 }), true);
  assert.equal(utils.scanHasBlockingIssues({ mode: "auditReference", duplicateReferenceKeys: 2 }), false);
});

test("directory drop coordinates select only the hovered target", () => {
  const bounds = [
    { kind: "reference", left: 10, top: 20, right: 210, bottom: 120 },
    { kind: "raw", left: 250, top: 20, right: 450, bottom: 120 },
  ];

  assert.equal(utils.directoryDropTargetAtPoint(100, 60, bounds), "reference");
  assert.equal(utils.directoryDropTargetAtPoint(300, 60, bounds), "raw");
  assert.equal(utils.directoryDropTargetAtPoint(230, 60, bounds), null);
  assert.equal(utils.directoryDropTargetAtPoint(100, 140, bounds), null);
});

test("raw format counts are grouped case-insensitively", () => {
  const items = [
    { kind: "raw", extension: ".NEF" },
    { kind: "raw", extension: ".nef" },
    { kind: "raw", extension: ".CR3" },
    { kind: "sidecar", extension: ".xmp" },
  ];

  assert.deepEqual(utils.rawFormatCounts(items), { NEF: 2, CR3: 1 });
});

test("cleanup destination copy names the selected operation", () => {
  assert.equal(utils.cleanupActionLabel("trash"), "移入系统回收站");
  assert.equal(utils.cleanupActionLabel("quarantine"), "移入 FramePair 隔离区");
});

test("only unmatched cleanup items are actionable", () => {
  assert.equal(
    utils.isActionableItem({ kind: "raw", matchStatus: "unmatched" }, "cleanupRaw"),
    true,
  );
  assert.equal(
    utils.isActionableItem({ kind: "raw", matchStatus: "matched" }, "cleanupRaw"),
    false,
  );
  assert.equal(
    utils.isActionableItem({ kind: "reference", matchStatus: "unmatched" }, "auditReference"),
    false,
  );
});

test("reverse audit is available only for a directory reference source", () => {
  assert.equal(utils.canAuditReferenceSource("directory"), true);
  assert.equal(utils.canAuditReferenceSource("manifest"), false);
  assert.equal(utils.canAuditReferenceSource("xmpRating"), false);
});

const previewAssets = [
  {
    id: "day/b",
    name: "B",
    relativeStem: "day/B",
    previewPath: null,
    jpegPaths: [],
    rawPaths: ["day/B.NEF"],
    xmpPaths: [],
    members: [],
    extensions: ["NEF"],
    sizeBytes: 20,
    modifiedMs: 20,
    rating: 0,
    ratingState: {
      framePair: 0,
      jpegMetadata: null,
      rawXmp: null,
      resolved: 0,
      conflict: false,
    },
    ratingIssues: [],
  },
  {
    id: "day/a",
    name: "A",
    relativeStem: "day/A",
    previewPath: "day/A.JPG",
    jpegPaths: ["day/A.JPG"],
    rawPaths: ["day/A.CR3"],
    xmpPaths: ["day/A.xmp"],
    members: [],
    extensions: ["JPG", "CR3"],
    sizeBytes: 10,
    modifiedMs: 10,
    rating: 4,
    ratingState: {
      framePair: 4,
      jpegMetadata: 4,
      rawXmp: null,
      resolved: 4,
      conflict: false,
    },
    ratingIssues: [],
  },
  {
    id: "other/c",
    name: "C",
    relativeStem: "other/C",
    previewPath: "other/C.jpeg",
    jpegPaths: ["other/C.jpeg"],
    rawPaths: [],
    xmpPaths: [],
    members: [],
    extensions: ["JPEG"],
    sizeBytes: 30,
    modifiedMs: 30,
    rating: 2,
    ratingState: {
      framePair: 2,
      jpegMetadata: null,
      rawXmp: null,
      resolved: 2,
      conflict: false,
    },
    ratingIssues: [],
  },
];

const directoryAssets = [
  {
    ...previewAssets[2],
    id: "root",
    name: "Root",
    relativeStem: "Root",
  },
  {
    ...previewAssets[1],
    id: "2026/a",
    name: "A",
    relativeStem: "2026/A",
  },
  {
    ...previewAssets[2],
    id: "2026/trip/b",
    name: "B",
    relativeStem: "2026/trip/B",
  },
  {
    ...previewAssets[2],
    id: "20260/c",
    name: "C",
    relativeStem: "20260/C",
  },
];

test("photo directory tree preserves nesting and recursive photo counts", () => {
  const tree = previewUtils.buildPhotoDirectoryTree(directoryAssets);

  assert.deepEqual(tree.map((node) => [node.path, node.totalCount]), [
    ["2026", 2],
    ["20260", 1],
  ]);
  assert.equal(tree[0].directCount, 1);
  assert.deepEqual(tree[0].children.map((node) => [node.path, node.totalCount]), [
    ["2026/trip", 1],
  ]);
});

test("directory filtering includes descendants without matching sibling prefixes", () => {
  assert.deepEqual(
    previewUtils.filterAssetsByDirectory(directoryAssets, "2026").map((item) => item.id),
    ["2026/a", "2026/trip/b"],
  );
  assert.deepEqual(
    previewUtils.filterAssetsByDirectory(directoryAssets, "2026/trip").map((item) => item.id),
    ["2026/trip/b"],
  );
  assert.equal(previewUtils.filterAssetsByDirectory(directoryAssets, "").length, 4);
});

test("preview filter counts explain paired and single-format groups", () => {
  const counts = previewUtils.previewFilterCounts(previewAssets);
  assert.deepEqual(counts, {
    all: 3,
    paired: 1,
    jpeg: 1,
    raw: 1,
  });
  assert.equal(previewUtils.availablePreviewFilter("paired", counts), "paired");
  assert.equal(
    previewUtils.availablePreviewFilter("paired", { ...counts, paired: 0 }),
    "all",
  );
});

test("preview guide opens until the completed preference is stored", () => {
  assert.equal(previewUtils.shouldOpenPreviewGuide(null), true);
  assert.equal(previewUtils.shouldOpenPreviewGuide("false"), true);
  assert.equal(previewUtils.shouldOpenPreviewGuide("true"), false);
});

test("preview keyboard shortcuts pause while an overlay is open", () => {
  assert.equal(previewUtils.previewKeyboardShortcutsEnabled("loupe", false, false), true);
  assert.equal(previewUtils.previewKeyboardShortcutsEnabled("loupe", true, false), false);
  assert.equal(previewUtils.previewKeyboardShortcutsEnabled("loupe", false, true), false);
  assert.equal(previewUtils.previewKeyboardShortcutsEnabled("grid", false, false), false);
});

test("context menu position stays inside the viewport", () => {
  assert.deepEqual(
    previewUtils.contextMenuPosition(790, 590, 800, 600, 260, 220),
    { left: 532, top: 372 },
  );
  assert.deepEqual(
    previewUtils.contextMenuPosition(120, 80, 800, 600, 260, 220),
    { left: 120, top: 80 },
  );
});

test("preview filters distinguish paired, JPEG-only, and RAW-only photos", () => {
  assert.deepEqual(
    previewUtils.filterPreviewAssets(previewAssets, "paired", "").map((item) => item.id),
    ["day/a"],
  );
  assert.deepEqual(
    previewUtils.filterPreviewAssets(previewAssets, "jpeg", "").map((item) => item.id),
    ["other/c"],
  );
  assert.deepEqual(
    previewUtils.filterPreviewAssets(previewAssets, "raw", "").map((item) => item.id),
    ["day/b"],
  );
});

test("preview search matches names and relative folders case-insensitively", () => {
  assert.deepEqual(
    previewUtils.filterPreviewAssets(previewAssets, "all", "DAY/a").map((item) => item.id),
    ["day/a"],
  );
  assert.deepEqual(
    previewUtils.filterPreviewAssets(previewAssets, "all", "other").map((item) => item.id),
    ["other/c"],
  );
});

test("preview rating filter combines with type and search filters", () => {
  assert.deepEqual(
    previewUtils.filterPreviewAssets(previewAssets, "all", "", 3).map((item) => item.id),
    ["day/a"],
  );
  assert.deepEqual(
    previewUtils.filterPreviewAssets(previewAssets, "jpeg", "other", 2).map((item) => item.id),
    ["other/c"],
  );
  assert.deepEqual(
    previewUtils.filterPreviewAssets(previewAssets, "all", "", 5).map((item) => item.id),
    [],
  );
});

test("optimistic ratings update scalar and structured FramePair state", () => {
  const updated = previewUtils.withFramePairRating(previewAssets[1], 5);
  assert.equal(updated.rating, 5);
  assert.equal(updated.ratingState.framePair, 5);
  assert.equal(updated.ratingState.resolved, 5);
  assert.equal(updated.ratingState.jpegMetadata, 4);
  assert.equal(updated.ratingState.rawXmp, null);
  assert.equal(updated.ratingState.conflict, true);
});

test("rating sync copy makes the automatic safety boundary explicit", () => {
  assert.equal(
    ratingSyncUtils.syncModeNotice("automatic"),
    "自动同步只更新评分元数据，不会复制、移动或清理照片。",
  );
  assert.equal(
    ratingSyncUtils.syncModeNotice("manual"),
    "手动同步会先生成只读计划，确认后才更新评分元数据。",
  );
});

test("rating sync targets require a destination and explicit JPG confirmation", () => {
  assert.deepEqual(
    ratingSyncUtils.validateSyncTargets(
      { rawXmp: false, jpegMetadata: false },
      false,
    ),
    { valid: false, message: "请至少选择一个评分同步目标" },
  );
  assert.deepEqual(
    ratingSyncUtils.validateSyncTargets(
      { rawXmp: false, jpegMetadata: true },
      false,
    ),
    { valid: false, message: "请先确认允许 FramePair 修改 JPG 内嵌评分元数据" },
  );
  assert.deepEqual(
    ratingSyncUtils.validateSyncTargets(
      { rawXmp: true, jpegMetadata: false },
      false,
    ),
    { valid: true },
  );
});

test("automatic sync outcomes produce non-blocking Chinese notices", () => {
  assert.deepEqual(
    ratingSyncUtils.autoSyncOutcomeNotice({ status: "synced", message: null }),
    { tone: "success", title: "评分已自动同步" },
  );
  assert.deepEqual(
    ratingSyncUtils.autoSyncOutcomeNotice({ status: "pending", message: "XMP 文件只读" }),
    {
      tone: "warning",
      title: "FramePair 评分已保存，外部同步待处理",
      detail: "XMP 文件只读",
    },
  );
  assert.equal(
    ratingSyncUtils.autoSyncOutcomeNotice({ status: "disabled", message: null }),
    null,
  );
});

test("rating sync plan status labels are concise and actionable", () => {
  assert.equal(ratingSyncUtils.syncStatusLabel("ready"), "待同步");
  assert.equal(ratingSyncUtils.syncStatusLabel("unchanged"), "已一致");
  assert.equal(ratingSyncUtils.syncStatusLabel("conflict"), "存在冲突");
});

test("cleanup module defaults to pair cleanup and rating ranges stay bounded", () => {
  assert.equal(ratingSyncUtils.defaultCleanupTaskType(), "pairCleanup");
  assert.deepEqual(ratingSyncUtils.validateRatingRange(0, 5), { valid: true });
  assert.deepEqual(ratingSyncUtils.validateRatingRange(5, 2), {
    valid: false,
    message: "最低评分不能高于最高评分",
  });
  assert.deepEqual(ratingSyncUtils.validateRatingRange(-1, 5), {
    valid: false,
    message: "评分范围必须在 0 到 5 星之间",
  });
});

test("batch sync selects only executable plan items", () => {
  assert.deepEqual(
    ratingSyncUtils.readySyncAssetIds([
      { assetId: "a", status: "ready" },
      { assetId: "b", status: "unchanged" },
      { assetId: "c", status: "conflict" },
      { assetId: "d", status: "ready" },
    ]),
    ["a", "d"],
  );
});

test("preview sorting and keyboard selection are stable", () => {
  const sorted = previewUtils.sortPreviewAssets(previewAssets, "name");
  assert.deepEqual(sorted.map((item) => item.id), ["day/a", "day/b", "other/c"]);
  assert.equal(previewUtils.adjacentPreviewAssetId(sorted, "day/a", 1), "day/b");
  assert.equal(previewUtils.adjacentPreviewAssetId(sorted, "other/c", 1), "other/c");
  assert.equal(previewUtils.adjacentPreviewAssetId(sorted, "day/b", -1), "day/a");
});

test("preview position is one-based and handles a missing selection", () => {
  const sorted = previewUtils.sortPreviewAssets(previewAssets, "name");
  assert.equal(typeof previewUtils.previewAssetPosition, "function");
  assert.equal(previewUtils.previewAssetPosition(sorted, "day/a"), 1);
  assert.equal(previewUtils.previewAssetPosition(sorted, "day/b"), 2);
  assert.equal(previewUtils.previewAssetPosition(sorted, "missing"), 0);
  assert.equal(previewUtils.previewAssetPosition([], null), 0);
});

test("filmstrip scroll keeps the selected thumbnail inside the viewport", () => {
  assert.equal(typeof previewUtils.filmstripScrollTarget, "function");
  assert.equal(previewUtils.filmstripScrollTarget({
    scrollLeft: 240,
    clientWidth: 400,
    scrollWidth: 1000,
    itemOffsetLeft: 360,
    itemWidth: 88,
  }), 240);
  assert.equal(previewUtils.filmstripScrollTarget({
    scrollLeft: 240,
    clientWidth: 400,
    scrollWidth: 1000,
    itemOffsetLeft: 620,
    itemWidth: 88,
  }), 318);
  assert.equal(previewUtils.filmstripScrollTarget({
    scrollLeft: 240,
    clientWidth: 400,
    scrollWidth: 1000,
    itemOffsetLeft: 230,
    itemWidth: 88,
  }), 220);
  assert.equal(previewUtils.filmstripScrollTarget({
    scrollLeft: 500,
    clientWidth: 400,
    scrollWidth: 1000,
    itemOffsetLeft: 950,
    itemWidth: 88,
  }), 600);
});

const previewRequest = {
  root: "/photos",
  relativePath: "day/A.JPG",
  maxEdge: 1800,
  version: "10:10",
};

test("preview URL cache deduplicates concurrent and repeated loads", async () => {
  const cache = new PreviewUrlCache();
  let calls = 0;
  let resolveLoad;
  const loader = () => {
    calls += 1;
    return new Promise((resolve) => {
      resolveLoad = resolve;
    });
  };

  const first = cache.getOrLoad(previewRequest, loader);
  const second = cache.getOrLoad(previewRequest, loader);
  assert.equal(calls, 1);
  resolveLoad("blob:photo-a");

  assert.equal(await first, "blob:photo-a");
  assert.equal(await second, "blob:photo-a");
  assert.equal(await cache.getOrLoad(previewRequest, loader), "blob:photo-a");
  assert.equal(cache.peek(previewRequest), "blob:photo-a");
  assert.equal(calls, 1);
});

test("clearing a preview root releases URLs and permits a fresh load", async () => {
  const released = [];
  const cache = new PreviewUrlCache((url) => released.push(url));
  let calls = 0;
  const loader = async () => `blob:photo-${++calls}`;

  assert.equal(await cache.getOrLoad(previewRequest, loader), "blob:photo-1");
  cache.clearRoot("/photos");
  assert.deepEqual(released, ["blob:photo-1"]);
  assert.equal(cache.peek(previewRequest), null);
  assert.equal(await cache.getOrLoad(previewRequest, loader), "blob:photo-2");
  assert.equal(calls, 2);
});

test("preview preloader covers every request with bounded concurrency", async () => {
  const requests = Array.from({ length: 9 }, (_, index) => index);
  const progress = [];
  let active = 0;
  let maxActive = 0;

  const result = await preloadPreviewRequests(
    requests,
    async (request) => {
      active += 1;
      maxActive = Math.max(maxActive, active);
      await new Promise((resolve) => setTimeout(resolve, 5));
      active -= 1;
      if (request === 4) throw new Error("broken photo");
    },
    {
      concurrency: 3,
      onProgress: (value) => progress.push({ ...value }),
    },
  );

  assert.deepEqual(result, { total: 9, completed: 9, failed: 1 });
  assert.equal(maxActive, 3);
  assert.deepEqual(progress.at(-1), result);
});
