import assert from "node:assert/strict";
import test from "node:test";
import {
  classifyWatermarkOrientation,
  createDefaultWatermarkTemplate,
  outputExtension,
  selectedLayoutVariant,
} from "../src/features/watermark/watermarkUtils.ts";

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
