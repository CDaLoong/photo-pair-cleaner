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

test("watermark studio composes the approved three-pane workspace", () => {
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  for (const component of [
    "WatermarkHeader",
    "WatermarkTemplatePanel",
    "WatermarkInspector",
    "WatermarkFilmstrip",
  ]) {
    assert.match(moduleSource, new RegExp(`<${component}`));
  }
  assert.match(moduleSource, /data-watermark-tour="sources-templates"/);
  assert.match(moduleSource, /data-watermark-tour="canvas"/);
  assert.match(moduleSource, /data-watermark-tour="inspector"/);
  assert.match(moduleSource, /data-watermark-tour="filmstrip"/);
});

test("watermark studio persists collapsible panels and restores immersive state", () => {
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  const shellSource = fs.readFileSync(new URL("../src/app/AppShell.tsx", import.meta.url), "utf8");
  assert.match(moduleSource, /framepair\.watermark\.left-panel-collapsed\.v1/);
  assert.match(moduleSource, /framepair\.watermark\.right-panel-collapsed\.v1/);
  assert.match(moduleSource, /immersiveRestoreRef/);
  assert.match(shellSource, /is-immersive/);
});

test("watermark filmstrip follows keyboard selection and reports orientation", () => {
  const filmstripSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkFilmstrip.tsx", import.meta.url),
    "utf8",
  );
  assert.match(filmstripSource, /filmstripScrollTarget/);
  assert.match(filmstripSource, /ArrowLeft/);
  assert.match(filmstripSource, /ArrowRight/);
  assert.match(filmstripSource, /Home/);
  assert.match(filmstripSource, /End/);
  assert.match(filmstripSource, /orientation/);
});

test("watermark header icon commands have accessible Chinese names", () => {
  const headerSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkHeader.tsx", import.meta.url),
    "utf8",
  );
  for (const label of ["撤销", "重做", "对比原图", "收起照片与模板", "收起属性面板", "进入沉浸模式"]) {
    assert.match(headerSource, new RegExp(`aria-label=.*${label}`));
  }
});
