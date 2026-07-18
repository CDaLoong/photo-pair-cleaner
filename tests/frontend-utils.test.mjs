import assert from "node:assert/strict";
import test from "node:test";
import {
  PreviewUrlCache,
  preloadPreviewRequests,
} from "../src/features/preview/previewCache.ts";
import * as previewUtils from "../src/features/preview/previewUtils.ts";
import * as utils from "../src/utils.ts";

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
    extensions: ["NEF"],
    sizeBytes: 20,
    modifiedMs: 20,
    rating: 0,
  },
  {
    id: "day/a",
    name: "A",
    relativeStem: "day/A",
    previewPath: "day/A.JPG",
    jpegPaths: ["day/A.JPG"],
    rawPaths: ["day/A.CR3"],
    extensions: ["JPG", "CR3"],
    sizeBytes: 10,
    modifiedMs: 10,
    rating: 4,
  },
  {
    id: "other/c",
    name: "C",
    relativeStem: "other/C",
    previewPath: "other/C.jpeg",
    jpegPaths: ["other/C.jpeg"],
    rawPaths: [],
    extensions: ["JPEG"],
    sizeBytes: 30,
    modifiedMs: 30,
    rating: 2,
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
