# FramePair 影像配对

FramePair 是一个 Windows 与 macOS 桌面工具。它以人工筛选结果为保留依据，按“相对路径 + 文件名”检查 RAW 目录，并安全处理未配对的 RAW 与可选 XMP 伴随文件。所有扫描、匹配和文件操作均在本机完成。

## 当前功能

- 支持点击或拖拽选择 JPG 参考目录、XMP 评分目录和 RAW 源目录。
- 保留依据可以是 JPG 目录、UTF-8 相对路径清单，或 Lightroom/Bridge 已写入磁盘的 XMP Rating。
- 支持 `.nef/.nrw/.cr2/.cr3/.arw/.sr2/.srf/.raf/.dng/.rw2/.orf/.pef`，格式白名单由 Rust 后端固定控制。
- 递归扫描子目录，并按照相对路径与文件名主体配对，避免不同拍摄目录中的同名文件互相匹配。
- 支持反向只读审计，找出没有对应 RAW 的 JPG，并导出 `.txt` 清单。
- 在执行前展示已配对、未配对、XMP 数量、格式分布和预计释放空间。
- 支持搜索、状态过滤、逐项选择和 XMP 处理开关。
- 默认不区分文件名大小写，可切换为严格匹配。
- 清理前重新校验路径、扩展名、文件大小和修改时间。
- 清理请求必须匹配 Rust 后端保存的当前扫描计划，重新扫描后旧计划立即失效。
- 可以移入系统回收站/废纸篓，也可以移入保留目录结构的 FramePair 隔离区。
- 隔离操作写入恢复清单；应用重启后仍可发现和恢复，且绝不覆盖原位置的同名文件。
- 不提供永久删除入口。
- 每次操作写入应用日志目录下的 `operations.jsonl`。

## 参考源

### JPG 目录

适合“先浏览并删除不满意的 JPG，再清理没有同名 JPG 的 RAW”这一工作流。JPG 目录和 RAW 目录不能相同或互相嵌套。

### 文件清单

清单必须是 UTF-8 `.txt`，每行一个带 `.jpg/.jpeg` 后缀的相对路径；空行与 `#` 开头的注释会被忽略。绝对路径、`..`、非 JPG 条目和重复匹配键会阻止扫描。

### XMP 星级

FramePair 读取 XMP 中的标准 `Rating`，最低保留星级可设为 1-5。使用 Lightroom Classic 或 Bridge 时，需要先将元数据写入磁盘上的 XMP；FramePair 不读取或修改 `.lrcat`。XMP 评分目录可以与 RAW 根目录相同。

## 开发环境

所有平台都需要 Node.js、npm 与 Rust stable。

Windows 还需要 Microsoft C++ Build Tools 和 WebView2。macOS 需要 Xcode Command Line Tools：

```bash
xcode-select --install
```

安装项目依赖并启动：

```bash
npm ci
npm run tauri -- dev
```

运行前端测试、生产构建和 Rust 核心测试：

```bash
npm run test:frontend
npm run build
npm run test:core
```

## 本地打包

Windows x64：

```powershell
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis,msi
```

macOS Apple Silicon：

```bash
rustup target add aarch64-apple-darwin
npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg
```

macOS Intel：

```bash
rustup target add x86_64-apple-darwin
npm run tauri -- build --target x86_64-apple-darwin --bundles app,dmg
```

macOS 安装包必须在 macOS 上构建。Windows 不能可靠地交叉生成 `.app` 或 `.dmg`，因此仓库提供了 `.github/workflows/release.yml`：推送 `v*` 标签或手动运行工作流后，它会分别在 Windows、Apple Silicon macOS 和 Intel macOS runner 上构建，并创建一个包含安装包的草稿 Release。

## 签名与分发

未签名安装包适合内部测试，但 Windows SmartScreen 和 macOS Gatekeeper 可能显示警告。面向其他用户正式分发时应配置：

- Windows：受信任的代码签名证书。
- macOS：Apple Developer ID Application 证书、公证和 stapling。
- GitHub Actions：按工作流中的变量名配置 `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_SIGNING_IDENTITY`、`APPLE_ID`、`APPLE_PASSWORD` 和 `APPLE_TEAM_ID` secrets。

Apple Silicon 与 Intel 可以继续分别发布，也可以在 macOS 构建机上使用 `lipo` 合并应用二进制后再生成 Universal DMG。分别发布更简单，也能保持安装包较小。

## 安全规则

1. 目录型 JPG 参考源与 RAW 根目录不能相同或互相嵌套；XMP 评分源允许与 RAW 根目录相同。
2. 扫描遇到目录权限或遍历错误时整体失败，不输出不完整删除计划。
3. 符号链接不会被递归跟随。
4. 前端只能提交 RAW 根目录下的相对路径。
5. Rust 后端拒绝绝对路径、`..`、非白名单 RAW/XMP 文件和越界后的规范路径。
6. 文件在扫描后发生变化时跳过，要求重新扫描。
7. 网络卷或特殊文件系统不支持系统废纸篓时，该文件会失败并保留在原处，不会退化为永久删除。
8. 隔离目录与操作目录不能是符号链接；移动记录写入失败时会尝试立即回滚。
9. 恢复前重新校验隔离路径、文件大小和修改时间，原位置存在文件时拒绝覆盖。
10. 反向 JPG 审计不会生成清理授权，无法通过执行接口删除 JPG。
11. XMP 文件超过 4 MiB、XML 损坏或评分非法时整次扫描失败。

## 项目结构

```text
src/                         React/TypeScript 工作台
src-tauri/src/lib.rs         扫描编排、Tauri 命令与操作计划
src-tauri/src/formats.rs     可信格式白名单和 XMP 配对键
src-tauri/src/reference.rs   目录、清单与 XMP 星级参考源
src-tauri/src/quarantine.rs  隔离、历史清单与冲突安全恢复
src-tauri/src/safety.rs      扫描计划和文件快照授权
src-tauri/capabilities/      Tauri 最小权限
src-tauri/icons/             Windows/macOS 安装图标
.github/workflows/release.yml 跨平台构建矩阵
```

当前模块聚焦文件配对与安全清理，不包含照片预览、评分写入、RAW 解码、AI 筛片或 Lightroom 目录数据库解析。

## 许可

FramePair 采用 [PolyForm Noncommercial License 1.0.0](LICENSE.md)。个人学习、研究、实验和其他非商业用途可以在协议范围内使用、修改与分发；商业使用需要另行取得作者的书面授权。

本项目属于源码可用软件，不是 OSI 定义下的开源软件。项目所使用的第三方依赖继续适用各自的许可证，本协议不会改变这些第三方许可证。
