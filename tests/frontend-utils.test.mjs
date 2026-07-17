import assert from "node:assert/strict";
import test from "node:test";
import * as utils from "../src/utils.ts";

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
