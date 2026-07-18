import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

test("watermark is a first-class app module", () => {
  const shell = fs.readFileSync(new URL("../src/app/AppShell.tsx", import.meta.url), "utf8");
  const app = fs.readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(shell, /"preview" \| "cleanup" \| "watermark"/);
  assert.match(shell, /水印导出/);
  assert.match(app, /<WatermarkModule/);
  assert.match(app, /watermarkTransfer/);
});

test("preview can send current photo directory or filter snapshot", () => {
  const preview = fs.readFileSync(
    new URL("../src/features/preview/PreviewModule.tsx", import.meta.url),
    "utf8",
  );
  assert.match(preview, /currentPhoto/);
  assert.match(preview, /currentDirectory/);
  assert.match(preview, /currentFilter/);
  assert.match(preview, /onSendToWatermark/);
});

test("watermark source intake stays JPG-only and reports skipped files", () => {
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  const panelSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkSourcePanel.tsx", import.meta.url),
    "utf8",
  );
  assert.match(moduleSource, /prepare_watermark_source/);
  assert.ok(moduleSource.includes("/\\.jpe?g$/i"));
  assert.match(panelSource, /仅支持 JPG\/JPEG/);
  assert.match(panelSource, /skippedRawOnly/);
  assert.match(panelSource, /skippedUnsupported/);
});

test("watermark source list keeps the keyboard selection visible", () => {
  const panelSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkSourcePanel.tsx", import.meta.url),
    "utf8",
  );
  assert.match(panelSource, /selectedRowRef/);
  assert.match(panelSource, /scrollIntoView\(\{ block: "nearest", inline: "nearest" \}\)/);
});

test("watermark work cannot be abandoned without explicit confirmation", () => {
  const appSource = fs.readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  const dialogSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkLeaveDialog.tsx", import.meta.url),
    "utf8",
  );
  assert.match(appSource, /onCloseRequested/);
  assert.match(appSource, /pendingDestination/);
  assert.match(appSource, /<WatermarkLeaveDialog/);
  assert.match(moduleSource, /onUnsavedWorkChange/);
  assert.match(moduleSource, /discardToken/);
  assert.match(dialogSource, /尚未导出/);
  assert.match(dialogSource, /放弃更改/);
});
