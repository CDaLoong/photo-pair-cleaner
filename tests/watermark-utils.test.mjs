import assert from "node:assert/strict";
import test from "node:test";
import {
  canManipulateWatermarkLayer,
  clampWatermarkNumber,
  classifyWatermarkOrientation,
  createDefaultWatermarkTemplate,
  createWatermarkExportProgress,
  DEFAULT_WATERMARK_OUTPUT,
  defaultWatermarkOutputDirectory,
  failedWatermarkPhotoIds,
  keyboardNudgeNormalized,
  normalizeWatermarkRotation,
  outputExtension,
  reduceWatermarkExportProgress,
  selectedLayoutVariant,
  snapNormalizedPosition,
  validateWatermarkOutputSettings,
  viewportDeltaToNormalized,
  watermarkFilenameExamples,
} from "../src/features/watermark/watermarkUtils.ts";
import * as watermarkUtils from "../src/features/watermark/watermarkUtils.ts";
import {
  WatermarkPreviewCache,
  decodeWatermarkPreviewEnvelope,
  stableWatermarkStringify,
  watermarkPreviewRequestKey,
} from "../src/features/watermark/watermarkPreviewCache.ts";
import {
  createWatermarkEditorState,
  watermarkEditorReducer,
} from "../src/features/watermark/watermarkEditorState.ts";

function templateWithOneLayer() {
  const template = createDefaultWatermarkTemplate("editor", "编辑测试");
  template.shared.layers.push({
    kind: "text",
    id: "signature",
    name: "署名",
    zIndex: 0,
    visible: true,
    locked: false,
    text: "FramePair",
    fontFamily: "Noto Sans CJK SC",
    fontWeight: 500,
    color: "#111111",
    align: "center",
    letterSpacingRatio: 0,
    lineHeight: 1.2,
    strokeColor: "#ffffff",
    strokeWidthRatio: 0,
    shadowColor: "#000000",
    shadowBlurRatio: 0,
    shadowOffsetXRatio: 0,
    shadowOffsetYRatio: 0,
  });
  for (const variant of Object.values(template.variants)) {
    variant.layerLayouts.signature = {
      placement: {
        anchorSpace: "frame",
        frameEdge: "bottom",
        x: 0.5,
        y: 0.5,
        width: 0.28,
        rotationDeg: 0,
        opacity: 1,
      },
      fontSizeRatio: 0.035,
    };
  }
  return template;
}

test("watermark orientation uses the agreed near-square band", () => {
  assert.equal(classifyWatermarkOrientation(1200, 800), "landscape");
  assert.equal(classifyWatermarkOrientation(800, 1200), "portrait");
  assert.equal(classifyWatermarkOrientation(1000, 960), "square");
});

test("watermark drag geometry is projected immediately from local placement", () => {
  assert.equal(typeof watermarkUtils.projectWatermarkLayerGeometry, "function");
  const geometry = {
    id: "signature",
    anchorRect: { x: 100, y: 200, width: 400, height: 100 },
    centerX: 300,
    centerY: 250,
    width: 200,
    height: 20,
    rotationDeg: 0,
  };
  assert.deepEqual(
    watermarkUtils.projectWatermarkLayerGeometry(
      geometry,
      { anchorSpace: "frame", frameEdge: "bottom", x: 0.5, y: 0.5, width: 0.5, rotationDeg: 0, opacity: 1 },
      { anchorSpace: "frame", frameEdge: "bottom", x: 0.75, y: 0.2, width: 0.25, rotationDeg: 30, opacity: 1 },
    ),
    { ...geometry, centerX: 400, centerY: 220, width: 100, height: 10, rotationDeg: 30 },
  );
});

test("default template has all three independent variants", () => {
  const template = createDefaultWatermarkTemplate("template-1", "未命名模板");
  assert.equal(template.schemaVersion, 1);
  assert.deepEqual(Object.keys(template.variants).sort(), ["landscape", "portrait", "square"]);
  assert.notEqual(template.variants.landscape, template.variants.portrait);
});

test("output extension follows the selected format", () => {
  assert.equal(outputExtension("jpeg"), "jpg");
  assert.equal(outputExtension("png"), "png");
  assert.equal(selectedLayoutVariant(1000, 960), "square");
});

test("watermark export defaults are safe and filename previews are deterministic", () => {
  assert.deepEqual(DEFAULT_WATERMARK_OUTPUT, {
    format: "jpeg",
    jpegQuality: 90,
    sizing: { kind: "original", allowUpscale: false },
    colorSpace: "srgb",
    transparentBackground: false,
    jpegFlattenColor: "#ffffff",
    metadataPolicy: "privacy",
    outputDirectory: null,
    suffix: "_FramePair",
    collisionPolicy: "sequence",
  });
  const snapshot = {
    rootPaths: ["/photos/session"],
    photos: [{ fileName: "DSC_0001.JPG" }, { fileName: "portrait.jpeg" }],
  };
  assert.equal(defaultWatermarkOutputDirectory(snapshot), "/photos/FramePair-Watermarked");
  assert.deepEqual(watermarkFilenameExamples(snapshot, DEFAULT_WATERMARK_OUTPUT), [
    "DSC_0001_FramePair.jpg",
    "portrait_FramePair.jpg",
  ]);
  assert.equal(validateWatermarkOutputSettings(DEFAULT_WATERMARK_OUTPUT, snapshot), null);
});

test("watermark export validation covers multi-root suffix quality size and PNG alpha", () => {
  const snapshot = { rootPaths: ["/one", "/two"], photos: [{ fileName: "one.jpg" }] };
  assert.match(validateWatermarkOutputSettings(DEFAULT_WATERMARK_OUTPUT, snapshot), /输出目录/);
  assert.match(validateWatermarkOutputSettings({ ...DEFAULT_WATERMARK_OUTPUT, jpegQuality: 0 }, snapshot), /质量/);
  assert.match(validateWatermarkOutputSettings({ ...DEFAULT_WATERMARK_OUTPUT, suffix: "../bad" }, snapshot), /后缀/);
  assert.match(validateWatermarkOutputSettings({ ...DEFAULT_WATERMARK_OUTPUT, sizing: { kind: "longEdge", pixels: 32, allowUpscale: false } }, snapshot), /长边/);
  const png = {
    ...DEFAULT_WATERMARK_OUTPUT,
    format: "png",
    transparentBackground: true,
    outputDirectory: "/output",
  };
  assert.equal(validateWatermarkOutputSettings(png, snapshot), null);
  assert.equal(watermarkFilenameExamples(snapshot, png)[0], "one_FramePair.png");
});

test("watermark export events reduce progress and expose failed-only retry IDs", () => {
  let progress = createWatermarkExportProgress();
  progress = reduceWatermarkExportProgress(progress, { type: "started", taskId: "task-1", total: 3 });
  progress = reduceWatermarkExportProgress(progress, { type: "itemStarted", taskId: "task-1", photoId: "a", index: 0 });
  progress = reduceWatermarkExportProgress(progress, { type: "itemFinished", taskId: "task-1", result: { photoId: "a", targetPath: "/a.jpg", status: "succeeded", message: "完成", sizeBytes: 100 } });
  progress = reduceWatermarkExportProgress(progress, { type: "itemFinished", taskId: "task-1", result: { photoId: "b", targetPath: "/b.jpg", status: "failed", message: "失败", sizeBytes: null } });
  progress = reduceWatermarkExportProgress(progress, { type: "finished", taskId: "task-1", summary: { total: 3, succeeded: 1, skipped: 0, failed: 1, cancelled: 1 } });
  assert.equal(progress.phase, "results");
  assert.equal(progress.results.length, 2);
  assert.deepEqual(failedWatermarkPhotoIds(progress), ["b"]);
  assert.deepEqual(progress.summary, { total: 3, succeeded: 1, skipped: 0, failed: 1, cancelled: 1 });
});

test("watermark preview keys use stable recursive property ordering", () => {
  const left = {
    schemaVersion: 1,
    source: { id: "photo-a", root: "/photos", relativePath: "a.jpg" },
    template: { id: "clean", variants: { landscape: { frame: { top: 0.1, bottom: 0.2 } } } },
  };
  const right = {
    template: { variants: { landscape: { frame: { bottom: 0.2, top: 0.1 } } }, id: "clean" },
    source: { relativePath: "a.jpg", root: "/photos", id: "photo-a" },
    schemaVersion: 1,
  };
  assert.equal(stableWatermarkStringify(left), stableWatermarkStringify(right));
  assert.equal(
    watermarkPreviewRequestKey(left, 1400),
    watermarkPreviewRequestKey(right, 1400),
  );
  assert.notEqual(
    watermarkPreviewRequestKey(left, 1400),
    watermarkPreviewRequestKey(right, 1200),
  );
});

test("watermark preview cache shares one loader promise per request key", async () => {
  const cache = new WatermarkPreviewCache();
  let calls = 0;
  const descriptor = {
    key: "photo-a:hash-1",
    photoId: "photo-a",
    root: "/photos",
    templateId: "clean",
  };
  const loader = async () => {
    calls += 1;
    return { url: "blob:shared", width: 100, height: 80, warnings: [] };
  };
  const first = cache.getOrLoad(descriptor, loader);
  const second = cache.getOrLoad(descriptor, loader);
  assert.equal(first, second);
  assert.equal((await first).url, "blob:shared");
  assert.equal(calls, 1);
});

test("watermark preview cache ignores stale generations", () => {
  const released = [];
  const cache = new WatermarkPreviewCache((url) => released.push(url));
  const first = cache.begin("photo-a", "hash-1");
  const second = cache.begin("photo-a", "hash-2");
  assert.equal(cache.accept(first, "blob:first"), false);
  assert.equal(cache.accept(second, "blob:second"), true);
  assert.deepEqual(released, ["blob:first"]);
});

test("watermark preview cache invalidates roots templates and releases retained URLs", async () => {
  const released = [];
  const cache = new WatermarkPreviewCache((url) => released.push(url));
  const load = (url) => async () => ({ url, width: 120, height: 80, warnings: [] });
  await cache.getOrLoad(
    { key: "a", photoId: "a", root: "/one", templateId: "clean" },
    load("blob:a"),
  );
  await cache.getOrLoad(
    { key: "b", photoId: "b", root: "/two", templateId: "clean" },
    load("blob:b"),
  );
  await cache.getOrLoad(
    { key: "c", photoId: "c", root: "/two", templateId: "frame" },
    load("blob:c"),
  );
  cache.invalidateRoot("/one");
  cache.invalidateTemplate("clean");
  assert.deepEqual(released.sort(), ["blob:a", "blob:b"]);
  assert.equal(cache.peek("c")?.url, "blob:c");
  cache.clear();
  assert.deepEqual(released.sort(), ["blob:a", "blob:b", "blob:c"]);
});

test("watermark preview cache retains only the active neighbor window", async () => {
  const released = [];
  const cache = new WatermarkPreviewCache((url) => released.push(url));
  for (const id of ["a", "b", "c", "d", "e", "f"]) {
    await cache.getOrLoad(
      { key: id, photoId: id, root: "/photos", templateId: "clean" },
      async () => ({ url: `blob:${id}`, width: 100, height: 80, warnings: [] }),
    );
  }
  cache.retainPhotos(new Set(["b", "c", "d", "e", "f"]));
  assert.deepEqual(released, ["blob:a"]);
  assert.equal(cache.peek("b")?.url, "blob:b");
});

test("watermark preview binary envelope validates header bounds", () => {
  const header = new TextEncoder().encode(JSON.stringify({
    width: 320,
    height: 200,
    warnings: ["字体已回退"],
    photoRect: { x: 10, y: 8, width: 300, height: 170 },
    layers: [{
      id: "signature",
      anchorRect: { x: 0, y: 178, width: 320, height: 22 },
      centerX: 160,
      centerY: 188,
      width: 100,
      height: 16,
      rotationDeg: 0,
    }],
  }));
  const png = Uint8Array.from([137, 80, 78, 71]);
  const envelope = new Uint8Array(4 + header.length + png.length);
  new DataView(envelope.buffer).setUint32(0, header.length, false);
  envelope.set(header, 4);
  envelope.set(png, 4 + header.length);
  const decoded = decodeWatermarkPreviewEnvelope(envelope);
  assert.deepEqual(decoded.header, {
    width: 320,
    height: 200,
    warnings: ["字体已回退"],
    photoRect: { x: 10, y: 8, width: 300, height: 170 },
    layers: [{
      id: "signature",
      anchorRect: { x: 0, y: 178, width: 320, height: 22 },
      centerX: 160,
      centerY: 188,
      width: 100,
      height: 16,
      rotationDeg: 0,
    }],
  });
  assert.deepEqual([...decoded.png], [...png]);
  assert.throws(() => decodeWatermarkPreviewEnvelope(Uint8Array.from([0, 1, 2])));
  assert.throws(() => {
    const invalid = Uint8Array.from([0, 0, 1, 0, 123]);
    decodeWatermarkPreviewEnvelope(invalid);
  });
});

test("variant placement edits do not leak into portrait", () => {
  const initial = createWatermarkEditorState(templateWithOneLayer());
  const next = watermarkEditorReducer(initial, {
    type: "setLayerPlacement",
    orientation: "landscape",
    layerId: "signature",
    patch: { x: 0.8 },
    historyGroup: null,
  });
  assert.equal(next.present.template.variants.landscape.layerLayouts.signature.placement.x, 0.8);
  assert.equal(next.present.template.variants.portrait.layerLayouts.signature.placement.x, 0.5);
  assert.equal(next.present.dirtyTemplate, true);
  assert.equal(next.present.unexportedChanges, true);
});

test("shared layer content updates every orientation without changing placements", () => {
  const initial = createWatermarkEditorState(templateWithOneLayer());
  const next = watermarkEditorReducer(initial, {
    type: "updateLayer",
    layerId: "signature",
    patch: { text: "新的署名" },
    historyGroup: null,
  });
  assert.equal(next.present.template.shared.layers[0].text, "新的署名");
  assert.equal(next.present.template.variants.landscape.layerLayouts.signature.placement.x, 0.5);
  assert.equal(next.present.template.variants.portrait.layerLayouts.signature.placement.x, 0.5);
});

test("one drag history group is one undo step", () => {
  let state = createWatermarkEditorState(templateWithOneLayer());
  for (const x of [0.55, 0.62, 0.74]) {
    state = watermarkEditorReducer(state, {
      type: "setLayerPlacement",
      orientation: "landscape",
      layerId: "signature",
      patch: { x },
      historyGroup: "drag-signature",
    });
  }
  assert.equal(state.past.length, 1);
  state = watermarkEditorReducer(state, { type: "closeHistoryGroup" });
  state = watermarkEditorReducer(state, { type: "undo" });
  assert.equal(state.present.template.variants.landscape.layerLayouts.signature.placement.x, 0.5);
  state = watermarkEditorReducer(state, { type: "redo" });
  assert.equal(state.present.template.variants.landscape.layerLayouts.signature.placement.x, 0.74);
});

test("layer lifecycle stays synchronized with all variants", () => {
  let state = createWatermarkEditorState(templateWithOneLayer());
  state = watermarkEditorReducer(state, {
    type: "duplicateLayer",
    layerId: "signature",
    newLayerId: "signature-copy",
  });
  assert.equal(state.present.template.shared.layers.length, 2);
  for (const variant of Object.values(state.present.template.variants)) {
    assert.ok(variant.layerLayouts["signature-copy"]);
  }
  state = watermarkEditorReducer(state, {
    type: "setLayerLocked",
    layerId: "signature-copy",
    locked: true,
  });
  state = watermarkEditorReducer(state, {
    type: "setLayerVisible",
    layerId: "signature-copy",
    visible: false,
  });
  state = watermarkEditorReducer(state, {
    type: "reorderLayer",
    layerId: "signature-copy",
    toIndex: 0,
  });
  assert.equal(state.present.template.shared.layers[0].id, "signature-copy");
  assert.equal(state.present.template.shared.layers[0].zIndex, 0);
  assert.equal(state.present.template.shared.layers[0].locked, true);
  assert.equal(state.present.template.shared.layers[0].visible, false);
  state = watermarkEditorReducer(state, { type: "deleteLayer", layerId: "signature-copy" });
  assert.equal(state.present.template.shared.layers.length, 1);
  for (const variant of Object.values(state.present.template.variants)) {
    assert.equal(variant.layerLayouts["signature-copy"], undefined);
  }
});

test("per-photo placement overrides are independent and clearable", () => {
  let state = createWatermarkEditorState(templateWithOneLayer());
  state = watermarkEditorReducer(state, {
    type: "setPhotoOverride",
    photoId: "photo-a",
    patch: { alignX: 0.7, scale: 0.92 },
    historyGroup: null,
  });
  assert.deepEqual(state.present.photoOverrides["photo-a"], {
    alignX: 0.7,
    alignY: 0.5,
    scale: 0.92,
  });
  assert.equal(state.present.dirtyTemplate, false);
  assert.equal(state.present.unexportedChanges, true);
  state = watermarkEditorReducer(state, { type: "clearPhotoOverride", photoId: "photo-a" });
  assert.equal(state.present.photoOverrides["photo-a"], undefined);
});

test("orientation and active layer are view state outside undo history", () => {
  let state = createWatermarkEditorState(templateWithOneLayer());
  state = watermarkEditorReducer(state, { type: "setActiveOrientation", orientation: "portrait" });
  state = watermarkEditorReducer(state, { type: "setActiveLayer", layerId: "signature" });
  assert.equal(state.activeOrientation, "portrait");
  assert.equal(state.activeLayerId, "signature");
  assert.equal(state.past.length, 0);
});

test("template replacement clears undo history and starts a fresh export boundary", () => {
  let state = createWatermarkEditorState(templateWithOneLayer());
  state = watermarkEditorReducer(state, {
    type: "setLayerPlacement",
    orientation: "landscape",
    layerId: "signature",
    patch: { x: 0.8 },
    historyGroup: null,
  });
  state = watermarkEditorReducer(state, {
    type: "replaceTemplate",
    template: createDefaultWatermarkTemplate("replacement", "替换模板"),
  });
  assert.equal(state.past.length, 0);
  assert.equal(state.future.length, 0);
  assert.equal(state.present.template.id, "replacement");
  assert.equal(state.present.dirtyTemplate, false);
  assert.equal(state.present.unexportedChanges, true);
  assert.equal(watermarkEditorReducer(state, { type: "undo" }), state);
});

test("watermark history remains bounded to one hundred documents", () => {
  let state = createWatermarkEditorState(templateWithOneLayer());
  for (let index = 0; index < 120; index += 1) {
    state = watermarkEditorReducer(state, {
      type: "setLayerPlacement",
      orientation: "landscape",
      layerId: "signature",
      patch: { x: index / 120 },
      historyGroup: null,
    });
  }
  assert.equal(state.past.length, 100);
});

test("canvas drag converts viewport pixels into anchor-relative coordinates", () => {
  assert.deepEqual(viewportDeltaToNormalized(
    { dx: 20, dy: -10 },
    { width: 400, height: 200 },
  ), { x: 0.05, y: -0.05 });
});

test("watermark snapping covers center edges and peers within six pixels", () => {
  const centerAndEdge = snapNormalizedPosition({
    position: { x: 0.508, y: 0.028 },
    anchorSize: { width: 400, height: 200 },
    layerSize: { width: 0.2, height: 0.1 },
    peers: [],
    thresholdPx: 6,
    bypass: false,
  });
  assert.deepEqual(centerAndEdge.position, { x: 0.5, y: 0.05 });
  assert.deepEqual(centerAndEdge.guides.sort(), ["anchor-center-x", "anchor-top"]);

  const peer = snapNormalizedPosition({
    position: { x: 0.69, y: 0.52 },
    anchorSize: { width: 400, height: 200 },
    layerSize: { width: 0.2, height: 0.1 },
    peers: [{ id: "peer", x: 0.7, y: 0.8 }],
    thresholdPx: 6,
    bypass: false,
  });
  assert.equal(peer.position.x, 0.7);
  assert.ok(peer.guides.includes("peer-x:peer"));
});

test("holding shift bypasses watermark snapping", () => {
  const snapped = snapNormalizedPosition({
    position: { x: 0.508, y: 0.49 },
    anchorSize: { width: 400, height: 200 },
    layerSize: { width: 0.2, height: 0.1 },
    peers: [],
    thresholdPx: 6,
    bypass: true,
  });
  assert.deepEqual(snapped, { position: { x: 0.508, y: 0.49 }, guides: [] });
});

test("keyboard nudges are zoom independent and support a coarse step", () => {
  assert.deepEqual(keyboardNudgeNormalized(
    { x: 0.5, y: 0.5 },
    "ArrowRight",
    { width: 500, height: 250 },
    false,
  ), { x: 0.502, y: 0.5 });
  assert.deepEqual(keyboardNudgeNormalized(
    { x: 0.5, y: 0.5 },
    "ArrowUp",
    { width: 500, height: 250 },
    true,
  ), { x: 0.5, y: 0.46 });
});

test("rotation numeric fields and locked layers enforce editor bounds", () => {
  assert.equal(normalizeWatermarkRotation(190), -170);
  assert.equal(normalizeWatermarkRotation(-540), -180);
  assert.equal(clampWatermarkNumber(1.4, 0, 1, 0.5), 1);
  assert.equal(clampWatermarkNumber(Number.NaN, 0, 1, 0.5), 0.5);
  assert.equal(canManipulateWatermarkLayer({ locked: false, visible: true }), true);
  assert.equal(canManipulateWatermarkLayer({ locked: true, visible: true }), false);
  assert.equal(canManipulateWatermarkLayer({ locked: false, visible: false }), false);
});

test("variant styling and resources participate in editor undo history", () => {
  let state = createWatermarkEditorState(templateWithOneLayer());
  state = watermarkEditorReducer(state, {
    type: "setVariantFrame",
    orientation: "landscape",
    patch: { bottom: 0.28 },
  });
  state = watermarkEditorReducer(state, {
    type: "setVariantBackground",
    orientation: "landscape",
    background: { kind: "solid", color: "#123456", opacity: 0.8 },
  });
  state = watermarkEditorReducer(state, {
    type: "addResource",
    resource: {
      id: "logo",
      name: "logo.png",
      mimeType: "image/png",
      sha256: "a".repeat(64),
      width: 20,
      height: 10,
      dataBase64: "AA==",
    },
  });
  assert.equal(state.present.template.variants.landscape.frame.bottom, 0.28);
  assert.equal(state.present.template.variants.landscape.background.color, "#123456");
  assert.equal(state.present.template.resources.logo.name, "logo.png");
  state = watermarkEditorReducer(state, { type: "undo" });
  assert.equal(state.present.template.resources.logo, undefined);
});
