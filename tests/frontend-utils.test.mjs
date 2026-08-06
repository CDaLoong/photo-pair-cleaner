import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import {
  PreviewLoadScheduler,
  PreviewUrlCache,
  preloadPreviewRequests,
} from "../src/features/preview/previewCache.ts";
import * as previewUtils from "../src/features/preview/previewUtils.ts";
import * as ratingRuleUtils from "../src/features/rating-rules/ratingRuleUtils.ts";
import * as ratingSyncUtils from "../src/features/rating-sync/ratingSyncUtils.ts";
import * as utils from "../src/utils.ts";

test("phase five registers organizer execution and recovery without a path-based cleanup command", () => {
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
  assert.equal(ratingRuleUtils.isExecutablePlanItem(items[1]), true);
  assert.equal(ratingRuleUtils.isExecutablePlanItem(items[2]), false);
  assert.deepEqual(ratingRuleUtils.defaultExecutableGroupIds(items), ["a", "b"]);
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
    {
      groups: 2,
      copyGroups: 1,
      moveGroups: 0,
      cleanupGroups: 1,
      files: 3,
      bytes: 114,
      cleanupBytes: 99,
    },
  );
});

test("rating organization UI confirms copy move and cleanup plans", () => {
  const selector = fs.readFileSync(new URL("../src/features/cleanup/TaskTypeSelector.tsx", import.meta.url), "utf8");
  const workspacePath = new URL("../src/features/rating-rules/RatingRulesWorkspace.tsx", import.meta.url);
  assert.equal(fs.existsSync(workspacePath), true);
  const workspace = fs.readFileSync(workspacePath, "utf8");
  assert.match(selector, /评分整理/);
  const review = fs.readFileSync(new URL("../src/features/rating-rules/OperationPlanReview.tsx", import.meta.url), "utf8");
  assert.match(workspace, /execute_operation_plan/);
  assert.match(workspace, /list_rating_operation_history/);
  assert.match(review, /执行所选/);
  assert.doesNotMatch(review, /第五阶段开放/);
  assert.match(workspace, /OperationExecuteDialog/);
  assert.match(workspace, /OperationHistoryPanel/);
  assert.match(workspace, /cleanupDestination/);
  const dialog = fs.readFileSync(new URL("../src/features/rating-rules/OperationExecuteDialog.tsx", import.meta.url), "utf8");
  assert.match(dialog, /FramePair 隔离区/);
  assert.match(dialog, /系统回收站/);
  assert.match(dialog, /useState<CleanupExecutionDestination>\("quarantine"\)/);
});

test("rating cleanup history restores quarantine and only opens system trash", () => {
  const history = fs.readFileSync(new URL("../src/features/rating-rules/OperationHistoryPanel.tsx", import.meta.url), "utf8");
  const workspace = fs.readFileSync(new URL("../src/features/rating-rules/RatingRulesWorkspace.tsx", import.meta.url), "utf8");
  assert.match(history, /恢复隔离/);
  assert.match(history, /打开系统回收站/);
  assert.match(history, /restoreQuarantine/);
  assert.match(workspace, /restore_rating_quarantine/);
  assert.match(workspace, /open_system_trash/);
  assert.doesNotMatch(workspace, /restore_rating_trash/);
});

test("rating organizer guide follows the complete always-visible workflow", () => {
  const guide = fs.readFileSync(new URL("../src/features/cleanup/CleanupGuideDialog.tsx", import.meta.url), "utf8");
  const workspace = fs.readFileSync(new URL("../src/features/rating-rules/RatingRulesWorkspace.tsx", import.meta.url), "utf8");
  const history = fs.readFileSync(new URL("../src/features/rating-rules/OperationHistoryPanel.tsx", import.meta.url), "utf8");
  for (const target of [
    "rating-rules-root",
    "rating-rules-template",
    "rating-rules-editor",
    "rating-rules-sync",
    "rating-rules-command",
    "rating-rules-history",
  ]) {
    assert.match(guide, new RegExp(`data-tour='${target}'`));
  }
  assert.match(workspace, /data-tour="rating-rules-command"/);
  assert.match(history, /data-tour="rating-rules-history"/);
});

test("guided tours can center targets inside the rating organizer scroller", () => {
  const guide = fs.readFileSync(new URL("../src/components/GuidedTourDialog.tsx", import.meta.url), "utf8");
  assert.match(guide, /\.rating-rules-workspace/);
});

test("rating organizer empty state offers direct editable template starts", () => {
  const workspace = fs.readFileSync(new URL("../src/features/rating-rules/RatingRulesWorkspace.tsx", import.meta.url), "utf8");
  assert.match(workspace, /useState<RatingRuleTemplateId>\("curatedArchive"\)/);
  assert.match(workspace, /rating-rules-template-shortcuts/);
  assert.match(workspace, /从常用模板开始/);
  assert.match(workspace, /applyTemplate\(item\.id\)/);
  assert.match(workspace, /添加完全自定义规则/);
});

test("rating organizer header exposes history count and a real jump target", () => {
  const workspace = fs.readFileSync(new URL("../src/features/rating-rules/RatingRulesWorkspace.tsx", import.meta.url), "utf8");
  const history = fs.readFileSync(new URL("../src/features/rating-rules/OperationHistoryPanel.tsx", import.meta.url), "utf8");
  assert.match(workspace, /aria-controls="rating-rules-history"/);
  assert.match(workspace, /history\.length/);
  assert.match(workspace, /getElementById\("rating-rules-history"\)/);
  assert.match(history, /id="rating-rules-history"/);
  assert.match(history, /执行完成后，操作回执与可恢复入口会显示在这里/);
});

test("rating organizer defines a compact reflow for narrow and zoomed layouts", () => {
  const workspace = fs.readFileSync(new URL("../src/features/rating-rules/RatingRulesWorkspace.tsx", import.meta.url), "utf8");
  const styles = fs.readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
  assert.match(workspace, /rating-rules-history-link/);
  assert.match(styles, /@media \(max-width: 560px\)[\s\S]*\.rating-rules-file-actions[\s\S]*grid-template-columns: 38px 38px repeat\(2, minmax\(0, 1fr\)\)/);
  assert.match(styles, /@media \(max-width: 560px\)[\s\S]*\.rating-rule-name[\s\S]*grid-column: 1 \/ -1/);
  assert.match(styles, /@media \(max-width: 420px\)[\s\S]*\.operation-plan-summary[\s\S]*repeat\(2, minmax\(0, 1fr\)\)/);
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

test("single-photo previews select a bounded tier from actual device pixels", () => {
  assert.equal(previewUtils.displayPreviewEdge(800, 600, 1), 1600);
  assert.equal(previewUtils.displayPreviewEdge(900, 700, 2), 2560);
  assert.equal(previewUtils.displayPreviewEdge(1800, 1200, 2), 4096);
  assert.equal(previewUtils.displayPreviewEdge(5000, 3000, 2), 4096);
  assert.equal(previewUtils.displayPreviewEdge(Number.NaN, Number.NaN, Number.NaN), 1600);
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

test("nearby preview preloads follow the current browsing direction", () => {
  assert.deepEqual(previewUtils.previewPreloadOffsets(1), [1, 2, 3, -1]);
  assert.deepEqual(previewUtils.previewPreloadOffsets(-1), [-1, -2, -3, 1]);
  assert.deepEqual(previewUtils.previewPreloadOffsets(0), [1, -1]);
});

test("virtual filmstrip keeps a fixed DOM window for huge directories", () => {
  const window = previewUtils.virtualFilmstripWindow({
    itemCount: 10_000,
    itemPitch: 95,
    viewportWidth: 950,
    scrollLeft: 475_000,
    overscan: 5,
  });

  assert.equal(window.totalWidth, 950_000);
  assert.ok(window.startIndex > 0);
  assert.ok(window.endIndex - window.startIndex <= 20);
  assert.ok(window.startIndex <= 5_000);
  assert.ok(window.endIndex > 5_000);
});

test("virtual photo grid keeps a fixed DOM window for large directories", () => {
  const firstWindow = previewUtils.virtualPhotoGridWindow({
    itemCount: 1000,
    tileSize: 180,
    viewportWidth: 800,
    viewportHeight: 600,
    scrollTop: 0,
  });
  const scrolledWindow = previewUtils.virtualPhotoGridWindow({
    itemCount: 1000,
    tileSize: 180,
    viewportWidth: 800,
    viewportHeight: 600,
    scrollTop: 4700,
  });

  assert.equal(firstWindow.columns, 4);
  assert.equal(firstWindow.totalHeight, 46988);
  assert.equal(firstWindow.startIndex, 0);
  assert.equal(firstWindow.endIndex, 24);
  assert.ok(scrolledWindow.startIndex > 0);
  assert.ok(scrolledWindow.endIndex - scrolledWindow.startIndex <= 32);
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

test("preview cache retains visible leases and evicts old unpinned URLs", async () => {
  const released = [];
  const cache = new PreviewUrlCache((url) => released.push(url), 2);
  const request = (name) => ({ ...previewRequest, relativePath: name });

  const first = cache.acquire(request("A.JPG"), async () => "blob:a");
  const second = cache.acquire(request("B.JPG"), async () => "blob:b");
  await Promise.all([first.promise, second.promise]);
  first.release();

  const third = cache.acquire(request("C.JPG"), async () => "blob:c");
  await third.promise;

  assert.deepEqual(released, ["blob:a"]);
  assert.equal(cache.peek(request("A.JPG")), null);
  assert.equal(cache.peek(request("B.JPG")), "blob:b");
  assert.equal(cache.peek(request("C.JPG")), "blob:c");
  second.release();
  third.release();
});

test("preview cache evicts by estimated decoded bytes", async () => {
  const released = [];
  const cache = new PreviewUrlCache((url) => released.push(url), {
    maxEntries: 10,
    maxCostBytes: 70 * 1024 * 1024,
  });
  const large = { ...previewRequest, relativePath: "large.JPG", maxEdge: 4096 };
  const medium = { ...previewRequest, relativePath: "medium.JPG", maxEdge: 1600 };

  await cache.getOrLoad(large, async () => "blob:large");
  await cache.getOrLoad(medium, async () => "blob:medium");

  assert.deepEqual(released, ["blob:large"]);
  assert.equal(cache.peek(large), null);
  assert.equal(cache.peek(medium), "blob:medium");
  assert.ok(cache.estimatedCostBytes() <= 70 * 1024 * 1024);
});

test("releasing the last pending preview lease cancels abandoned work", async () => {
  const cache = new PreviewUrlCache();
  const lease = cache.acquire(previewRequest, (signal) => new Promise((resolve, reject) => {
    signal.addEventListener("abort", () => reject(new Error("cancelled")), { once: true });
  }));

  lease.release();

  await assert.rejects(lease.promise, /cancelled/);
  assert.equal(cache.peek(previewRequest), null);
});

test("preview scheduler prioritizes visible work without exceeding its limit", async () => {
  const scheduler = new PreviewLoadScheduler(1);
  const order = [];
  let releaseFirst;
  const first = scheduler.schedule(async () => {
    order.push("first");
    await new Promise((resolve) => {
      releaseFirst = resolve;
    });
  });
  const background = scheduler.schedule(async () => {
    order.push("background");
  });
  const foreground = scheduler.schedule(async () => {
    order.push("foreground");
  }, "foreground");

  await new Promise((resolve) => setTimeout(resolve, 0));
  releaseFirst();
  await Promise.all([first, background, foreground]);

  assert.deepEqual(order, ["first", "foreground", "background"]);
});

test("preview scheduler reserves capacity for foreground work", async () => {
  const scheduler = new PreviewLoadScheduler(3, 1);
  const started = [];
  const releases = [];
  const hold = (name) => scheduler.schedule(async () => {
    started.push(name);
    await new Promise((resolve) => releases.push(resolve));
  }, name.startsWith("foreground") ? "foreground" : "background");

  const work = [
    hold("background-1"),
    hold("background-2"),
    hold("foreground-1"),
    hold("foreground-2"),
  ];
  await new Promise((resolve) => setTimeout(resolve, 0));

  assert.deepEqual(started, ["background-1", "foreground-1", "foreground-2"]);
  for (const release of releases.splice(0)) release();
  await new Promise((resolve) => setTimeout(resolve, 0));
  for (const release of releases.splice(0)) release();
  await Promise.all(work);
});

test("aborting queued preview work rejects before an active job finishes", async () => {
  const scheduler = new PreviewLoadScheduler(1, 1);
  let releaseActive;
  let queuedRan = false;
  const active = scheduler.schedule(() => new Promise((resolve) => {
    releaseActive = resolve;
  }));
  const controller = new AbortController();
  const queued = scheduler.schedule(async () => {
    queuedRan = true;
  }, "background", controller.signal);

  controller.abort();
  const outcome = await Promise.race([
    queued.then(() => "resolved", () => "cancelled"),
    new Promise((resolve) => setTimeout(() => resolve("timeout"), 0)),
  ]);

  assert.equal(outcome, "cancelled");
  assert.equal(queuedRan, false);
  releaseActive();
  await active;
});

test("preview indexing streams progress and queues every display preview", () => {
  const moduleSource = fs.readFileSync(
    new URL("../src/features/preview/PreviewModule.tsx", import.meta.url),
    "utf8",
  );
  const thumbnailSource = fs.readFileSync(
    new URL("../src/features/preview/PhotoThumbnail.tsx", import.meta.url),
    "utf8",
  );
  const gridSource = fs.readFileSync(
    new URL("../src/features/preview/VirtualPhotoGrid.tsx", import.meta.url),
    "utf8",
  );
  const filmstripSource = fs.readFileSync(
    new URL("../src/features/preview/VirtualPhotoFilmstrip.tsx", import.meta.url),
    "utf8",
  );
  const cacheSource = fs.readFileSync(
    new URL("../src/features/preview/previewCache.ts", import.meta.url),
    "utf8",
  );
  const backendSource = fs.readFileSync(
    new URL("../src-tauri/src/lib.rs", import.meta.url),
    "utf8",
  );
  const previewBackendSource = fs.readFileSync(
    new URL("../src-tauri/src/preview.rs", import.meta.url),
    "utf8",
  );
  const previewCacheBackendSource = fs.readFileSync(
    new URL("../src-tauri/src/preview_cache.rs", import.meta.url),
    "utf8",
  );
  const nativePreviewSource = fs.readFileSync(
    new URL("../src-tauri/src/native_preview.rs", import.meta.url),
    "utf8",
  );
  const nativePreviewBridgeSource = fs.readFileSync(
    new URL("../src/features/preview/NativePhotoPreview.tsx", import.meta.url),
    "utf8",
  );
  const cargoSource = fs.readFileSync(
    new URL("../src-tauri/Cargo.toml", import.meta.url),
    "utf8",
  );
  const stylesheetSource = fs.readFileSync(
    new URL("../src/styles.css", import.meta.url),
    "utf8",
  );

  assert.match(moduleSource, /new Channel<PhotoIndexEvent>\(\)/);
  assert.match(moduleSource, /onEvent: channel/);
  assert.match(moduleSource, /preview-index-progress-track/);
  assert.match(moduleSource, /preloadPreviewRequests/);
  assert.doesNotMatch(moduleSource, /warmPhotoPreviewCache/);
  assert.match(moduleSource, /view !== "loupe"/);
  assert.match(moduleSource, /setPreloadingAssetIds\(new Set\(queue\.map/);
  assert.doesNotMatch(moduleSource, /previewPreloadOffsets\(direction\)/);
  assert.match(moduleSource, /new AbortController\(\)/);
  assert.match(moduleSource, /恢复上次照片目录超时，请重新选择目录/);
  assert.match(moduleSource, /loupePreviewEdge/);
  assert.match(moduleSource, /setLoupePreviewEdge/);
  assert.match(moduleSource, /photoPreviewRequest/);
  assert.match(moduleSource, /preloadPhotoPreviewUrl/);
  assert.match(moduleSource, /nativePreviewActive/);
  assert.doesNotMatch(moduleSource, /大图预加载进度/);
  assert.match(cacheSource, /warm_photo_thumbnail/);
  assert.doesNotMatch(cacheSource, /authorize_photo_original/);
  assert.match(backendSource, /warm_photo_thumbnail/);
  assert.match(backendSource, /async fn get_preview_cache_stats/);
  assert.match(backendSource, /async fn show_native_photo_preview/);
  assert.match(backendSource, /async fn hide_native_photo_preview/);
  assert.match(nativePreviewSource, /QLPreviewView/);
  assert.match(nativePreviewSource, /setPreviewItem/);
  assert.match(nativePreviewSource, /setMasksToBounds/);
  assert.match(nativePreviewSource, /removeFromSuperview/);
  assert.match(nativePreviewBridgeSource, /ResizeObserver/);
  assert.match(nativePreviewBridgeSource, /displayPreviewEdge/);
  assert.match(nativePreviewBridgeSource, /VITE_ENABLE_EMBEDDED_QUICK_LOOK/);
  assert.match(nativePreviewBridgeSource, /show_native_photo_preview/);
  assert.match(nativePreviewBridgeSource, /hide_native_photo_preview/);
  assert.match(previewBackendSource, /THUMBNAIL_CACHE_VERSION: u8 = 3/);
  assert.match(previewBackendSource, /PREVIEW_CACHE_BUDGET_BYTES/);
  assert.match(previewBackendSource, /PREVIEW_CACHE_MAX_ENTRIES/);
  assert.match(previewBackendSource, /thumbnail_cache_relative_path/);
  assert.match(previewCacheBackendSource, /pragma_update\(None, "journal_mode", "WAL"\)/);
  assert.match(previewCacheBackendSource, /preview_entries_lru/);
  assert.match(previewCacheBackendSource, /cache_for/);
  assert.match(previewBackendSource, /THUMBNAIL_GENERATION_LOCKS/);
  assert.match(previewBackendSource, /96\.\.=4096/);
  assert.match(previewBackendSource, /formatOptions/);
  assert.match(previewBackendSource, /set_icc_profile/);
  assert.match(thumbnailSource, /Math\.min\(maxEdge, QUICK_PREVIEW_EDGE\)/);
  assert.match(thumbnailSource, /QUICK_PREVIEW_EDGE = 512/);
  assert.match(thumbnailSource, /peekPhotoPreviewUrl/);
  assert.match(thumbnailSource, /rootMargin: "600px"/);
  assert.match(thumbnailSource, /acquirePhotoPreviewUrl/);
  assert.match(thumbnailSource, /qualityFirst/);
  assert.match(thumbnailSource, /cachedFull \?\? cachedPreview/);
  assert.doesNotMatch(thumbnailSource, /loaded\?\.url \?\? null/);
  assert.match(thumbnailSource, /const previewPromise = previewLease\.promise/);
  assert.match(thumbnailSource, /onFullReadyRef\.current\?\.\(\)/);
  assert.match(cacheSource, /PREVIEW_LOAD_TIMEOUT_MS/);
  assert.match(gridSource, /virtualPhotoGridWindow/);
  assert.match(gridSource, /maxEdge=\{512\}/);
  assert.match(gridSource, /assets\.slice\(windowState\.startIndex, windowState\.endIndex\)/);
  assert.match(filmstripSource, /virtualFilmstripWindow/);
  assert.match(filmstripSource, /assets\.slice\(windowState\.startIndex, windowState\.endIndex\)/);
  assert.match(filmstripSource, /filmstrip-preload-indicator/);
  assert.doesNotMatch(filmstripSource, /filmstrip-ready-indicator/);
  assert.match(stylesheetSource, /minmax\(50px, auto\) 100px/);
  assert.match(stylesheetSource, /\.loupe-filmstrip \{[\s\S]*scrollbar-gutter: stable/);
  assert.match(cargoSource, /objc2-app-kit/);
  assert.match(cargoSource, /objc2-foundation/);
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
