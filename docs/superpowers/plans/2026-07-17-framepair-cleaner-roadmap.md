# FramePair Cleaner Roadmap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不进入图片预览、AI 筛片和图库管理赛道的前提下，为 FramePair 增加主流 RAW 格式、安全隔离与恢复、反向配对审计，以及文件清单/XMP 星级参考源。

**Architecture:** Rust 继续作为格式白名单、路径边界、匹配和文件移动的唯一可信层，React 只负责收集选项与展示后端生成的稳定计划。新增能力按“格式策略、隔离操作、扫描方向、参考源”拆成独立模块，每个阶段均可单独发布，并沿用计划 ID、文件快照和执行前复验机制。

**Tech Stack:** Tauri 2、Rust 2024、React 19、TypeScript 7、Vite 8、`walkdir`、`trash`、`serde`、`quick-xml`。

---

## 产品决策与边界

1. 主流 RAW 格式第一批支持：`.nef/.nrw/.cr2/.cr3/.arw/.sr2/.srf/.raf/.dng/.rw2/.orf/.pef`。
2. JPG 参考格式仍限定为 `.jpg/.jpeg`；伴随文件第一批仍只处理 `.xmp`。
3. 格式白名单由 Rust 固定定义，前端不能通过 IPC 放宽可处理扩展名。
4. 隔离区固定为 RAW 根目录下的 `.framepair-quarantine/<operation-id>/`，保持同卷原子移动并保留相对子目录。
5. 隔离操作必须生成恢复清单；恢复时拒绝覆盖已经存在或已经变化的文件。
6. 反向检查第一版只报告“没有 RAW 的 JPG”，允许筛选、搜索、定位和导出清单，不提供删除 JPG 的入口。
7. Lightroom 集成不直接读取 `.lrcat`；只支持 UTF-8 相对路径清单和 Lightroom/Bridge 写出的 XMP 星级。
8. XMP 星级来源允许与 RAW 根目录相同；目录型 JPG 参考源仍禁止与 RAW 根目录相同或互相嵌套。
9. 反向审计只允许目录型 JPG 参考源，文件清单和 XMP 星级来源只用于 RAW 清理模式。
10. 不引入 RAW 解码、缩略图、相似度、AI 评分和图库数据库。

## 版本顺序

| 里程碑 | 范围 | 建议版本 | 预估 |
|---|---|---:|---:|
| M1 | 主流 RAW 格式与统一白名单 | `0.2.0` | 1-2 天 |
| M2 | 隔离区、恢复与操作历史 | `0.3.0` | 2-3 天 |
| M3 | 无 RAW 的 JPG 反向审计 | `0.4.0` | 1-2 天 |
| M4 | 文件清单与 XMP 星级参考源 | `0.5.0` | 3-4 天 |
| M5 | 跨平台回归、文档与安装包 | `0.5.0` | 1-2 天 |

## 文件职责

- Create: `src-tauri/src/formats.rs`：可信扩展名白名单、格式识别、XMP 配对键。
- Create: `src-tauri/src/quarantine.rs`：隔离路径、移动清单、恢复校验。
- Create: `src-tauri/src/reference.rs`：目录、文件清单、XMP 星级三类参考源解析。
- Modify: `src-tauri/src/lib.rs`：扫描编排、Tauri 命令和操作计划，不再保存扩展名规则细节。
- Modify: `src-tauri/src/safety.rs`：把 `DeletionPlan` 泛化为可供回收站与隔离区共用的 `CleanupPlan`。
- Modify: `src-tauri/tests/safety_logic.rs`：计划授权、隔离恢复和路径攻击回归测试。
- Modify: `src/types.ts`：扫描模式、参考源、执行目标与历史记录类型。
- Modify: `src/App.tsx`：新选项状态、IPC 请求、恢复流程和状态持久化。
- Modify: `src/components/SetupView.tsx`：扫描方向、参考源和支持格式说明。
- Modify: `src/components/ResultsWorkspace.tsx`：通用“已配对/未配对”结果与反向审计动作。
- Modify: `src/components/ConfirmDialog.tsx`：回收站/隔离区分段选择和对应确认文本。
- Modify: `src/utils.ts`：通用结果过滤、可执行项判断与扩展名统计。
- Modify: `tests/frontend-utils.test.mjs`：新状态模型、动作权限和统计测试。
- Modify: `README.md`、`docs/TECHNICAL-SOLUTION.md`：能力边界、安全模型和 Lightroom 工作流。

### Task 1: 建立 Rust 格式白名单

**Files:**
- Create: `src-tauri/src/formats.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/formats.rs`

- [ ] **Step 1: 写格式识别与白名单失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn recognizes_supported_raw_families() {
        for name in [
            "a.NEF", "a.NRW", "a.CR2", "a.CR3", "a.ARW", "a.SR2",
            "a.SRF", "a.RAF", "a.DNG", "a.RW2", "a.ORF", "a.PEF",
        ] {
            assert!(is_raw(Path::new(name)), "{name} should be RAW");
        }
        assert!(!is_raw(Path::new("a.tiff")));
        assert!(!is_raw(Path::new("a.exe")));
    }

    #[test]
    fn xmp_keys_support_both_common_naming_forms() {
        assert_eq!(
            sidecar_match_keys(Path::new("day/a.xmp"), false),
            vec!["day/a"]
        );
        assert_eq!(
            sidecar_match_keys(Path::new("day/a.NEF.xmp"), false),
            vec!["day/a.nef", "day/a"]
        );
    }
}
```

- [ ] **Step 2: 运行测试并确认失败**

Run: `npm run test:core`

Expected: FAIL，提示 `formats` 模块或 `is_raw`、`sidecar_match_keys` 尚不存在。

- [ ] **Step 3: 实现固定白名单**

```rust
use std::path::Path;

pub(crate) const REFERENCE_EXTENSIONS: &[&str] = &["jpg", "jpeg"];
pub(crate) const RAW_EXTENSIONS: &[&str] = &[
    "nef", "nrw", "cr2", "cr3", "arw", "sr2", "srf", "raf", "dng", "rw2",
    "orf", "pef",
];
pub(crate) const SIDECAR_EXTENSIONS: &[&str] = &["xmp"];

pub(crate) fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn is_reference(path: &Path) -> bool {
    REFERENCE_EXTENSIONS.contains(&extension_of(path).as_str())
}

pub(crate) fn is_raw(path: &Path) -> bool {
    RAW_EXTENSIONS.contains(&extension_of(path).as_str())
}

pub(crate) fn is_sidecar(path: &Path) -> bool {
    SIDECAR_EXTENSIONS.contains(&extension_of(path).as_str())
}

fn normalized_key(path: &Path, case_sensitive: bool) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if case_sensitive { value } else { value.to_lowercase() }
}

pub(crate) fn sidecar_match_keys(path: &Path, case_sensitive: bool) -> Vec<String> {
    let without_xmp = path.with_extension("");
    let mut keys = vec![normalized_key(&without_xmp, case_sensitive)];
    if RAW_EXTENSIONS.contains(&extension_of(&without_xmp).as_str()) {
        keys.push(normalized_key(&without_xmp.with_extension(""), case_sensitive));
    }
    keys
}
```

- [ ] **Step 4: 让扫描与执行校验共用白名单**

在 `src-tauri/src/lib.rs` 声明 `mod formats;`，删除 `ScanRequest` 中三个扩展名数组，扫描时改用 `is_reference/is_raw/is_sidecar`。`validate_delete_candidate` 必须使用：

```rust
if !formats::is_raw(&relative) && !formats::is_sidecar(&relative) {
    return Err(format!("不允许处理 .{} 文件", formats::extension_of(&relative)));
}
```

- [ ] **Step 5: 运行 Rust 测试并提交**

Run: `npm run test:core`

Expected: 所有格式、扫描和安全测试通过。

```bash
git add src-tauri/src/formats.rs src-tauri/src/lib.rs
git commit -m "support common raw formats"
```

### Task 2: 更新多格式前端与扫描统计

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/types.ts`
- Modify: `src/components/SetupView.tsx`
- Modify: `src/components/ResultsWorkspace.tsx`
- Modify: `src/utils.ts`
- Test: `tests/frontend-utils.test.mjs`

- [ ] **Step 1: 写扩展名统计测试**

```javascript
test("raw format counts are grouped case-insensitively", () => {
  const items = [
    { kind: "raw", extension: ".NEF" },
    { kind: "raw", extension: ".nef" },
    { kind: "raw", extension: ".CR3" },
    { kind: "sidecar", extension: ".xmp" },
  ];
  assert.deepEqual(utils.rawFormatCounts(items), { NEF: 2, CR3: 1 });
});
```

- [ ] **Step 2: 运行前端测试并确认失败**

Run: `npm run test:frontend`

Expected: FAIL，提示 `rawFormatCounts` 不存在。

- [ ] **Step 3: 实现统计函数并删除前端扩展名授权**

```ts
export function rawFormatCounts(items: Pick<ScanItem, "kind" | "extension">[]) {
  return items.reduce<Record<string, number>>((counts, item) => {
    if (item.kind !== "raw") return counts;
    const key = item.extension.replace(/^\./, "").toUpperCase();
    counts[key] = (counts[key] ?? 0) + 1;
    return counts;
  }, {});
}
```

`runScan()` 只发送根目录、扫描方向、参考源和大小写选项，不再发送扩展名数组。

- [ ] **Step 4: 调整界面文案**

目录选择页显示“支持 Nikon、Canon、Sony、Fujifilm、DNG 等主流 RAW”；结果摘要使用扫描结果动态显示 `NEF 120 · CR3 38`，不增加一组默认全选的格式复选框。

- [ ] **Step 5: 构建并提交**

Run: `npm run test:frontend`

Expected: PASS。

Run: `npm run build`

Expected: TypeScript 和 Vite 构建成功。

```bash
git add src/App.tsx src/types.ts src/components/SetupView.tsx src/components/ResultsWorkspace.tsx src/utils.ts tests/frontend-utils.test.mjs
git commit -m "show multi-format scan results"
```

### Task 3: 增加隔离区数据模型与文件移动

**Files:**
- Create: `src-tauri/src/quarantine.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/safety.rs`
- Test: `src-tauri/tests/safety_logic.rs`

- [ ] **Step 1: 写隔离与恢复的失败测试**

```rust
#[path = "../src/quarantine.rs"]
mod quarantine;

use std::path::Path;

#[test]
fn quarantine_preserves_relative_paths_and_restores_without_overwrite() {
    let temp = tempfile::tempdir().expect("temp root");
    let raw_root = temp.path().join("raw");
    let source = raw_root.join("day/a.NEF");
    std::fs::create_dir_all(source.parent().expect("source parent")).expect("source dir");
    std::fs::write(&source, b"raw").expect("source file");

    let record = quarantine::move_file(&raw_root, "operation-1", Path::new("day/a.NEF"))
        .expect("quarantine move");
    assert!(!source.exists());
    assert!(record.quarantined_path.ends_with("operation-1/day/a.NEF"));

    quarantine::restore_file(&raw_root, &record).expect("restore");
    assert!(source.exists());
    assert!(!record.quarantined_path.exists());
}

#[test]
fn quarantine_rejects_symlink_root() {
    let temp = tempfile::tempdir().expect("temp root");
    let raw_root = temp.path().join("raw");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&raw_root).expect("raw root");
    std::fs::create_dir_all(&outside).expect("outside root");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, raw_root.join(".framepair-quarantine"))
        .expect("quarantine symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&outside, raw_root.join(".framepair-quarantine"))
        .expect("quarantine symlink");

    assert!(quarantine::operation_root(&raw_root, "operation-1").is_err());
}

#[test]
fn restore_rejects_an_existing_destination() {
    let temp = tempfile::tempdir().expect("temp root");
    let raw_root = temp.path().join("raw");
    let source = raw_root.join("a.NEF");
    std::fs::create_dir_all(&raw_root).expect("raw root");
    std::fs::write(&source, b"first").expect("source file");
    let record = quarantine::move_file(&raw_root, "operation-1", Path::new("a.NEF"))
        .expect("quarantine move");
    std::fs::write(&source, b"replacement").expect("replacement file");

    assert!(quarantine::restore_file(&raw_root, &record).is_err());
    assert_eq!(std::fs::read(&source).expect("destination"), b"replacement");
    assert!(record.quarantined_path.exists());
}

#[test]
fn quarantine_rejects_parent_traversal() {
    let temp = tempfile::tempdir().expect("temp root");
    let raw_root = temp.path().join("raw");
    std::fs::create_dir_all(&raw_root).expect("raw root");

    assert!(
        quarantine::move_file(&raw_root, "operation-1", Path::new("../outside.NEF"))
            .is_err()
    );
}
```

- [ ] **Step 2: 运行测试并确认失败**

Run: `npm run test:core`

Expected: FAIL，提示 `quarantine` 模块不存在。

- [ ] **Step 3: 实现隔离记录与同卷移动**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuarantineRecord {
    pub operation_id: String,
    pub relative_path: String,
    pub quarantined_path: PathBuf,
    pub size_bytes: u64,
    pub modified_ms: Option<u64>,
}

pub(crate) fn operation_root(raw_root: &Path, operation_id: &str) -> Result<PathBuf, String> {
    if operation_id.is_empty()
        || !operation_id.chars().all(|value| value.is_ascii_alphanumeric() || value == '-')
    {
        return Err("隔离操作编号不合法".to_string());
    }
    let root = raw_root.join(".framepair-quarantine");
    if root.exists() && fs::symlink_metadata(&root).map_err(|e| e.to_string())?.file_type().is_symlink() {
        return Err("隔离目录不能是符号链接".to_string());
    }
    Ok(root.join(operation_id))
}
```

`move_file()` 使用经过 `safe_relative_path` 校验的相对路径，先创建目标父目录，再调用 `fs::rename`。目标已存在时整项失败，绝不覆盖。每次成功后向操作目录的 `manifest.jsonl` 追加并 `sync_data()`。

- [ ] **Step 4: 排除隔离目录的再次扫描**

`collect_files()` 改为：

```rust
for entry in WalkDir::new(root)
    .follow_links(false)
    .into_iter()
    .filter_entry(|entry| {
        entry.depth() != 1 || entry.file_name() != ".framepair-quarantine"
    })
{
    let entry = entry.map_err(|error| format!("扫描目录失败：{error}"))?;
    if entry.file_type().is_file() {
        files.push(entry.into_path());
    }
}
```

- [ ] **Step 5: 泛化操作计划**

把 `DeletionPlan` 重命名为 `CleanupPlan`，保持计划 ID、规范化根目录和文件快照授权不变。新增：

```rust
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupDestination {
    Trash,
    Quarantine,
}
```

将 `move_to_trash` 重命名为 `execute_cleanup`；每个候选项通过相同授权后，根据 `destination` 调用 `trash::delete` 或 `quarantine::move_file`。

- [ ] **Step 6: 运行测试并提交**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Expected: PASS。

Run: `npm run test:core`

Expected: 全部通过。

```bash
git add src-tauri/src/quarantine.rs src-tauri/src/lib.rs src-tauri/src/safety.rs src-tauri/tests/safety_logic.rs
git commit -m "add recoverable quarantine cleanup"
```

### Task 4: 增加隔离选择、历史和恢复界面

**Files:**
- Modify: `src/types.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/ConfirmDialog.tsx`
- Modify: `src/components/ResultsWorkspace.tsx`
- Modify: `src/styles.css`
- Test: `tests/frontend-utils.test.mjs`

- [ ] **Step 1: 写操作模式文案和权限测试**

```javascript
test("cleanup destination copy names the selected operation", () => {
  assert.equal(utils.cleanupActionLabel("trash"), "移入系统回收站");
  assert.equal(utils.cleanupActionLabel("quarantine"), "移入 FramePair 隔离区");
});
```

- [ ] **Step 2: 定义前端类型**

```ts
export type CleanupDestination = "trash" | "quarantine";

export interface QuarantineOperation {
  operationId: string;
  createdAt: string;
  rawRoot: string;
  moved: number;
  restored: number;
  manifestPath: string;
}
```

- [ ] **Step 3: 在确认对话框增加分段控制**

默认保持现有 `trash` 行为。两个选项为“系统回收站”和“FramePair 隔离区”，切换时同步更新确认句子；隔离模式说明“保留原目录结构，可从操作结果恢复”。

- [ ] **Step 4: 接入执行与恢复命令**

`execute_cleanup` 请求包含 `destination`。操作成功后，结果区显示“打开隔离目录”和“恢复本次文件”；恢复命令只接受后端返回的 `operationId`，前端不能提交任意源/目标路径。

后端同时提供 `list_quarantine_operations(raw_root)`：只遍历 `.framepair-quarantine` 的一级操作目录并读取 `manifest.jsonl`，返回可恢复数量；不接受前端传入 manifest 路径。应用重启后，结果区的“隔离历史”仍能发现未恢复操作。

- [ ] **Step 5: 验证并提交**

Run: `npm run test:frontend`

Expected: PASS。

Run: `npm run build`

Expected: PASS。

```bash
git add src/types.ts src/App.tsx src/components/ConfirmDialog.tsx src/components/ResultsWorkspace.tsx src/styles.css tests/frontend-utils.test.mjs
git commit -m "add quarantine and restore controls"
```

### Task 5: 将结果模型泛化为配对状态

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types.ts`
- Modify: `src/utils.ts`
- Modify: `src/components/ResultsWorkspace.tsx`
- Test: `src-tauri/src/lib.rs`
- Test: `tests/frontend-utils.test.mjs`

- [ ] **Step 1: 写通用状态失败测试**

```javascript
test("only unmatched raw items are actionable in cleanup mode", () => {
  assert.equal(utils.isActionableItem({ kind: "raw", matchStatus: "unmatched" }, "cleanupRaw"), true);
  assert.equal(utils.isActionableItem({ kind: "raw", matchStatus: "matched" }, "cleanupRaw"), false);
  assert.equal(utils.isActionableItem({ kind: "reference", matchStatus: "unmatched" }, "auditReference"), false);
});
```

- [ ] **Step 2: 替换误导性的 Keep/Delete 命名**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchStatus {
    Matched,
    Unmatched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanMode {
    CleanupRaw,
    AuditReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Raw,
    Reference,
    Sidecar,
}
```

`ScanSummary` 将 `matched_raws/missing_raws` 泛化为 `matched/unmatched`，并回传 `mode`；`ScanItem` 将字段 `status` 改为 `match_status`。前端对应类型为 `"matched" | "unmatched"`、`"cleanupRaw" | "auditReference"` 和 `"raw" | "reference" | "sidecar"`。过滤器文案改为“未配对 / 已配对 / 全部”。

- [ ] **Step 3: 只让清理模式生成后端操作候选**

`CleanupPlan` 只收录 `ScanMode::CleanupRaw` 下的未配对 RAW 和对应 XMP。`ScanMode::AuditReference` 返回空操作候选集合，确保前端错误调用执行命令时仍被后端拒绝。

- [ ] **Step 4: 更新全部测试与提交**

Run: `npm run test:frontend`

Expected: PASS。

Run: `npm run test:core`

Expected: PASS。

```bash
git add src-tauri/src/lib.rs src/types.ts src/utils.ts src/components/ResultsWorkspace.tsx tests/frontend-utils.test.mjs
git commit -m "generalize scan results to match status"
```

### Task 6: 增加“没有 RAW 的 JPG”反向审计

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/App.tsx`
- Modify: `src/components/SetupView.tsx`
- Modify: `src/components/ResultsWorkspace.tsx`
- Modify: `src/styles.css`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: 写嵌套路径反向匹配测试**

```rust
#[test]
fn audits_references_without_matching_raws() {
    let temp = tempfile::tempdir().expect("temp directory");
    let reference = temp.path().join("jpg");
    let raw = temp.path().join("raw");
    fs::create_dir_all(reference.join("day")).expect("jpg day");
    fs::create_dir_all(raw.join("day")).expect("raw day");
    fs::write(reference.join("day/kept.JPG"), b"jpg").expect("kept jpg");
    fs::write(reference.join("day/orphan.JPG"), b"jpg").expect("orphan jpg");
    fs::write(raw.join("day/kept.CR3"), b"raw").expect("kept raw");

    let summary = scan_pairs_impl(&request_with_mode(&reference, &raw, ScanMode::AuditReference))
        .expect("reverse audit");
    assert_eq!(summary.unmatched, 1);
    assert!(summary.items.iter().any(|item| item.relative_path == "day/orphan.JPG"));
}
```

- [ ] **Step 2: 实现双向索引比较**

扫描阶段始终建立参考键与 RAW 键两个集合。`CleanupRaw` 遍历 RAW 并查询参考键；`AuditReference` 遍历参考文件并查询 RAW 键。两种模式共用 `match_key(relative_path)`，不能退化为只按 basename 匹配。

- [ ] **Step 3: 增加扫描方向控制**

在目录选择页使用两个选项的分段控制：“清理无 JPG 的 RAW”和“检查无 RAW 的 JPG”。反向模式的主按钮改为“开始检查”，结果页隐藏选择框、释放空间和执行按钮，提供“在 Finder/资源管理器显示”与“导出未配对清单”。

当参考源不是 `Directory` 时禁用“检查无 RAW 的 JPG”，后端也必须拒绝 `AuditReference + Manifest` 和 `AuditReference + XmpRating` 的组合。

- [ ] **Step 4: 增加清单导出命令**

后端命令 `export_audit_manifest(plan_id, destination)` 只导出当前审计计划中的相对路径，每行一个 UTF-8 路径。目标文件由系统保存对话框返回；后端拒绝导出清理计划或过期计划。

- [ ] **Step 5: 验证并提交**

Run: `npm run test:frontend`

Expected: PASS。

Run: `npm run test:core`

Expected: PASS。

```bash
git add src-tauri/src/lib.rs src/App.tsx src/components/SetupView.tsx src/components/ResultsWorkspace.tsx src/styles.css
git commit -m "add reverse jpg pairing audit"
```

### Task 7: 增加 UTF-8 文件清单参考源

**Files:**
- Create: `src-tauri/src/reference.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/SetupView.tsx`
- Test: `src-tauri/src/reference.rs`

- [ ] **Step 1: 写严格清单解析测试**

```rust
#[test]
fn parses_relative_utf8_manifest_and_rejects_ambiguous_entries() {
    let input = "day/a.JPG\nother/b.jpeg\n";
    let keys = parse_manifest(input, false).expect("valid manifest");
    assert_eq!(keys, HashSet::from(["day/a".to_string(), "other/b".to_string()]));

    assert!(parse_manifest("../a.jpg\n", false).is_err());
    assert!(parse_manifest("/absolute/a.jpg\n", false).is_err());
    assert!(parse_manifest("a.jpg\na.jpeg\n", false).is_err());
}
```

- [ ] **Step 2: 定义参考源协议**

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ReferenceSource {
    Directory { root: String },
    Manifest { path: String },
    XmpRating { root: String, minimum_rating: u8 },
}
```

`Manifest` 第一版只接受 UTF-8 `.txt`，每个非空行必须是带 `.jpg/.jpeg` 后缀的规范相对路径。忽略行首尾空白和 `#` 开头注释；拒绝绝对路径、`.`、`..` 和重复匹配键。顶层文件名合法，但必须与 RAW 根目录顶层文件匹配。

- [ ] **Step 3: 让扫描消费统一的参考键集合**

```rust
pub(crate) struct ReferenceIndex {
    pub keys: HashMap<String, Vec<String>>,
    pub source_items: usize,
    pub duplicate_keys: usize,
}
```

`reference::build_index()` 根据 `ReferenceSource` 返回相同结构，`scan_pairs_impl()` 不再关心参考键来自目录、清单还是星级。

- [ ] **Step 4: 增加前端来源选择**

目录页新增“保留依据”菜单：`JPG 目录`、`文件清单`、`XMP 星级`。选择文件清单时使用系统文件选择器限制 `.txt`，并显示格式要求；切换来源必须清空旧扫描计划。

- [ ] **Step 5: 验证并提交**

Run: `npm run test:core`

Expected: PASS。

Run: `npm run build`

Expected: PASS。

```bash
git add src-tauri/src/reference.rs src-tauri/src/lib.rs src/types.ts src/App.tsx src/components/SetupView.tsx
git commit -m "support manifest reference sources"
```

### Task 8: 增加 XMP 星级参考源

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/reference.rs`
- Modify: `src/App.tsx`
- Modify: `src/components/SetupView.tsx`
- Test: `src-tauri/src/reference.rs`

- [ ] **Step 1: 添加解析依赖并写属性/元素两种 XMP 测试**

在 `src-tauri/Cargo.toml` 添加：

```toml
quick-xml = "0.38"
```

```rust
#[test]
fn reads_xmp_rating_from_attribute_or_element() {
    let attribute = br#"<rdf:Description xmp:Rating=\"5\" />"#;
    let element = br#"<xmp:Rating>4</xmp:Rating>"#;
    assert_eq!(xmp_rating(attribute).expect("attribute rating"), Some(5));
    assert_eq!(xmp_rating(element).expect("element rating"), Some(4));
    assert_eq!(xmp_rating(b"<x:xmpmeta />").expect("missing rating"), None);
}
```

- [ ] **Step 2: 实现有界 XMP 解析**

`xmp_rating()` 使用 `quick_xml::Reader`，只识别 local name 为 `Rating` 的属性或元素，评分必须是 `-1..=5`。单个 XMP 超过 4 MiB 时拒绝解析；XML 损坏时扫描整体失败，不能静默生成不完整清理计划。

- [ ] **Step 3: 建立达到阈值的参考键**

`XmpRating { minimum_rating }` 只允许 `1..=5`。遍历参考根目录中的 `.xmp`，评分达到阈值时，通过 `sidecar_match_keys()` 得到相对路径键；同一个 XMP 的双键如果同时匹配多个不同图像，记为重复键并阻止执行。

根目录边界校验按参考源区分：`Directory` 继续禁止与 RAW 根目录重叠；`XmpRating` 允许根目录等于 RAW 根目录，并复用 `collect_files()` 对 `.framepair-quarantine` 的排除逻辑；`Manifest` 校验为规范化普通文件且扩展名必须是 `.txt`。

- [ ] **Step 4: 增加星级阈值控件和 Lightroom 指引文案**

使用 1-5 数值步进器选择最低星级。界面只说明“读取已写入磁盘的 XMP Rating”，不声称直接读取 Lightroom 目录；没有 XMP 的文件自然不进入保留集合。

- [ ] **Step 5: 验证并提交**

Run: `npm run test:core`

Expected: PASS。

Run: `npm run build`

Expected: PASS。

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/reference.rs src/App.tsx src/components/SetupView.tsx
git commit -m "support xmp rating references"
```

### Task 9: 文档、跨平台回归与发布验收

**Files:**
- Modify: `README.md`
- Modify: `docs/TECHNICAL-SOLUTION.md`
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: 更新用户工作流文档**

README 明确列出支持格式、三种参考源、隔离恢复流程和反向审计的只读边界。Lightroom 示例必须写为：“先将评级写入 XMP，再让 FramePair 读取 XMP”，不得描述为读取 `.lrcat`。

- [ ] **Step 2: 更新安全模型**

技术文档补充：Rust 固定白名单、隔离目录排除规则、隔离清单、恢复冲突拒绝、反向审计不产生可执行候选、XMP 解析大小上限。

- [ ] **Step 3: 执行完整本地验证**

Run: `npm run test:frontend`

Expected: 全部通过。

Run: `npm run build`

Expected: TypeScript 和 Vite 构建成功。

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Expected: 无格式差异。

Run: `npm run test:core`

Expected: Rust 单元与集成测试全部通过。

Run: `npm run tauri -- build --debug --bundles app`

Expected: 当前平台生成可启动桌面应用。

- [ ] **Step 4: 执行人工文件系统矩阵**

使用临时测试目录逐项检查：12 种 RAW 后缀、大小写差异、同名不同子目录、`file.xmp`、`file.RAW.xmp`、只读文件、符号链接、隔离后恢复、恢复目标冲突、损坏 XMP、反向审计、网络卷回收站失败。任何失败都必须保留原文件并显示可定位的错误。

- [ ] **Step 5: 提交文档并创建版本标签**

```bash
git add README.md docs/TECHNICAL-SOLUTION.md .github/workflows/ci.yml
git commit -m "document expanded cleanup workflows"
git tag v0.5.0
git push origin main
git push origin v0.5.0
```

## 每阶段验收门槛

- M1：所有受支持 RAW 均可扫描和安全处理，任何未列入白名单的扩展名无法通过 IPC 获得处理权限。
- M2：隔离和恢复保持相对路径；进程重启后仍可根据清单恢复；任何冲突都不覆盖文件。
- M3：反向模式能准确列出无 RAW 的 JPG，且后端不存在删除这些 JPG 的授权计划。
- M4：目录、清单、XMP 星级产生相同结构的参考索引；重复键一律阻断执行。
- M5：Windows 与 macOS CI 通过，安装包启动、拖放、扫描、隔离、恢复、回收站和日志均完成手工抽查。

## 暂不纳入本轮

- RAW/JPG 图片预览、缩略图缓存和放大比较。
- AI 选片、模糊检测、人脸/闭眼检测和相似图聚类。
- Lightroom `.lrcat` 数据库解析或插件开发。
- 永久删除入口、自动定时清理和无人值守执行。
- 视频、HEIC、TIFF 和非 XMP 伴随文件。
