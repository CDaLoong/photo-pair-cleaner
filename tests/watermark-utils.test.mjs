import assert from "node:assert/strict";
import test from "node:test";
import {
  classifyWatermarkOrientation,
  createDefaultWatermarkTemplate,
  outputExtension,
  selectedLayoutVariant,
} from "../src/features/watermark/watermarkUtils.ts";
import {
  WatermarkPreviewCache,
  decodeWatermarkPreviewEnvelope,
  stableWatermarkStringify,
  watermarkPreviewRequestKey,
} from "../src/features/watermark/watermarkPreviewCache.ts";

test("watermark orientation uses the agreed near-square band", () => {
  assert.equal(classifyWatermarkOrientation(1200, 800), "landscape");
  assert.equal(classifyWatermarkOrientation(800, 1200), "portrait");
  assert.equal(classifyWatermarkOrientation(1000, 960), "square");
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
  });
  assert.deepEqual([...decoded.png], [...png]);
  assert.throws(() => decodeWatermarkPreviewEnvelope(Uint8Array.from([0, 1, 2])));
  assert.throws(() => {
    const invalid = Uint8Array.from([0, 0, 1, 0, 123]);
    decodeWatermarkPreviewEnvelope(invalid);
  });
});
