# FramePair 影像配对

FramePair 是一个 Windows 与 macOS 桌面工具。它以人工筛选后保留的 JPG 为参考，按“相对路径 + 文件名”检查 RAW 目录，并把未配对的 NEF 与可选 XMP 伴随文件移入系统回收站或废纸篓。

## 当前功能

- 使用系统目录选择器选择 JPG 参考目录和 RAW 源目录。
- 递归扫描所有子目录，但只索引 `.jpg/.jpeg` 与 `.nef`。
- 在执行前展示已配对、待清理、XMP 数量和预计释放空间。
- 支持搜索、状态过滤、逐项选择和 XMP 处理开关。
- 默认不区分文件名大小写，可切换为严格匹配。
- 清理前重新校验路径、扩展名、文件大小和修改时间。
- 删除请求必须匹配 Rust 后端保存的当前扫描计划，重新扫描后旧计划立即失效。
- 只移入系统回收站/废纸篓，不提供永久删除入口。
- 每次操作写入应用日志目录下的 `operations.jsonl`。

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

1. JPG 与 RAW 根目录不能相同或互相嵌套。
2. 扫描遇到目录权限或遍历错误时整体失败，不输出不完整删除计划。
3. 符号链接不会被递归跟随。
4. 前端只能提交 RAW 根目录下的相对路径。
5. Rust 后端拒绝绝对路径、`..`、非 `.nef/.xmp` 文件和越界后的规范路径。
6. 文件在扫描后发生变化时跳过，要求重新扫描。
7. 网络卷或特殊文件系统不支持系统废纸篓时，该文件会失败并保留在原处，不会退化为永久删除。

## 项目结构

```text
src/                         React/TypeScript 工作台
src-tauri/src/lib.rs         扫描、匹配、校验、回收站与日志
src-tauri/capabilities/      Tauri 最小权限
src-tauri/icons/             Windows/macOS 安装图标
.github/workflows/release.yml 跨平台构建矩阵
```

当前版本聚焦 Nikon `.NEF`、`.JPG/.JPEG` 和 `.xmp`。扩展到 Canon、Sony、Fujifilm 等格式时，应同时调整扫描白名单、删除白名单、界面规则和测试，不能只放宽前端输入。

## 许可

FramePair 采用 [PolyForm Noncommercial License 1.0.0](LICENSE.md)。个人学习、研究、实验和其他非商业用途可以在协议范围内使用、修改与分发；商业使用需要另行取得作者的书面授权。

本项目属于源码可用软件，不是 OSI 定义下的开源软件。项目所使用的第三方依赖继续适用各自的许可证，本协议不会改变这些第三方许可证。
