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
  // 键的字面量集中在 storageKeys.ts，模块只引用符号名。
  const keySource = fs.readFileSync(new URL("../src/storageKeys.ts", import.meta.url), "utf8");
  assert.match(keySource, /framepair\.watermark\.left-panel-collapsed\.v1/);
  assert.match(keySource, /framepair\.watermark\.right-panel-collapsed\.v1/);
  assert.match(moduleSource, /STORAGE_KEYS\.watermarkLeftPanelCollapsed/);
  assert.match(moduleSource, /STORAGE_KEYS\.watermarkRightPanelCollapsed/);
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

test("watermark canvas supports precise pointer and keyboard manipulation", () => {
  const canvasSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkCanvas.tsx", import.meta.url),
    "utf8",
  );
  assert.match(canvasSource, /setPointerCapture/);
  assert.match(canvasSource, /onPointerCancel/);
  assert.match(canvasSource, /historyGroup/);
  assert.match(canvasSource, /event\.shiftKey/);
  assert.match(canvasSource, /thresholdPx: 6/);
  assert.match(canvasSource, /watermark-scale-handle/);
  assert.match(canvasSource, /watermark-rotate-handle/);
  assert.match(canvasSource, /ZOOM_LEVELS = \[0\.5, 0\.75, 1, 1\.5, 2, 3, 4\]/);
});

test("watermark inspector exposes complete local editing controls", () => {
  const inspectorSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkInspector.tsx", import.meta.url),
    "utf8",
  );
  for (const label of [
    "添加水印图层",
    "复制图层",
    "删除图层",
    "EXIF 内容",
    "锚定区域",
    "画布比例",
    "照片样式",
    "背景",
    "单张照片调整",
    "清除单张调整",
  ]) {
    assert.match(inspectorSource, new RegExp(label));
  }
  const backend = fs.readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  assert.match(backend, /import_watermark_resource/);
});

test("watermark templates are portable local assets with immutable builtins", () => {
  const panelSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkTemplatePanel.tsx", import.meta.url),
    "utf8",
  );
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  for (const label of ["保存本地模板", "另存为本地模板", "导入模板", "导出模板", "删除本地模板"]) {
    assert.match(panelSource, new RegExp(label));
  }
  assert.match(panelSource, /active\.builtIn/);
  assert.match(moduleSource, /list_watermark_templates/);
  assert.match(moduleSource, /save_watermark_template/);
  assert.match(moduleSource, /delete_watermark_template/);
  assert.match(moduleSource, /import_watermark_template/);
  assert.match(moduleSource, /export_watermark_template/);
  assert.match(moduleSource, /当前模板包含未保存的调整/);
  assert.match(moduleSource, /Array\.isArray\(entries\)/);
});

test("watermark export dialog confirms settings streams progress and retries failures", () => {
  const dialogSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkExportDialog.tsx", import.meta.url),
    "utf8",
  );
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  const headerSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkHeader.tsx", import.meta.url),
    "utf8",
  );
  for (const label of [
    "导出设置",
    "文件格式",
    "输出尺寸",
    "元数据",
    "同名文件",
    "预计空间",
    "已完成的副本会保留",
    "重试失败项",
    "在文件管理器中显示",
  ]) {
    assert.match(dialogSource, new RegExp(label));
  }
  for (const command of [
    "start_watermark_export",
    "cancel_watermark_export",
    "retry_watermark_export_failures",
    "reveal_watermark_export",
    "acknowledge_watermark_export",
  ]) {
    assert.match(moduleSource, new RegExp(command));
  }
  assert.match(headerSource, /data-watermark-tour="export"/);
  assert.match(headerSource, /导出副本/);
});

test("watermark guide covers the complete workflow and documentation is release ready", () => {
  const guideSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkGuideDialog.tsx", import.meta.url),
    "utf8",
  );
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  const sharedGuide = fs.readFileSync(
    new URL("../src/components/GuidedTourDialog.tsx", import.meta.url),
    "utf8",
  );
  for (const selector of ["sources-templates", "templates", "canvas", "filmstrip", "export"]) {
    assert.match(guideSource, new RegExp(selector));
  }
  assert.match(moduleSource, /STORAGE_KEYS\.watermarkGuide/);
  assert.match(moduleSource, /<WatermarkGuideDialog/);
  assert.match(moduleSource, /setGuideOpen\(true\)/);
  assert.match(sharedGuide, /watermark-studio/);
  assert.match(moduleSource, /SOURCE_PRELOAD_CONCURRENCY = 3/);

  const readme = fs.readFileSync(new URL("../README.md", import.meta.url), "utf8");
  for (const phrase of ["水印导出快速上手", "仅支持 JPG/JPEG", "JPEG/PNG", "元数据", "同名", "模板 JSON", "绝不修改原照片"]) {
    assert.match(readme, new RegExp(phrase));
  }
});

test("watermark release versions stay aligned at 0.8.0", () => {
  const pkg = JSON.parse(fs.readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  const tauri = JSON.parse(fs.readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const cargo = fs.readFileSync(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8");
  assert.equal(pkg.version, "0.8.0");
  assert.equal(tauri.version, "0.8.0");
  assert.match(cargo, /^version = "0\.8\.0"$/m);
});

test("packaged CSP permits generated preview blobs only for images", () => {
  const tauri = JSON.parse(fs.readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const directives = new Map(tauri.app.security.csp.split(";").map((directive) => {
    const [name, ...sources] = directive.trim().split(/\s+/);
    return [name, sources];
  }));
  assert.ok(directives.get("img-src")?.includes("blob:"), "img-src must allow in-memory preview images");
  assert.ok(!directives.get("default-src")?.includes("blob:"), "blob URLs must stay scoped to images");
});

test("watermark selection never displays the previous photo preview", () => {
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  const canvasSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkCanvas.tsx", import.meta.url),
    "utf8",
  );
  assert.match(moduleSource, /previewPhotoId === selectedPhoto\?\.id/);
  assert.match(moduleSource, /originalPhotoId === selectedPhoto\?\.id/);
  assert.match(canvasSource, /watermark-canvas-placeholder/);
});

test("watermark pointer dragging stays local until the pointer is released", () => {
  const canvasSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkCanvas.tsx", import.meta.url),
    "utf8",
  );
  const moveDrag = canvasSource.slice(
    canvasSource.indexOf("function moveDrag"),
    canvasSource.indexOf("function endDrag"),
  );
  const endDrag = canvasSource.slice(
    canvasSource.indexOf("function endDrag"),
    canvasSource.indexOf("function nudgeLayer"),
  );
  assert.match(moveDrag, /setLiveGeometry/);
  assert.doesNotMatch(moveDrag, /onSetLayerPlacement/);
  assert.match(endDrag, /onSetLayerPlacement/);
  assert.doesNotMatch(endDrag, /setLiveGeometry\(null\)/);
});

test("watermark foreground renders are not queued behind speculative neighbors", () => {
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(moduleSource, /for \(const neighbor of keepPhotos\)/);
  assert.doesNotMatch(moduleSource, /cache\.retainPhotos/);
});

test("watermark preview bounds decoded pixels before floating-point rendering", () => {
  const renderSource = fs.readFileSync(
    new URL("../src-tauri/src/watermark_render.rs", import.meta.url),
    "utf8",
  );
  assert.match(renderSource, /decode_path\(source, preview_source_edge\(target\)\)/);
  assert.match(renderSource, /image\.resize\(maximum_edge, maximum_edge, FilterType::Lanczos3\)/);
});

test("watermark loads a full-size original preview only when comparison is active", () => {
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  assert.match(moduleSource, /if \(compareOriginal\) \{\s*void loadPhotoPreviewUrl\(request\)/);
  assert.match(moduleSource, /\[compareOriginal, selectedPhoto, snapshot\]/);
});

test("watermark orientation controls disable layouts without a matching photo", () => {
  const moduleSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkModule.tsx", import.meta.url),
    "utf8",
  );
  const inspectorSource = fs.readFileSync(
    new URL("../src/features/watermark/WatermarkInspector.tsx", import.meta.url),
    "utf8",
  );
  assert.match(moduleSource, /availableOrientations=\{availableOrientations\}/);
  assert.match(inspectorSource, /disabled=\{!availableOrientations\.has\(item\.id\)\}/);
  assert.match(inspectorSource, /当前任务没有.*照片/);
});
