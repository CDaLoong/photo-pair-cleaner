# FramePair Watermark Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build FramePair's third module, “水印导出”, so users can import JPG files, apply structured text/logo/EXIF layers and decorative borders, preview the result, and safely export JPEG or PNG copies without modifying source photos.

**Architecture:** Keep the React editor under `src/features/watermark` and pass source selections through typed props owned by `App.tsx`. Put all trusted file access, versioned model validation, rendering, metadata handling, template persistence, and export execution in focused Rust `watermark_*` modules; preview and full export consume the same `WatermarkRenderRequest` model.

**Tech Stack:** React 19, TypeScript 7, Vite 8, Tauri 2, Rust 2024, `image` 0.25.10 with JPEG/PNG, `imageproc` 0.26.2, `cosmic-text` 0.19.0, `little_exif` 0.6.23, `moxcms` 0.8.1, Node test runner, Cargo integration tests.

---

## Execution Constraints

- Execute on the current branch and current worktree. Do not create another worktree.
- Execute tasks sequentially. Do not run implementation tasks concurrently.
- Follow TDD inside every task: failing focused test, minimal implementation, focused pass, broader regression pass, commit.
- Do not modify RAW, XMP, or source JPG files. All image writes target a user-selected output path through a temporary file.
- Keep `src/App.tsx`, `src-tauri/src/lib.rs`, and `src/features/preview/PreviewModule.tsx` as composition layers. Move watermark behavior into new focused files.
- Use Chinese user-facing text and `lucide-react` icons. Reuse existing FramePair tokens and control styles.
- Do not add WebP/AVIF, cloud/community templates, AI features, free drawing, or social publishing.

## Phase Gates

| Phase | Tasks | Working result required before continuing |
|---|---:|---|
| 1. 来源与模块骨架 | 1-3 | Third navigation item works; directory, drop, and preview handoff produce an immutable JPG-only source snapshot. |
| 2. 共享渲染与实时预览 | 4-8 | The Rust renderer produces the same layout model for cached preview and full-size output, including all three layer types. |
| 3. 结构化编辑器 | 9-11 | Three-pane editor supports undo/redo, variants, per-photo placement override, direct manipulation, inspector controls, and collapsible panels. |
| 4. 模板与漂亮边框 | 12 | Six built-in templates and portable local JSON templates work with embedded resources and three orientation variants. |
| 5. 安全批量导出 | 13-15 | JPEG/PNG export, metadata policy, naming conflicts, progress, cancellation, result review, and failed-only retry work end to end. |
| 6. 引导、性能与发布验收 | 16 | Guided tour, preload behavior, responsive layout, golden images, docs, CI, and cross-platform manual checks pass. |

## File Structure

### Frontend files

- Create `src/features/watermark/types.ts`: IPC and editor domain types shared by watermark components.
- Create `src/features/watermark/watermarkUtils.ts`: pure orientation, naming, layout, validation, and formatting helpers.
- Create `src/features/watermark/watermarkEditorState.ts`: reducer, command history, variants, and per-photo overrides.
- Create `src/features/watermark/watermarkPreviewCache.ts`: render request hashing, stale request suppression, and object URL lifecycle.
- Create `src/features/watermark/WatermarkModule.tsx`: module orchestration only.
- Create `src/features/watermark/WatermarkHeader.tsx`: module header and undo/redo/compare/export commands.
- Create `src/features/watermark/WatermarkSourcePanel.tsx`: source list, directory tree, and source warnings.
- Create `src/features/watermark/WatermarkTemplatePanel.tsx`: built-in and local template lists and commands.
- Create `src/features/watermark/WatermarkCanvas.tsx`: preview display, zoom, compare, handles, drag, rotate, and snapping.
- Create `src/features/watermark/WatermarkInspector.tsx`: layer, border, canvas, and output property controls.
- Create `src/features/watermark/WatermarkFilmstrip.tsx`: photo switching, preload state, and selected-item auto-scroll.
- Create `src/features/watermark/WatermarkExportDialog.tsx`: confirmation, progress, cancellation, results, and retry.
- Create `src/features/watermark/WatermarkGuideDialog.tsx`: five-step masked guide.
- Create `src/features/watermark/WatermarkLeaveDialog.tsx`: guard module/app exit with unsaved template changes or unexported work.
- Create `src/features/watermark/SendToWatermarkMenu.tsx`: preview-to-watermark scope menu.
- Create `src/features/watermark/watermark.css`: feature-scoped three-pane and responsive styles.
- Modify `src/App.tsx`: own typed transfer draft and switch to the watermark module.
- Modify `src/app/AppShell.tsx`: add the third navigation entry.
- Modify `src/features/preview/PreviewModule.tsx`: expose current-photo/current-directory/current-filter transfer actions.
- Modify `src/features/preview/PhotoContextMenu.tsx`: add current-photo handoff command.
- Modify `src/components/GuidedTourDialog.tsx`: recognize the watermark workspace as a scroll container.
- Modify `package.json`: run all `tests/*.test.mjs` files.
- Create `tests/watermark-utils.test.mjs`: pure model, reducer, cache, and source-transfer tests.
- Create `tests/watermark-ui.test.mjs`: structural integration checks for the new module.

### Rust files

- Create `src-tauri/src/watermark_model.rs`: versioned serde types and strict validation.
- Create `src-tauri/src/watermark_source.rs`: JPG-only source snapshot creation and snapshot revalidation.
- Create `src-tauri/src/watermark_geometry.rs`: normalized coordinates, canvas layout, anchors, snapping, and photo placement.
- Create `src-tauri/src/watermark_color.rs`: ICC parsing, sRGB/source-profile conversion, gradients, and compositing color rules.
- Create `src-tauri/src/watermark_metadata.rs`: visible EXIF fields and output metadata policies.
- Create `src-tauri/src/watermark_text.rs`: font discovery, fallback, shaping, measurement, and rasterization.
- Create `src-tauri/src/watermark_render.rs`: background, photo, effects, and ordered layer rendering.
- Create `src-tauri/src/watermark_templates.rs`: built-ins, local persistence, validation, JSON import/export, and embedded resources.
- Create `src-tauri/src/watermark_output.rs`: output sizing, naming, collision policy, encoding, metadata, and atomic commit.
- Create `src-tauri/src/watermark_export.rs`: bounded queue, cancellation state, events, and failed-only retry.
- Create `src-tauri/src/watermark_commands.rs`: thin Tauri command adapters.
- Modify `src-tauri/src/lib.rs`: register modules, managed state, and command names only.
- Modify `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`: add audited rendering and metadata dependencies.
- Modify `src-tauri/tauri.conf.json`: bundle the licensed Chinese font resource and update module description.
- Create `src-tauri/resources/fonts/NotoSansCJKsc-Regular.otf`: stable built-in Chinese font.
- Create `src-tauri/resources/fonts/OFL.txt`: bundled font license.
- Create `THIRD_PARTY_NOTICES.md`: dependency and bundled asset notices.
- Create `src-tauri/tests/watermark_model.rs`.
- Create `src-tauri/tests/watermark_source.rs`.
- Create `src-tauri/tests/watermark_geometry.rs`.
- Create `src-tauri/tests/watermark_render.rs`.
- Create `src-tauri/tests/watermark_metadata.rs`.
- Create `src-tauri/tests/watermark_templates.rs`.
- Create `src-tauri/tests/watermark_output.rs`.
- Create `src-tauri/tests/watermark_export.rs`.
- Create `src-tauri/tests/watermark_golden.rs`.
- Create `src-tauri/tests/fixtures/watermark/`: reviewed input and expected PNG fixtures.

## Dependency Decisions

- Pin `imageproc` exactly to `0.26.2` because it is the patched `0.26` release for RUSTSEC-2026-0115 and matches the documented API used by this plan.
- Use `cosmic-text` instead of `imageproc` text drawing so Chinese shaping, system font discovery, and fallback are consistent on macOS and Windows.
- Use `little_exif` only behind a panic-safe adapter because its public documentation notes panic behavior for unsupported inputs. Source types are still restricted to verified JPG/JPEG.
- Use `moxcms` for ICC transforms. Never relabel non-sRGB pixels as sRGB without conversion.
- Bundle one OFL-licensed Noto CJK font for stable built-in templates and golden tests; user-selected system fonts remain optional and produce a visible fallback warning when missing.

## Phase 1: Sources And Module Shell

### Task 1: Versioned Watermark Domain Model

**Files:**
- Create: `src/features/watermark/types.ts`
- Create: `src/features/watermark/watermarkUtils.ts`
- Create: `src-tauri/src/watermark_model.rs`
- Create: `src-tauri/tests/watermark_model.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `package.json`
- Create: `tests/watermark-utils.test.mjs`

- [ ] **Step 1: Broaden the frontend test command and write failing model tests**

Change the script to:

```json
"test:frontend": "node --test tests/*.test.mjs"
```

Create tests that establish the public model behavior:

```js
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
```

- [ ] **Step 2: Run the frontend test and verify it fails**

Run: `npm run test:frontend`

Expected: FAIL because `types.ts` and `watermarkUtils.ts` do not exist.

- [ ] **Step 3: Add exact frontend domain types and defaults**

Define these unions and interfaces in `types.ts`:

```ts
export const WATERMARK_SCHEMA_VERSION = 1 as const;

export type WatermarkOrientation = "landscape" | "portrait" | "square";
export type WatermarkLayerKind = "text" | "exifText" | "image";
export type WatermarkAnchorSpace = "photo" | "frame" | "canvas";
export type WatermarkFrameEdge = "top" | "right" | "bottom" | "left";
export type WatermarkOutputFormat = "jpeg" | "png";
export type MetadataPolicy = "preserve" | "privacy" | "remove";
export type CollisionPolicy = "sequence" | "skip" | "overwriteOutput";

export interface NormalizedPlacement {
  anchorSpace: WatermarkAnchorSpace;
  frameEdge: WatermarkFrameEdge | null;
  x: number;
  y: number;
  width: number;
  rotationDeg: number;
  opacity: number;
}

export interface WatermarkLayerBase {
  id: string;
  name: string;
  zIndex: number;
  visible: boolean;
  locked: boolean;
}

export interface TextLayer extends WatermarkLayerBase {
  kind: "text";
  text: string;
  fontFamily: string;
  fontWeight: number;
  color: string;
  align: "left" | "center" | "right";
  letterSpacingRatio: number;
  lineHeight: number;
  strokeColor: string;
  strokeWidthRatio: number;
  shadowColor: string;
  shadowBlurRatio: number;
  shadowOffsetXRatio: number;
  shadowOffsetYRatio: number;
}

export interface ExifTextLayer extends Omit<TextLayer, "kind" | "text"> {
  kind: "exifText";
  fields: string[];
  separator: string;
  prefix: string;
  suffix: string;
  missingValue: string | null;
}

export interface ImageLayer extends WatermarkLayerBase {
  kind: "image";
  resourceId: string;
  fit: "contain" | "cover";
}

export type WatermarkLayer = TextLayer | ExifTextLayer | ImageLayer;
```

Complete the remaining type contract in the same file. These property names are authoritative for every later frontend and Rust serde model:

```ts
export interface FrameInsets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface GradientStop {
  offset: number;
  color: string;
  opacity: number;
}

export type WatermarkBackground =
  | { kind: "transparent" }
  | { kind: "solid"; color: string; opacity: number }
  | { kind: "sampled"; x: number; y: number; color: string; sampleEachPhoto: boolean }
  | { kind: "linearGradient"; angleDeg: number; stops: GradientStop[] }
  | { kind: "radialGradient"; centerX: number; centerY: number; radius: number; stops: GradientStop[] }
  | { kind: "blurredPhoto"; blurRatio: number; scale: number; overlayColor: string; overlayOpacity: number }
  | { kind: "image"; resourceId: string; fit: "contain" | "cover"; opacity: number };

export interface PhotoStyle {
  alignX: number;
  alignY: number;
  scale: number;
  cornerRadiusRatio: number;
  strokeWidthRatio: number;
  strokeColor: string;
  shadowBlurRatio: number;
  shadowOpacity: number;
  shadowOffsetXRatio: number;
  shadowOffsetYRatio: number;
}

export interface VariantLayerLayout {
  placement: NormalizedPlacement;
  fontSizeRatio: number | null;
}

export interface LayoutVariant {
  canvasRatio: number | null;
  frame: FrameInsets;
  background: WatermarkBackground;
  photo: PhotoStyle;
  layerLayouts: Record<string, VariantLayerLayout>;
}

export interface EmbeddedTemplateResource {
  id: string;
  name: string;
  mimeType: "image/png" | "image/jpeg";
  sha256: string;
  width: number;
  height: number;
  dataBase64: string;
}

export interface WatermarkTemplate {
  schemaVersion: typeof WATERMARK_SCHEMA_VERSION;
  id: string;
  name: string;
  shared: { layers: WatermarkLayer[]; palette: string[] };
  variants: Record<WatermarkOrientation, LayoutVariant>;
  resources: Record<string, EmbeddedTemplateResource>;
}

export type WatermarkSourceOrigin =
  | "directory"
  | "drop"
  | "preview-photo"
  | "preview-directory"
  | "preview-filter";

export interface WatermarkSourcePhoto {
  id: string;
  root: string;
  relativePath: string;
  fileName: string;
  sizeBytes: number;
  modifiedMs: number;
  pixelWidth: number;
  pixelHeight: number;
  orientation: WatermarkOrientation;
}

export interface WatermarkSourceSnapshot {
  id: string;
  createdAtMs: number;
  origin: WatermarkSourceOrigin;
  rootPaths: string[];
  photos: WatermarkSourcePhoto[];
  skippedRawOnly: number;
  skippedUnsupported: number;
}

export interface PhotoPlacementOverride {
  alignX: number;
  alignY: number;
  scale: number;
}

export type WatermarkSizing =
  | { kind: "original"; allowUpscale: false }
  | { kind: "longEdge"; pixels: number; allowUpscale: boolean };

export interface WatermarkOutputSettings {
  format: WatermarkOutputFormat;
  jpegQuality: number;
  sizing: WatermarkSizing;
  colorSpace: "srgb" | "preserve";
  transparentBackground: boolean;
  jpegFlattenColor: string;
  metadataPolicy: MetadataPolicy;
  outputDirectory: string | null;
  suffix: string;
  collisionPolicy: CollisionPolicy;
}

export interface WatermarkRenderRequest {
  schemaVersion: typeof WATERMARK_SCHEMA_VERSION;
  source: WatermarkSourcePhoto;
  template: WatermarkTemplate;
  photoOverride: PhotoPlacementOverride | null;
  colorSpace: "srgb" | "preserve";
  transparentBackground: boolean;
  jpegFlattenColor: string;
}
```

Implement complete defaults in `watermarkUtils.ts`:

```ts
import type {
  LayoutVariant,
  WatermarkOrientation,
  WatermarkOutputFormat,
  WatermarkTemplate,
} from "./types";
import { WATERMARK_SCHEMA_VERSION } from "./types";

export function classifyWatermarkOrientation(width: number, height: number): WatermarkOrientation {
  if (width <= 0 || height <= 0) throw new Error("照片尺寸必须大于 0");
  const ratio = width / height;
  if (ratio >= 0.95 && ratio <= 1.05) return "square";
  return width > height ? "landscape" : "portrait";
}

function defaultVariant(): LayoutVariant {
  return {
    canvasRatio: null,
    frame: { top: 0.04, right: 0.04, bottom: 0.14, left: 0.04 },
    background: { kind: "solid", color: "#ffffff", opacity: 1 },
    photo: {
      alignX: 0.5,
      alignY: 0.5,
      scale: 1,
      cornerRadiusRatio: 0,
      strokeWidthRatio: 0,
      strokeColor: "#ffffff",
      shadowBlurRatio: 0,
      shadowOpacity: 0,
      shadowOffsetXRatio: 0,
      shadowOffsetYRatio: 0,
    },
    layerLayouts: {},
  };
}

export function createDefaultWatermarkTemplate(id: string, name: string): WatermarkTemplate {
  return {
    schemaVersion: WATERMARK_SCHEMA_VERSION,
    id,
    name,
    shared: { layers: [], palette: ["#ffffff", "#111111"] },
    variants: {
      landscape: defaultVariant(),
      portrait: defaultVariant(),
      square: defaultVariant(),
    },
    resources: {},
  };
}

export function selectedLayoutVariant(width: number, height: number): WatermarkOrientation {
  return classifyWatermarkOrientation(width, height);
}

export function outputExtension(format: WatermarkOutputFormat): "jpg" | "png" {
  return format === "jpeg" ? "jpg" : "png";
}
```

- [ ] **Step 4: Run the frontend tests and verify they pass**

Run: `npm run test:frontend`

Expected: PASS for all existing tests and the new watermark model tests.

- [ ] **Step 5: Write failing Rust validation tests**

Before compiling the Rust model, add the bounded resource decoder dependency and update the lockfile:

```toml
base64 = "0.22.1"
```

Create `src-tauri/tests/watermark_model.rs` using the repository's `#[path]` integration-test pattern:

```rust
#[path = "../src/watermark_model.rs"]
mod watermark_model;

use watermark_model::{default_template, validate_template};

#[test]
fn default_template_contains_three_valid_variants() {
    let template = default_template("template-1", "未命名模板");
    assert!(validate_template(&template).is_ok());
    assert_eq!(template.schema_version, 1);
    assert_eq!(template.variants.len(), 3);
}

#[test]
fn validation_rejects_out_of_range_normalized_values() {
    let mut template = default_template("template-1", "无效模板");
    template.variants.get_mut("landscape").unwrap().frame.left = 1.5;
    assert!(validate_template(&template).unwrap_err().contains("边框"));
}
```

- [ ] **Step 6: Run the Rust model test and verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test watermark_model`

Expected: FAIL because `watermark_model.rs` does not exist.

- [ ] **Step 7: Implement the strict Rust model**

Mirror the TypeScript names with `#[serde(rename_all = "camelCase", deny_unknown_fields)]`. Use tagged enums for background and layers, `BTreeMap<String, LayoutVariant>` for variants, and constants for limits:

```rust
use base64::Engine;

pub(crate) const WATERMARK_SCHEMA_VERSION: u16 = 1;
pub(crate) const MAX_LAYERS: usize = 64;
pub(crate) const MAX_RESOURCE_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_TEMPLATE_RESOURCE_BYTES: usize = 128 * 1024 * 1024;

pub(crate) fn normalized(value: f32, label: &str) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("{label}必须在 0 到 1 之间"));
    }
    Ok(())
}

fn finite_between(value: f32, minimum: f32, maximum: f32, label: &str) -> Result<(), String> {
    if !value.is_finite() || value < minimum || value > maximum {
        return Err(format!("{label}必须在 {minimum} 到 {maximum} 之间"));
    }
    Ok(())
}

impl WatermarkLayer {
    pub(crate) fn base(&self) -> &WatermarkLayerBase {
        match self {
            Self::Text { base, .. } | Self::ExifText { base, .. } | Self::Image { base, .. } => base,
        }
    }
}

fn validate_layer_layout(layer: &WatermarkLayer, layout: &VariantLayerLayout) -> Result<(), String> {
    finite_between(layout.placement.x, -1.0, 2.0, "图层 X")?;
    finite_between(layout.placement.y, -1.0, 2.0, "图层 Y")?;
    finite_between(layout.placement.rotation_deg, -360.0, 360.0, "图层角度")?;
    for (label, value) in [("图层宽度", layout.placement.width), ("图层透明度", layout.placement.opacity)] {
        normalized(value, label)?;
    }
    match (layer, layout.font_size_ratio) {
        (WatermarkLayer::Text { .. } | WatermarkLayer::ExifText { .. }, Some(value)) => {
            normalized(value, "文字字号")
        }
        (WatermarkLayer::Image { .. }, None) => Ok(()),
        (WatermarkLayer::Text { .. } | WatermarkLayer::ExifText { .. }, None) => {
            Err("文字图层必须设置当前方向字号".to_string())
        }
        (WatermarkLayer::Image { .. }, Some(_)) => Err("图片图层不能设置文字字号".to_string()),
    }
}

pub(crate) fn validate_template(template: &WatermarkTemplate) -> Result<(), String> {
    if template.schema_version != WATERMARK_SCHEMA_VERSION {
        return Err(format!("不支持水印模板版本 {}", template.schema_version));
    }
    if template.id.trim().is_empty() || template.name.trim().is_empty() {
        return Err("模板 ID 和名称不能为空".to_string());
    }
    if template.shared.layers.len() > MAX_LAYERS {
        return Err(format!("模板图层不能超过 {MAX_LAYERS} 个"));
    }
    if template.variants.len() != 3 {
        return Err("模板只能包含横版、竖版和方形三种布局".to_string());
    }
    for orientation in ["landscape", "portrait", "square"] {
        let variant = template
            .variants
            .get(orientation)
            .ok_or_else(|| format!("模板缺少 {orientation} 布局"))?;
        for (label, value) in [
            ("上边框", variant.frame.top),
            ("右边框", variant.frame.right),
            ("下边框", variant.frame.bottom),
            ("左边框", variant.frame.left),
        ] {
            normalized(value, label)?;
        }
        for layer in &template.shared.layers {
            let base = layer.base();
            let layout = variant.layer_layouts.get(&base.id)
                .ok_or_else(|| format!("{orientation} 布局缺少图层 {}", base.name))?;
            validate_layer_layout(layer, layout)?;
        }
    }
    let total = template.resources.values().try_fold(0usize, |sum, resource| {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&resource.data_base64)
            .map_err(|_| format!("资源 {} 不是有效 Base64", resource.id))?;
        if bytes.len() > MAX_RESOURCE_BYTES {
            return Err(format!("资源 {} 超过 32 MiB", resource.id));
        }
        sum.checked_add(bytes.len()).ok_or_else(|| "模板资源大小溢出".to_string())
    })?;
    if total > MAX_TEMPLATE_RESOURCE_BYTES {
        return Err("模板资源总量超过 128 MiB".to_string());
    }
    Ok(())
}
```

Add `mod watermark_model;` to `lib.rs`, without commands yet.

- [ ] **Step 8: Run focused and regression tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_model
npm run test:frontend
npm run build
```

Expected: all commands PASS.

- [ ] **Step 9: Commit the domain foundation**

```bash
git add package.json src/features/watermark/types.ts src/features/watermark/watermarkUtils.ts tests/watermark-utils.test.mjs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/watermark_model.rs src-tauri/src/lib.rs src-tauri/tests/watermark_model.rs
git commit -m "feat: add watermark domain model"
```

### Task 2: JPG Source Snapshot And Revalidation

**Files:**
- Create: `src-tauri/src/watermark_source.rs`
- Create: `src-tauri/tests/watermark_source.rs`
- Create: `src-tauri/src/watermark_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/features/watermark/types.ts`

- [ ] **Step 1: Write failing source tests**

Cover recursive directory import, explicit file import, EXIF orientation, JPG+RAW behavior, RAW-only skip count, multiple JPG members, symlink refusal, and snapshot change detection:

```rust
#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/watermark_model.rs"]
mod watermark_model;
#[path = "../src/watermark_source.rs"]
mod watermark_source;

use image::{Rgb, RgbImage};
use std::fs;
use watermark_model::WatermarkSourceOrigin;
use watermark_source::{prepare_source, revalidate_photo, SourceInput, WatermarkSourceRequest};

#[test]
fn directory_source_contains_each_jpeg_and_counts_raw_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    fs::create_dir_all(root.join("day")).unwrap();
    RgbImage::from_pixel(120, 80, Rgb([20, 40, 60]))
        .save(root.join("day/A.JPG")).unwrap();
    fs::write(root.join("day/A.NEF"), b"raw").unwrap();
    fs::write(root.join("day/B.CR3"), b"raw").unwrap();

    let snapshot = prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::Directory,
        inputs: vec![SourceInput::Directory { path: root.to_string_lossy().into_owned() }],
    }).unwrap();

    assert_eq!(snapshot.photos.len(), 1);
    assert_eq!(snapshot.skipped_raw_only, 1);
    assert_eq!(snapshot.photos[0].orientation, watermark_model::WatermarkOrientation::Landscape);
    assert!(revalidate_photo(&snapshot.photos[0]).is_ok());
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test watermark_source`

Expected: FAIL because the source module does not exist.

- [ ] **Step 3: Implement secure source preparation**

Implement these boundaries:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum SourceInput {
    Directory { path: String },
    File { path: String },
    RelativePaths { root: String, relative_paths: Vec<String> },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkSourceRequest {
    pub(crate) origin: WatermarkSourceOrigin,
    pub(crate) inputs: Vec<SourceInput>,
}

pub(crate) fn prepare_source(request: WatermarkSourceRequest) -> Result<WatermarkSourceSnapshot, String>;
pub(crate) fn revalidate_photo(photo: &WatermarkSourcePhoto) -> Result<PathBuf, String>;
```

Rules inside `prepare_source`:

- Canonicalize every input root or file.
- Reject non-regular files and symlinks.
- Walk directories with `follow_links(false)`.
- Collect `.jpg/.jpeg` case-insensitively as independent entries.
- Count logical RAW-only groups but never add RAW paths to `photos`.
- Read JPEG dimensions and `ImageDecoder::orientation()`, then classify the corrected width/height.
- Store canonical root, safe relative path, size, modified time, corrected dimensions, and stable ID.
- Sort by lowercased root plus relative path.
- Deduplicate identical canonical JPG paths.

`revalidate_photo` must re-canonicalize the file, ensure it remains under its root, reject symlinks, verify extension, size, and modified time, and return the authorized canonical path.

- [ ] **Step 4: Add the thin Tauri command**

In `watermark_commands.rs`:

```rust
#[tauri::command]
pub(crate) async fn prepare_watermark_source(
    request: crate::watermark_source::WatermarkSourceRequest,
) -> Result<crate::watermark_model::WatermarkSourceSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || crate::watermark_source::prepare_source(request))
        .await
        .map_err(|error| format!("准备水印照片任务异常结束：{error}"))?
}
```

Register `watermark_source`, `watermark_commands`, and `prepare_watermark_source` in `lib.rs`.

- [ ] **Step 5: Run focused and core tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_source
npm run test:core
```

Expected: PASS; existing preview/photo group indexing remains unchanged.

- [ ] **Step 6: Commit source snapshots**

```bash
git add src-tauri/src/watermark_source.rs src-tauri/src/watermark_commands.rs src-tauri/src/lib.rs src-tauri/tests/watermark_source.rs src/features/watermark/types.ts
git commit -m "feat: prepare safe JPG watermark sources"
```

### Task 3: Third Module Navigation, Standalone Import, And Preview Handoff

**Files:**
- Create: `src/features/watermark/WatermarkModule.tsx`
- Create: `src/features/watermark/WatermarkSourcePanel.tsx`
- Create: `src/features/watermark/SendToWatermarkMenu.tsx`
- Create: `src/features/watermark/watermark.css`
- Create: `tests/watermark-ui.test.mjs`
- Modify: `src/App.tsx`
- Modify: `src/app/AppShell.tsx`
- Modify: `src/features/preview/PreviewModule.tsx`
- Modify: `src/features/preview/PhotoContextMenu.tsx`

- [ ] **Step 1: Write failing structural and transfer tests**

Test that the third module is registered, `App` owns the transfer, and the preview exposes three scopes:

```js
import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

test("watermark is a first-class app module", () => {
  const shell = fs.readFileSync(new URL("../src/app/AppShell.tsx", import.meta.url), "utf8");
  const app = fs.readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(shell, /"preview" \| "cleanup" \| "watermark"/);
  assert.match(shell, /水印导出/);
  assert.match(app, /<WatermarkModule/);
});

test("preview can send current photo directory or filter snapshot", () => {
  const preview = fs.readFileSync(new URL("../src/features/preview/PreviewModule.tsx", import.meta.url), "utf8");
  assert.match(preview, /currentPhoto/);
  assert.match(preview, /currentDirectory/);
  assert.match(preview, /currentFilter/);
  assert.match(preview, /onSendToWatermark/);
});
```

- [ ] **Step 2: Run and verify failure**

Run: `npm run test:frontend`

Expected: FAIL because no watermark module is registered.

- [ ] **Step 3: Add typed transfer ownership in `App.tsx`**

Use a monotonically changing transfer ID so sending the same list twice still triggers intake:

```tsx
const [watermarkTransfer, setWatermarkTransfer] = useState<WatermarkTransferDraft | null>(null);

function sendToWatermark(draft: Omit<WatermarkTransferDraft, "transferId">) {
  setWatermarkTransfer({ ...draft, transferId: crypto.randomUUID() });
  setActiveModule("watermark");
}
```

Pass `onSendToWatermark={sendToWatermark}` to `PreviewModule` and `transfer={watermarkTransfer}` to `WatermarkModule`. Add a third `module-panel` without unmounting inactive modules.

- [ ] **Step 4: Add the module shell and standalone intake**

`WatermarkModule` must:

- Accept `{ active, transfer }`.
- Call `prepare_watermark_source` once per new `transferId`.
- Use Tauri dialog for directory selection.
- Listen to drag/drop only while active and accept any mix of directories and JPG/JPEG files.
- Reject unsupported individual files with a Chinese summary, not an empty state.
- Render a source panel that lists JPG count, RAW-only skipped count, root paths, and directory tree.

The first working composition is:

```tsx
export function WatermarkModule({ active, transfer }: WatermarkModuleProps) {
  const [snapshot, setSnapshot] = useState<WatermarkSourceSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <section className="watermark-module" aria-label="水印导出">
      <header className="watermark-header">
        <div className="module-heading"><Stamp aria-hidden="true" /><div><strong>水印导出</strong><span>边框、署名与发布副本</span></div></div>
        <button className="secondary-command" type="button" onClick={chooseDirectory} disabled={busy}>
          <FolderOpen aria-hidden="true" size={17} />选择目录
        </button>
      </header>
      <WatermarkSourcePanel snapshot={snapshot} busy={busy} error={error} onChooseDirectory={chooseDirectory} />
    </section>
  );
}
```

- [ ] **Step 5: Add preview handoff scopes**

`SendToWatermarkMenu` emits one of these immutable drafts:

```ts
type WatermarkTransferScope = "currentPhoto" | "currentDirectory" | "currentFilter";

interface WatermarkTransferDraft {
  transferId: string;
  origin: "preview-photo" | "preview-directory" | "preview-filter";
  inputs: Array<{ kind: "relativePaths"; root: string; relativePaths: string[] }>;
}
```

Build the relative path list at click time from `selectedAsset`, `directoryAssets`, or `visibleAssets`; include every `jpegPaths` member and no RAW/XMP path. Add current-photo handoff to `PhotoContextMenu` and all three scopes to the preview header menu.

- [ ] **Step 6: Add responsive module-shell styles**

Use the existing design tokens. The empty/source state must remain usable at the app's `860x620` minimum. Keep cards at `6px` radius or less and use an unframed full-height work surface.

- [ ] **Step 7: Verify Phase 1**

Run:

```bash
npm run test:frontend
npm run build
npm run test:core
```

Manual check with `npm run tauri -- dev`:

1. Third navigation item appears and preserves preview/cleanup state.
2. Directory picker and drag/drop create the same source list.
3. Preview scopes send the exact visible JPG list.
4. RAW-only counts appear but RAW does not enter the list.

- [ ] **Step 8: Commit the Phase 1 vertical slice**

```bash
git add src/App.tsx src/app/AppShell.tsx src/features/preview/PreviewModule.tsx src/features/preview/PhotoContextMenu.tsx src/features/watermark tests/watermark-ui.test.mjs
git commit -m "feat: add watermark source workspace"
```

## Phase 2: Shared Renderer And Live Preview

### Task 4: Geometry, Canvas, And Anchor Spaces

**Files:**
- Create: `src-tauri/src/watermark_geometry.rs`
- Create: `src-tauri/tests/watermark_geometry.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/watermark_model.rs`

- [ ] **Step 1: Write failing geometry tests**

Test natural canvas, fixed canvas ratio, asymmetric borders, photo alignment, three anchor spaces, frame edge bands, and clipping:

```rust
#[test]
fn asymmetric_frame_resolves_from_short_edge_ratios() {
    let layout = resolve_layout(ResolvedLayoutInput {
        photo_width: 1200,
        photo_height: 800,
        output_long_edge: None,
        canvas_ratio: None,
        frame: FrameInsets { top: 0.05, right: 0.10, bottom: 0.20, left: 0.10 },
        align_x: 0.5,
        align_y: 0.5,
        photo_scale: 1.0,
    }).unwrap();
    assert_eq!(layout.photo_rect.width, 1200);
    assert_eq!(layout.photo_rect.height, 800);
    assert_eq!(layout.canvas.width, 1360);
    assert_eq!(layout.canvas.height, 1000);
}

#[test]
fn bottom_frame_anchor_never_uses_the_photo_rect() {
    let layout = sample_layout();
    let region = anchor_region(&layout, AnchorSpace::Frame, Some(FrameEdge::Bottom)).unwrap();
    assert!(region.y >= layout.photo_rect.bottom());
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test watermark_geometry`

Expected: FAIL because geometry functions do not exist.

- [ ] **Step 3: Implement integer-safe geometry**

Use `u64` for intermediate multiplication and checked conversions to `u32`. Resolve ratios against the corrected photo short edge, then calculate the canvas and photo rect. Expose:

```rust
pub(crate) fn resolve_layout(input: ResolvedLayoutInput) -> Result<ResolvedLayout, String>;
pub(crate) fn anchor_region(
    layout: &ResolvedLayout,
    space: AnchorSpace,
    edge: Option<FrameEdge>,
) -> Result<PixelRect, String>;
pub(crate) fn normalized_placement(
    region: PixelRect,
    placement: &LayerPlacement,
) -> Result<ResolvedLayerPlacement, String>;
```

Reject zero-sized outputs, canvas edges over 32768 pixels, total pixels over 200 MP for export, and over 16 MP for preview. Preview scaling happens before layout resolution so the same normalized geometry is used at both sizes.

- [ ] **Step 4: Run tests and commit**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_geometry
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

```bash
git add src-tauri/src/watermark_geometry.rs src-tauri/src/watermark_model.rs src-tauri/src/lib.rs src-tauri/tests/watermark_geometry.rs
git commit -m "feat: resolve watermark canvas geometry"
```

### Task 5: Color Pipeline, Decorative Backgrounds, And Photo Effects

**Files:**
- Create: `src-tauri/src/watermark_color.rs`
- Create: `src-tauri/src/watermark_render.rs`
- Create: `src-tauri/tests/watermark_render.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add pinned rendering dependencies**

Update dependencies exactly:

```toml
cosmic-text = "0.19.0"
image = { version = "0.25.10", default-features = false, features = ["jpeg", "png"] }
imageproc = { version = "=0.26.2", default-features = false }
little_exif = "0.6.23"
moxcms = "0.8.1"
```

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS and `Cargo.lock` records the exact resolved graph.

- [ ] **Step 2: Write failing color and base-render tests**

Cover sRGB conversion, preserved compatible ICC, transparent PNG canvas, solid/sample/linear/radial backgrounds, blurred-photo extension, custom image background, photo scale/alignment, rounded corners, stroke, and shadow.

The ICC test must generate a Display P3 profile with `moxcms`, convert a known red sample to sRGB, and assert the result differs from relabeling the original bytes.

- [ ] **Step 3: Implement the color pipeline**

Expose this explicit behavior. All compositing uses normalized linear-sRGB floats; output conversion happens only after backgrounds, photo effects, text, and image layers are complete:

```rust
pub(crate) enum OutputColorSpace {
    Srgb,
    SourceIcc(Vec<u8>),
}

pub(crate) fn linear_srgb_profile() -> moxcms::ColorProfile {
    let mut profile = moxcms::ColorProfile::new_srgb();
    let linear = Some(moxcms::curve_from_gamma(1.0));
    profile.red_trc = linear.clone();
    profile.green_trc = linear.clone();
    profile.blue_trc = linear;
    profile
}

pub(crate) fn source_to_linear_srgb(
    pixels: &[u8],
    source_icc: Option<&[u8]>,
) -> Result<Vec<f32>, String>;

pub(crate) fn linear_srgb_to_output(
    pixels: &[f32],
    output: &OutputColorSpace,
) -> Result<(Vec<u8>, Vec<u8>), String>;
```

Both conversion functions use `ColorProfile::create_transform_f32(Layout::Rgb, ...)` and `TransformExecutor::transform`. Convert source `u8` samples to `0..=1` floats before the first transform. Clamp only after the final transform; return the encoded destination ICC beside the output pixels.

Rules:

- No ICC means assume sRGB.
- Source photos and embedded image resources convert from their ICC profile, or assumed sRGB, into linear sRGB before compositing.
- CSS-style template colors are parsed as sRGB and linearized before compositing.
- `sRGB` output converts the finished linear canvas to encoded sRGB and embeds the generated sRGB profile.
- `preserve` output converts the finished linear canvas to the parsed source RGB profile and embeds the original source profile.
- Unsupported/non-RGB ICC in preserve mode produces a blocking warning and offers sRGB fallback; it never relabels pixels.

- [ ] **Step 4: Implement ordered base rendering**

`render_base` must return RGBA plus source metadata context:

```rust
pub(crate) struct RenderedCanvas {
    pub(crate) image: image::Rgba32FImage,
    pub(crate) layout: ResolvedLayout,
    pub(crate) source_icc: Option<Vec<u8>>,
}

pub(crate) fn render_base(
    source: &Path,
    variant: &LayoutVariant,
    photo_override: Option<&PhotoPlacementOverride>,
    target: RenderTarget,
) -> Result<RenderedCanvas, String>;
```

Render in this order: background, blurred/custom background, photo shadow, rounded photo mask, photo pixels, stroke. Keep the `Rgba32FImage` premultiplied while blending. Gradient interpolation, blur composition, shadows, and layer blending therefore remain linear-light operations.

- [ ] **Step 5: Run focused tests and security audit**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_render
cargo test --manifest-path src-tauri/Cargo.toml
cargo tree --manifest-path src-tauri/Cargo.toml -i imageproc
```

Expected: PASS; `cargo tree` resolves `imageproc` exactly to `0.26.2`, never `0.26.0` or `0.26.1`.

- [ ] **Step 6: Commit base rendering**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/watermark_color.rs src-tauri/src/watermark_render.rs src-tauri/src/lib.rs src-tauri/tests/watermark_render.rs
git commit -m "feat: render watermark canvas and borders"
```

### Task 6: EXIF Field Resolution And Output Metadata Model

**Files:**
- Create: `src-tauri/src/watermark_metadata.rs`
- Create: `src-tauri/tests/watermark_metadata.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing metadata tests**

Create JPEG fixtures in the test using `little_exif` and cover camera make/model, lens, focal length, aperture, shutter, ISO, date/time, author, copyright, missing-field separator collapse, orientation normalization, GPS removal, serial removal, preserve, privacy, and remove policies.

```rust
#[test]
fn missing_exif_fields_collapse_separators() {
    let values = ExifValues {
        camera_model: Some("Nikon Z8".into()),
        lens_model: None,
        aperture: Some("f/2.8".into()),
        ..ExifValues::default()
    };
    assert_eq!(format_exif_fields(&[ExifField::CameraModel, ExifField::LensModel, ExifField::Aperture], " · ", &values, None), "Nikon Z8 · f/2.8");
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test watermark_metadata`

Expected: FAIL because the metadata adapter does not exist.

- [ ] **Step 3: Implement a panic-safe EXIF adapter**

Read metadata from verified in-memory JPEG bytes inside `catch_unwind`, then map supported tags to `ExifValues`. Never let malformed metadata unwind across a Tauri command.

```rust
pub(crate) fn read_exif_values(path: &Path) -> Result<ExifValues, String> {
    let bytes = fs::read(path).map_err(|error| format!("无法读取 JPG 元数据：{error}"))?;
    let parsed = std::panic::catch_unwind(|| {
        little_exif::metadata::Metadata::new_from_vec(
            &bytes,
            little_exif::filetype::FileExtension::JPEG,
        )
    }).map_err(|_| "JPG EXIF 解析异常".to_string())?;
    let metadata = parsed.map_err(|error| format!("无法解析 JPG EXIF：{error}"))?;
    Ok(ExifValues::from_metadata(&metadata))
}
```

Implement `apply_metadata_policy` so privacy removes all GPS group tags plus body/camera/lens serial tags, preserve retains supported tags, remove returns an empty metadata set, and every non-empty policy sets output width/height and Orientation=1 while dropping embedded EXIF thumbnails.

Parse standard XMP APP1 and IPTC APP13 segments with a bounded JPEG segment parser. Preserve mode copies them after updating/removing stale dimension and orientation values. Privacy mode builds only allowed author/copyright/date values and removes location/serial fields. PNG writes only target-supported EXIF fields and does not claim IPTC/XMP preservation.

- [ ] **Step 4: Run metadata and regression tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_metadata
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS, including corrupt metadata cases without panic.

- [ ] **Step 5: Commit metadata handling**

```bash
git add src-tauri/src/watermark_metadata.rs src-tauri/src/lib.rs src-tauri/tests/watermark_metadata.rs
git commit -m "feat: resolve watermark EXIF metadata"
```

### Task 7: Text, EXIF Text, And Image Layers

**Files:**
- Create: `src-tauri/src/watermark_text.rs`
- Modify: `src-tauri/src/watermark_render.rs`
- Modify: `src-tauri/src/watermark_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/watermark_render.rs`
- Create: `src-tauri/resources/fonts/NotoSansCJKsc-Regular.otf`
- Create: `src-tauri/resources/fonts/OFL.txt`
- Create: `THIRD_PARTY_NOTICES.md`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Add the licensed font resource before layer tests**

Download the official Noto CJK Simplified Chinese regular font and its OFL text from the pinned upstream commit:

```bash
curl --fail --location https://raw.githubusercontent.com/notofonts/noto-cjk/f8d157532fbfaeda587e826d4cd5b21a49186f7c/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf --output src-tauri/resources/fonts/NotoSansCJKsc-Regular.otf
curl --fail --location https://raw.githubusercontent.com/notofonts/noto-cjk/f8d157532fbfaeda587e826d4cd5b21a49186f7c/Sans/LICENSE --output src-tauri/resources/fonts/OFL.txt
```

Record the upstream repository, commit `f8d157532fbfaeda587e826d4cd5b21a49186f7c`, filename, hashes, and OFL-1.1 license in `THIRD_PARTY_NOTICES.md`.

Add to `tauri.conf.json`:

```json
"resources": [
  "resources/fonts/NotoSansCJKsc-Regular.otf",
  "resources/fonts/OFL.txt"
]
```

Verify: `shasum -a 256 src-tauri/resources/fonts/NotoSansCJKsc-Regular.otf`

Expected font hash:

```text
2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b
```

Expected license hash:

```text
6a73f9541c2de74158c0e7cf6b0a58ef774f5a780bf191f2d7ec9cc53efe2bf2
```

- [ ] **Step 2: Write failing layer tests**

Add cases for Chinese/Latin mixed text, multiline alignment, font fallback warning, stroke, shadow, rotation, opacity, z-index, photo/frame/canvas anchors, EXIF missing values, PNG alpha image layer, and corrupted embedded resource rejection.

- [ ] **Step 3: Implement font discovery and text rendering**

`FontCatalog` loads the bundled font first and system fonts second. Expose stable family names and fallback status:

```rust
pub(crate) struct ResolvedFont {
    pub(crate) requested_family: String,
    pub(crate) resolved_family: String,
    pub(crate) used_fallback: bool,
}

pub(crate) fn list_fonts(resource_dir: &Path) -> Result<Vec<FontSummary>, String>;
pub(crate) fn measure_text(request: &TextRenderRequest, catalog: &mut FontCatalog) -> Result<TextMetrics, String>;
pub(crate) fn draw_text(canvas: &mut Rgba32FImage, request: &TextRenderRequest, catalog: &mut FontCatalog) -> Result<ResolvedFont, String>;
```

Use `cosmic_text::FontSystem`, `Buffer`, `Attrs`, `Metrics`, `Shaping::Advanced`, and `SwashCache`. Derive the pixel font size from the active variant's `VariantLayerLayout.fontSizeRatio * canvas_short_edge`; reject `null` for text/EXIF layers and require `null` for image layers. Draw shadow, stroke offsets, then fill using premultiplied alpha.

- [ ] **Step 4: Implement ordered layer rendering**

Add:

```rust
pub(crate) fn render_request(
    source: &Path,
    request: &WatermarkRenderRequest,
    resource_dir: &Path,
) -> Result<RenderOutcome, String>;
```

Select the corrected orientation variant, render base, sort visible layers by `(zIndex, id)`, resolve `layerLayouts[layer.id]` for each shared layer, resolve EXIF text, decode bounded image resources, apply rotation/opacity, composite, and return warnings including fallback fonts and clipped layers. A visible shared layer without a layout in the active variant is a validation error rather than an implicit default.

- [ ] **Step 5: Add font-list command and verify**

Add `list_watermark_fonts` to `watermark_commands.rs` and register it in `lib.rs`.

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_render
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri -- build --debug
```

Expected: tests PASS and the debug bundle contains the Noto font resource.

- [ ] **Step 6: Commit structured layer rendering**

```bash
git add src-tauri/resources THIRD_PARTY_NOTICES.md src-tauri/tauri.conf.json src-tauri/src/watermark_text.rs src-tauri/src/watermark_render.rs src-tauri/src/watermark_commands.rs src-tauri/src/lib.rs src-tauri/tests/watermark_render.rs
git commit -m "feat: render text EXIF and logo layers"
```

### Task 8: Live Preview Command And Frontend Cache

**Files:**
- Create: `src/features/watermark/watermarkPreviewCache.ts`
- Create: `src/features/watermark/WatermarkCanvas.tsx`
- Modify: `src/features/watermark/WatermarkModule.tsx`
- Modify: `src/features/watermark/WatermarkSourcePanel.tsx`
- Modify: `src/features/watermark/watermark.css`
- Modify: `tests/watermark-utils.test.mjs`
- Modify: `src-tauri/src/watermark_commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing preview-cache tests**

Test stable request keys, one shared promise per key, stale response suppression, root/template invalidation, and URL revocation:

```js
test("watermark preview cache ignores stale generations", async () => {
  const cache = new WatermarkPreviewCache((url) => released.push(url));
  const first = cache.begin("photo-a", "hash-1");
  const second = cache.begin("photo-a", "hash-2");
  assert.equal(cache.accept(first, "blob:first"), false);
  assert.equal(cache.accept(second, "blob:second"), true);
  assert.deepEqual(released, ["blob:first"]);
});
```

- [ ] **Step 2: Run and verify failure**

Run: `npm run test:frontend`

Expected: FAIL because the cache does not exist.

- [ ] **Step 3: Add binary preview command**

`render_watermark_preview` accepts a source photo snapshot, full `WatermarkRenderRequest`, and `maxEdge` constrained to `256..=2400`. It revalidates the source, calls `render_request` with preview limits, always encodes PNG so alpha preview is lossless, and returns one efficient binary envelope through `tauri::ipc::Response`.

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatermarkPreviewHeader {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn preview_envelope(header: &WatermarkPreviewHeader, png: &[u8]) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(header)
        .map_err(|error| format!("无法序列化水印预览信息：{error}"))?;
    let length = u32::try_from(json.len())
        .map_err(|_| "水印预览信息过大".to_string())?;
    let mut output = Vec::with_capacity(4 + json.len() + png.len());
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(&json);
    output.extend_from_slice(png);
    Ok(output)
}
```

The frontend reads the first four bytes as a big-endian JSON-header length, parses the next bytes as UTF-8 JSON, and creates an `image/png` object URL from the remaining bytes. Reject envelopes shorter than four bytes or with an out-of-bounds header length. Do not return filesystem paths to generated preview files.

- [ ] **Step 4: Implement the frontend cache and canvas loading state**

Hash the canonical JSON render request with a stable key-order serializer. Debounce editor changes by 80 ms, but switch photos immediately. Keep the previous preview visible under a small loading indicator until the latest response decodes. Revoke URLs on invalidation and module teardown.

All source thumbnails preload with the existing `preloadPreviewRequests` worker pool. Full edited previews cache only current and two neighbors on each side.

- [ ] **Step 5: Verify Phase 2**

Run:

```bash
npm run test:frontend
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Manual check: switch through at least 30 JPGs with arrow keys; selected filmstrip item stays visible and cached neighbors display without a blank canvas.

- [ ] **Step 6: Commit live preview**

```bash
git add src/features/watermark/watermarkPreviewCache.ts src/features/watermark/WatermarkCanvas.tsx src/features/watermark/WatermarkModule.tsx src/features/watermark/WatermarkSourcePanel.tsx src/features/watermark/watermark.css tests/watermark-utils.test.mjs src-tauri/src/watermark_commands.rs src-tauri/src/lib.rs
git commit -m "feat: preview watermark renders live"
```

## Phase 3: Structured Editor

### Task 9: Editor Reducer, Undo/Redo, Variants, And Per-Photo Overrides

**Files:**
- Create: `src/features/watermark/watermarkEditorState.ts`
- Create: `src/features/watermark/WatermarkLeaveDialog.tsx`
- Modify: `tests/watermark-utils.test.mjs`
- Modify: `src/features/watermark/WatermarkModule.tsx`
- Modify: `src/App.tsx`
- Modify: `src/app/AppShell.tsx`

- [ ] **Step 1: Write failing reducer tests**

Cover add/update/delete/reorder/lock/visibility, bounded undo/redo, coalesced drag commands, shared content update across variants, current-variant-only placement update, orientation selection, per-photo placement override, clear override, and template switch history boundary.

```js
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
  assert.notEqual(next.present.template.variants.portrait.layerLayouts.signature.placement.x, 0.8);
});
```

- [ ] **Step 2: Run and verify failure**

Run: `npm run test:frontend`

Expected: FAIL because the reducer does not exist.

- [ ] **Step 3: Implement command history**

Use this state shape:

```ts
export interface WatermarkEditorState {
  past: WatermarkEditorDocument[];
  present: WatermarkEditorDocument;
  future: WatermarkEditorDocument[];
  activeLayerId: string | null;
  activeOrientation: WatermarkOrientation;
  historyGroup: string | null;
}
```

Keep at most 100 committed documents. Pointer-move updates with the same `historyGroup` replace the latest present state; pointer-up closes the group so a drag is one undo step. Photo selection and zoom are view state and do not enter document history. Template replacement clears history after explicit confirmation.

Track `dirtyTemplate` after any edit not saved as a template and `unexportedChanges` after any edit/source change newer than the last successful export. `WatermarkModule` reports `hasUnsavedWork` to `App`. `App` intercepts navigation away from `watermark`, stores the requested module, and opens `WatermarkLeaveDialog`; confirming discards the current task and completes the requested navigation.

Subscribe to `getCurrentWindow().onCloseRequested` while unsaved work exists. Prevent the first close request and show the same dialog. A confirmed app close sets a one-shot bypass ref and calls `getCurrentWindow().close()` again. Do not use a browser-only `beforeunload` prompt inside Tauri.

- [ ] **Step 4: Run tests and integrate into module state**

Run:

```bash
npm run test:frontend
npm run build
```

Expected: PASS.

- [ ] **Step 5: Commit editor state**

```bash
git add src/features/watermark/watermarkEditorState.ts src/features/watermark/WatermarkLeaveDialog.tsx src/features/watermark/WatermarkModule.tsx src/App.tsx src/app/AppShell.tsx tests/watermark-utils.test.mjs
git commit -m "feat: manage watermark editor history"
```

### Task 10: Three-Pane Workspace, Filmstrip, And Collapsible Panels

**Files:**
- Create: `src/features/watermark/WatermarkHeader.tsx`
- Create: `src/features/watermark/WatermarkTemplatePanel.tsx`
- Create: `src/features/watermark/WatermarkInspector.tsx`
- Create: `src/features/watermark/WatermarkFilmstrip.tsx`
- Modify: `src/features/watermark/WatermarkModule.tsx`
- Modify: `src/features/watermark/WatermarkSourcePanel.tsx`
- Modify: `src/features/watermark/watermark.css`
- Modify: `tests/watermark-ui.test.mjs`

- [ ] **Step 1: Write failing workspace tests**

Assert the real panel components, collapse preferences, tour targets, filmstrip auto-scroll helper, orientation indicator, compare command, and icon-only accessible labels exist.

- [ ] **Step 2: Run and verify failure**

Run: `npm run test:frontend`

Expected: FAIL because the workspace components do not exist.

- [ ] **Step 3: Compose the approved B layout**

Use one grid, not nested decorative cards:

```tsx
<section className={workspaceClass}>
  <WatermarkHeader />
  <div className="watermark-workspace">
    <aside className="watermark-left-panel" data-watermark-tour="sources-templates">
      {leftTab === "photos" ? <WatermarkSourcePanel /> : <WatermarkTemplatePanel />}
    </aside>
    <main className="watermark-stage" data-watermark-tour="canvas">
      <WatermarkCanvas />
    </main>
    <aside className="watermark-inspector-panel" data-watermark-tour="inspector">
      <WatermarkInspector />
    </aside>
  </div>
  <WatermarkFilmstrip data-watermark-tour="filmstrip" />
</section>
```

Persist left/right collapsed preferences under versioned `localStorage` keys. Add a single immersive command that collapses both internal panels and the global module sidebar; leaving immersive mode restores the prior three states.

- [ ] **Step 4: Implement filmstrip navigation and preload status**

Reuse `filmstripScrollTarget`. Support click, arrow keys, Home/End, and visible `current / total`. The selected item ref must update after every keyboard change. Show orientation and warning markers without changing item dimensions.

- [ ] **Step 5: Add responsive rules**

- `>=1280px`: left 248px, right 304px.
- `1000-1279px`: left 210px, right 280px.
- `<1000px`: auto-collapse left panel and expose it as a tool drawer; keep inspector and canvas usable.
- At app minimum `860x620`: inspector is a right overlay drawer, filmstrip remains 86px high, and no toolbar text overlaps.
- Browser zoom at 200% uses the same compact rules without horizontal page overflow.

- [ ] **Step 6: Verify and commit**

Run:

```bash
npm run test:frontend
npm run build
```

Manual check at `1180x780`, `860x620`, `1440x900`, and 200% zoom.

```bash
git add src/features/watermark tests/watermark-ui.test.mjs
git commit -m "feat: build watermark studio workspace"
```

### Task 11: Direct Manipulation, Snapping, And Precise Inspector Controls

**Files:**
- Modify: `src/features/watermark/WatermarkCanvas.tsx`
- Modify: `src/features/watermark/WatermarkInspector.tsx`
- Modify: `src/features/watermark/watermarkEditorState.ts`
- Modify: `src/features/watermark/watermarkUtils.ts`
- Modify: `src/features/watermark/watermark.css`
- Modify: `tests/watermark-utils.test.mjs`

- [ ] **Step 1: Write failing snapping and conversion tests**

Test viewport-to-normalized coordinates, zoom-independent drag, locked layer refusal, center/edge/peer snapping, Shift bypass, keyboard nudge, rotation normalization, and clamped numeric inputs.

```js
test("canvas drag converts viewport pixels into anchor-relative coordinates", () => {
  assert.deepEqual(viewportDeltaToNormalized({ dx: 20, dy: -10 }, { width: 400, height: 200 }), {
    x: 0.05,
    y: -0.05,
  });
});
```

- [ ] **Step 2: Run and verify failure**

Run: `npm run test:frontend`

Expected: FAIL on missing coordinate helpers.

- [ ] **Step 3: Implement pointer interaction**

Use Pointer Events with pointer capture. On pointer down, record the resolved anchor rect and initial placement. On move, convert deltas to normalized anchor coordinates, apply snap candidates within 6 screen pixels, and dispatch one grouped reducer command. On pointer up/cancel, close the group.

Handles:

- Four corner scale handles keep aspect ratio.
- Rotation handle normalizes to `-180..180`.
- Locked layers show selection but no handles.
- Frame layers cannot switch edge by accidental drag; edge changes only in inspector.

- [ ] **Step 4: Implement inspector controls**

Provide feature-complete controls for:

- Layer list: select, show/hide, lock, duplicate, delete, move up/down.
- Text: content, font, weight, size, color swatch, alignment, spacing, line height, stroke, shadow.
- EXIF: ordered field list, separator, prefix/suffix, missing-value behavior.
- Image: choose resource, contain/cover, opacity.
- Placement: anchor space, frame edge, X/Y, width, rotation, opacity.
- Border/canvas: four linked/unlinked insets, ratio, alignment, scale, rounded corners, stroke, shadow, background kind and parameters.
- Orientation segmented control: horizontal, vertical, square.
- Per-photo override: photo scale/position and “清除单张调整”.

- [ ] **Step 5: Verify Phase 3 and commit**

Run:

```bash
npm run test:frontend
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Manual check mouse drag, trackpad, keyboard-only, 50%-400% zoom, undo/redo after drag, and variant switching.

```bash
git add src/features/watermark tests/watermark-utils.test.mjs
git commit -m "feat: edit watermark layers precisely"
```

## Phase 4: Templates And Decorative Presets

### Task 12: Built-In And Portable Local Templates

**Files:**
- Create: `src-tauri/src/watermark_templates.rs`
- Create: `src-tauri/tests/watermark_templates.rs`
- Modify: `src-tauri/src/watermark_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/features/watermark/WatermarkTemplatePanel.tsx`
- Modify: `src/features/watermark/WatermarkModule.tsx`
- Modify: `src/features/watermark/types.ts`
- Modify: `tests/watermark-ui.test.mjs`

- [ ] **Step 1: Write failing template persistence tests**

Cover six built-ins, immutable built-ins, local save/copy/rename/delete, atomic database writes, JSON extension, embedded base64 resources, resource checksum, 32 MiB item limit, 128 MiB total limit, 16384-pixel edge, 100 MP decode limit, path traversal refusal, remote URL refusal, missing resource reference, old-version migration, and future-version refusal.

- [ ] **Step 2: Run and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test watermark_templates`

Expected: FAIL because template storage does not exist.

- [ ] **Step 3: Implement built-in templates**

Return exactly these IDs and names with three complete variants each:

```rust
const BUILTIN_TEMPLATE_IDS: [(&str, &str); 6] = [
    ("minimal-signature", "极简署名"),
    ("white-exif-frame", "白色 EXIF 底边框"),
    ("dark-gallery-frame", "深色画廊边框"),
    ("gradient-magazine", "渐变杂志边框"),
    ("blurred-extension", "照片模糊延展"),
    ("transparent-logo", "透明 Logo 角标"),
];
```

The transparent-logo template embeds the project-owned `src-tauri/icons/icon.png` at compile time with `include_bytes!("../icons/icon.png")`, records it as a normal validated PNG resource, and labels the layer “替换为你的 Logo”. Built-ins use the bundled font and no filesystem-dependent resource.

- [ ] **Step 4: Implement local storage and portable JSON**

Use `watermark-templates.json` in the Tauri app data directory for local templates. Reuse the repository's `NamedTempFile`, `sync_all`, and no-symlink checks from rating rules.

Import/export model:

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WatermarkTemplateFile {
    schema_version: u16,
    template: WatermarkTemplate,
}
```

Resources serialize as MIME, SHA-256, dimensions, and base64. Decode and hash every resource before accepting it. Import returns a new local ID instead of silently replacing an existing template.

- [ ] **Step 5: Register commands and complete template UI**

Commands:

- `list_watermark_templates`
- `save_watermark_template`
- `delete_watermark_template`
- `import_watermark_template`
- `export_watermark_template`

The UI must confirm delete, warn before replacing unsaved editor state, allow “另存为”, and keep built-in controls read-only.

- [ ] **Step 6: Verify Phase 4 and commit**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_templates
npm run test:frontend
npm run build
npm run test:core
```

Manual check export/import on a template containing a custom PNG logo and background; disconnect or rename the original assets and verify the imported template still renders.

```bash
git add src-tauri/src/watermark_templates.rs src-tauri/src/watermark_commands.rs src-tauri/src/lib.rs src-tauri/tests/watermark_templates.rs src/features/watermark tests/watermark-ui.test.mjs
git commit -m "feat: manage portable watermark templates"
```

## Phase 5: Safe JPEG And PNG Export

### Task 13: Output Planning, Naming, Color, Metadata, And Atomic Writes

**Files:**
- Create: `src-tauri/src/watermark_output.rs`
- Create: `src-tauri/tests/watermark_output.rs`
- Modify: `src-tauri/src/watermark_render.rs`
- Modify: `src-tauri/src/watermark_metadata.rs`
- Modify: `src-tauri/src/watermark_commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing output tests**

Cover:

- JPEG and PNG extension.
- Original size and longest-edge resize.
- No-upscale default and explicit upscale.
- JPEG quality `1..=100` validation.
- PNG alpha and JPEG white/custom flatten color.
- sRGB and compatible source ICC.
- Preserve/privacy/remove metadata.
- Single-root default output and multi-root required selection.
- Suffix sanitization on Windows/macOS.
- Sequence, skip, and overwrite-output collision policies.
- Source/output equality refusal.
- Existing non-output overwrite refusal.
- Temporary file cleanup after encoding failure.
- Reopen/verify dimensions and format before atomic commit.

- [ ] **Step 2: Run and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test watermark_output`

Expected: FAIL because output planning does not exist.

- [ ] **Step 3: Implement output planning**

Build an immutable plan before encoding:

```rust
#[derive(Debug, Clone)]
pub(crate) struct PlannedWatermarkOutput {
    pub(crate) photo: WatermarkSourcePhoto,
    pub(crate) target_path: PathBuf,
    pub(crate) format: WatermarkOutputFormat,
    pub(crate) target_width: u32,
    pub(crate) target_height: u32,
    pub(crate) collision: PlannedCollision,
}

pub(crate) fn plan_outputs(
    snapshot: &WatermarkSourceSnapshot,
    settings: &WatermarkOutputSettings,
) -> Result<Vec<PlannedWatermarkOutput>, String>;
```

`overwriteOutput` is allowed only when the existing target is a regular non-symlink file inside the selected output directory and is not any source path. It still writes a sibling temp file and atomically replaces only after validation.

V1 writes every generated copy directly into the selected output directory instead of recreating source subdirectories. Duplicate source names are handled by the selected sequence/skip/overwrite-output policy, and the confirmation dialog shows the resolved names before execution.

- [ ] **Step 4: Implement JPEG/PNG encoding and metadata commit**

JPEG uses `JpegEncoder::new_with_quality`; PNG uses `PngEncoder`. Call `set_icc_profile` before encoding. Encode to a `NamedTempFile` in the destination directory, flush and sync, reopen through `image`, verify target format/dimensions, apply metadata, reopen metadata for policy verification, then persist.

Return a typed item outcome rather than a string-only result:

```rust
pub(crate) struct WatermarkOutputResult {
    pub(crate) photo_id: String,
    pub(crate) target_path: String,
    pub(crate) status: WatermarkOutputStatus,
    pub(crate) message: String,
    pub(crate) size_bytes: Option<u64>,
}
```

- [ ] **Step 5: Run tests and commit**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_output
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS with no temp files left in failure cases.

```bash
git add src-tauri/src/watermark_output.rs src-tauri/src/watermark_render.rs src-tauri/src/watermark_metadata.rs src-tauri/src/watermark_commands.rs src-tauri/src/lib.rs src-tauri/tests/watermark_output.rs
git commit -m "feat: write safe watermark copies"
```

### Task 14: Bounded Export Queue, Progress, Cancellation, And Retry

**Files:**
- Create: `src-tauri/src/watermark_export.rs`
- Create: `src-tauri/tests/watermark_export.rs`
- Modify: `src-tauri/src/watermark_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/features/watermark/types.ts`

- [ ] **Step 1: Write failing queue tests**

Use an injectable `OutputExecutor` test double to cover bounded concurrency, stable result ordering, single-item failure isolation, cancel-before-start, cancel-during-active-item, active item completion, no new work after cancellation, completed-copy retention, disk-space preflight, event counts, duplicate task refusal, and failed-only retry.

- [ ] **Step 2: Run and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test watermark_export`

Expected: FAIL because queue state does not exist.

- [ ] **Step 3: Implement managed task state**

```rust
#[derive(Default)]
pub(crate) struct WatermarkExportStore {
    tasks: Mutex<HashMap<String, Arc<ExportTaskControl>>>,
}

pub(crate) struct ExportTaskControl {
    cancelled: AtomicBool,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum WatermarkExportEvent {
    Started { task_id: String, total: usize },
    ItemStarted { task_id: String, photo_id: String, index: usize },
    ItemFinished { task_id: String, result: WatermarkOutputResult },
    Finished { task_id: String, summary: WatermarkExportSummary },
}
```

Concurrency is `min(4, max(1, available_parallelism / 2))`. Check cancellation before claiming each next item. Do not interrupt an atomic file commit. Store completed results until the frontend acknowledges them; do not resume tasks after app restart.

- [ ] **Step 4: Add streaming commands**

- `start_watermark_export(request, on_event: tauri::ipc::Channel<WatermarkExportEvent>)`
- `cancel_watermark_export(task_id)`
- `retry_watermark_export_failures(task_id, on_event)`
- `reveal_watermark_export(task_id)`
- `acknowledge_watermark_export(task_id)`

Register `WatermarkExportStore::default()` with `manage` in `lib.rs`.

`reveal_watermark_export` accepts only a completed task ID, obtains the canonical output directory already stored in that task, and opens it with the existing platform-specific reveal helper. It never accepts an arbitrary frontend path.

- [ ] **Step 5: Run tests and commit**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test watermark_export
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

```bash
git add src-tauri/src/watermark_export.rs src-tauri/src/watermark_commands.rs src-tauri/src/lib.rs src-tauri/tests/watermark_export.rs src/features/watermark/types.ts
git commit -m "feat: execute cancellable watermark exports"
```

### Task 15: Export Confirmation, Progress, Results, And Failed-Only Retry UI

**Files:**
- Create: `src/features/watermark/WatermarkExportDialog.tsx`
- Modify: `src/features/watermark/WatermarkHeader.tsx`
- Modify: `src/features/watermark/WatermarkInspector.tsx`
- Modify: `src/features/watermark/WatermarkModule.tsx`
- Modify: `src/features/watermark/types.ts`
- Modify: `src/features/watermark/watermarkUtils.ts`
- Modify: `src/features/watermark/watermark.css`
- Modify: `tests/watermark-utils.test.mjs`
- Modify: `tests/watermark-ui.test.mjs`

- [ ] **Step 1: Write failing output validation and UI tests**

Test default JPEG settings, PNG alpha availability, filename preview, multi-root output requirement, invalid suffix, quality/size validation, metadata labels, collision labels, event reduction, cancellation state, result counts, and failed-only retry IDs.

- [ ] **Step 2: Run and verify failure**

Run: `npm run test:frontend`

Expected: FAIL on missing export helpers/dialog.

- [ ] **Step 3: Implement output settings**

Default settings:

```ts
export const DEFAULT_WATERMARK_OUTPUT: WatermarkOutputSettings = {
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
};
```

Disable export until source, template, font/resource warnings, output directory, and settings validate. Show estimated bytes as a range based on source size, never as a false exact number.

Use the Tauri directory picker for output selection with folder creation enabled by the native picker. For a single source root, prefill its sibling `FramePair-Watermarked`; for multiple roots, leave the field empty until the user chooses or creates a directory.

- [ ] **Step 4: Implement the dialog state machine**

States are `confirm -> running -> results`. Confirmation displays count, skipped/warning count, format, size, quality/alpha, directory, two filename examples, collision policy, metadata policy, and estimated space. Running displays current filename and succeeded/skipped/failed counts. Results groups items and supports “重试失败项” plus “在文件管理器中显示”.

Cancellation text must say completed copies remain. Closing while running requires confirmation and sends cancel before dismissing.

- [ ] **Step 5: Verify Phase 5 and commit**

Run:

```bash
npm run test:frontend
npm run build
npm run test:core
```

Manual matrix:

1. JPEG sRGB privacy mode to empty directory.
2. PNG transparent output.
3. Sequence/skip/overwrite-output collisions.
4. Cancel a 30-photo task.
5. Make one target fail and retry only that item.
6. Confirm source JPG hashes remain unchanged.

```bash
git add src/features/watermark tests/watermark-utils.test.mjs tests/watermark-ui.test.mjs
git commit -m "feat: complete watermark export workflow"
```

## Phase 6: Guidance, Performance, Golden Tests, And Release Readiness

### Task 16: Guided Tour, Golden Rendering, Responsive QA, And Documentation

**Files:**
- Create: `src/features/watermark/WatermarkGuideDialog.tsx`
- Create: `src-tauri/tests/watermark_golden.rs`
- Create: `src-tauri/tests/fixtures/watermark/inputs/landscape.jpg`
- Create: `src-tauri/tests/fixtures/watermark/inputs/portrait-oriented.jpg`
- Create: `src-tauri/tests/fixtures/watermark/expected/solid-text.png`
- Create: `src-tauri/tests/fixtures/watermark/expected/gradient-exif.png`
- Create: `src-tauri/tests/fixtures/watermark/expected/blur-logo.png`
- Modify: `src/components/GuidedTourDialog.tsx`
- Modify: `src/features/watermark/WatermarkModule.tsx`
- Modify: `src/features/watermark/watermark.css`
- Modify: `tests/watermark-ui.test.mjs`
- Modify: `README.md`
- Modify: `package.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Write failing guide and documentation tests**

Assert five guide selectors exist, first-use storage key is versioned, guide can reopen, watermark workspace is a recognized scroll container, README lists the third module and safety guarantees, and all three package versions match.

- [ ] **Step 2: Run and verify failure**

Run: `npm run test:frontend`

Expected: FAIL because the guide and docs are incomplete.

- [ ] **Step 3: Implement the five-step masked guide**

Steps and selectors:

```ts
import { Download, ImagePlus, Layers3, LayoutTemplate, PanelsTopLeft } from "lucide-react";

const WATERMARK_GUIDE_STEPS: GuidedTourStep[] = [
  {
    title: "导入要发布的 JPG",
    description: "可以选择目录、拖入 JPG，或接收照片浏览中的当前筛选结果。",
    icon: ImagePlus,
    selector: "[data-watermark-tour='sources-templates']",
    placement: "right",
    points: [
      { label: "只处理 JPG", detail: "JPG+RAW 组合只读取 JPG，RAW-only 会明确跳过。" },
      { label: "任务列表固定", detail: "导入后形成快照，原筛选变化不会偷偷改变列表。" },
    ],
    tip: "导入和预览不会修改任何原照片。",
  },
  {
    title: "从模板开始",
    description: "选择内置模板，或打开保存在本机的自定义模板。",
    icon: LayoutTemplate,
    selector: "[data-watermark-tour='templates']",
    placement: "right",
    points: [
      { label: "内置模板", detail: "内置模板只读，调整后可以另存为我的模板。" },
      { label: "三种方向", detail: "一个模板同时保存横版、竖版和方形布局。" },
    ],
    tip: "模板只保存排版和素材，不包含待处理照片。",
  },
  {
    title: "在画布调整图层",
    description: "选择文字、EXIF 或 Logo 图层，拖动后再用右侧属性精确微调。",
    icon: Layers3,
    selector: "[data-watermark-tour='canvas']",
    placement: "top",
    points: [
      { label: "选择作用区域", detail: "图层可以锚定照片、指定边框或整个画布。" },
      { label: "精确控制", detail: "位置、尺寸、角度、透明度和样式都可以输入数值。" },
    ],
    tip: "撤销和重做会把一次拖动视为一个操作。",
  },
  {
    title: "检查三种照片方向",
    description: "通过胶片栏切换照片，检查横版、竖版和方形是否都排版正确。",
    icon: PanelsTopLeft,
    selector: "[data-watermark-tour='filmstrip']",
    placement: "top",
    points: [
      { label: "自动匹配版式", detail: "照片会按校正方向后的宽高自动选择布局。" },
      { label: "单张微调", detail: "只调整当前照片的缩放和位置，不破坏模板结构。" },
    ],
    tip: "选中照片会自动滚动到胶片栏可见范围。",
  },
  {
    title: "确认设置并导出副本",
    description: "核对格式、尺寸、元数据、目录和同名处理后再开始批量导出。",
    icon: Download,
    selector: "[data-watermark-tour='export']",
    placement: "left",
    points: [
      { label: "JPEG 或 PNG", detail: "JPEG 适合发布，PNG 支持无损与透明背景。" },
      { label: "默认隐私模式", detail: "保留拍摄信息，同时移除 GPS 和设备序列号。" },
    ],
    tip: "FramePair 只生成新副本，绝不覆盖源照片。",
  },
];
```

Open automatically once after a non-empty source loads; persist `framepair.watermark.guide.v1=true` on dismissal.

- [ ] **Step 4: Add reviewed golden image tests**

The golden test renders deterministic fixtures with the bundled font and compares decoded RGBA pixels, allowing maximum per-channel difference `2` and mismatch ratio below `0.001`.

```rust
fn assert_image_close(actual: &RgbaImage, expected: &RgbaImage) {
    assert_eq!(actual.dimensions(), expected.dimensions());
    let mut mismatches = 0usize;
    for (left, right) in actual.pixels().zip(expected.pixels()) {
        if left.0.iter().zip(right.0).any(|(a, b)| a.abs_diff(b) > 2) {
            mismatches += 1;
        }
    }
    let ratio = mismatches as f64 / (actual.width() as f64 * actual.height() as f64);
    assert!(ratio < 0.001, "golden mismatch ratio {ratio}");
}
```

Generate candidates only with `UPDATE_WATERMARK_GOLDENS=1 cargo test --manifest-path src-tauri/Cargo.toml --test watermark_golden`, inspect all three images visually, then commit the reviewed expected files. Normal CI never updates goldens.

- [ ] **Step 5: Run performance and memory checks**

Use a directory with at least 100 high-resolution JPGs:

- Thumbnail preload uses at most 3 workers.
- Edited-preview cache contains current plus four neighbors.
- Rapid arrow navigation does not show a blank canvas after adjacent cache is warm.
- Cancel reacts before another queued export starts.
- Resident memory returns near baseline after leaving the task and object URLs are revoked.

Record findings in the commit body; fix measured regressions before proceeding.

- [ ] **Step 6: Update documentation and version**

Add to README:

- Watermark module quick start.
- JPG-only input and RAW-only skip behavior.
- Structured layers and three orientation variants.
- JPEG/PNG output, metadata policies, and collision behavior.
- Template JSON portability.
- Local-only and never-modify-source guarantees.

Bump `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` from `0.7.1` to `0.8.0` in the same commit.

- [ ] **Step 7: Run the complete automated gate**

Run:

```bash
npm ci
npm run test:frontend
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
git diff --check
```

Expected: every command PASS with no modified generated files.

- [ ] **Step 8: Run final manual acceptance**

On macOS and Windows, verify:

1. Directory selection, mixed file/directory drop, and all preview handoff scopes.
2. Horizontal, vertical, and square variants with per-photo placement overrides.
3. Chinese text, missing system font fallback, Logo alpha, EXIF missing fields.
4. Six built-ins and JSON round trip after original resource files are unavailable.
5. JPEG and transparent PNG output in all metadata policies.
6. Collision sequence/skip/overwrite-output, cancellation, failure, and retry.
7. `860x620`, `1180x780`, wide desktop, and 200% zoom with both panels collapsed/expanded.
8. Source JPG, RAW, and XMP SHA-256 hashes are unchanged after every export.

- [ ] **Step 9: Commit release-ready polish**

```bash
git add src/features/watermark/WatermarkGuideDialog.tsx src/features/watermark/WatermarkModule.tsx src/features/watermark/watermark.css src/components/GuidedTourDialog.tsx tests/watermark-ui.test.mjs src-tauri/tests/watermark_golden.rs src-tauri/tests/fixtures/watermark README.md package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "feat: finish watermark export module"
```

## Final Acceptance Gate

Before declaring the module complete:

- `git status --short` is clean.
- The design spec's 12 acceptance criteria each have a passing automated or named manual check above.
- No command accepts arbitrary unvalidated output paths plus source paths in the same request.
- No code path writes source JPG, RAW, or XMP files.
- Preview and export both call `watermark_render::render_request`.
- All user-facing errors are Chinese and actionable; Rust error chains are not exposed directly.
- Built-in asset and dependency licenses are documented and compatible with distribution.
- The local module still runs without network access.

## Spec Coverage Map

| Design requirement | Implemented by |
|---|---|
| JPG-only sources, RAW-only skip, immutable handoff snapshot | Tasks 2-3 |
| Third navigation module and B three-pane layout | Tasks 3, 10 |
| Normalized photo/frame/canvas anchors | Tasks 1, 4, 11 |
| Decorative borders and photo effects | Tasks 5, 11-12 |
| Text, EXIF, and image layers | Tasks 6-7, 11 |
| Horizontal/vertical/square variants | Tasks 1, 9-12 |
| Single-photo placement override | Tasks 9-11 |
| Shared preview/export renderer and preload | Tasks 5-8, 16 |
| Built-in/local/portable JSON templates | Task 12 |
| JPEG/PNG, color, sizing, metadata, naming | Tasks 5-6, 13, 15 |
| Atomic output, collision safety, source protection | Tasks 2, 13-14 |
| Progress, cancellation, result review, failed retry | Tasks 14-15 |
| Masked guide, responsive/immersive editor | Tasks 10, 16 |
| Rendering, integration, performance, cross-platform tests | Tasks 4-8, 12-16 |
