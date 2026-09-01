import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const capabilitiesDir = new URL("../src-tauri/capabilities/", import.meta.url);
const manifestPath = new URL("../src-tauri/gen/schemas/acl-manifests.json", import.meta.url);
const srcDir = new URL("../src/", import.meta.url);

// ACL 清单由 cargo build 生成，src-tauri/gen/ 未入库，所以 CI 的前端 job 上可能没有。
// 缺失时退化为只校验显式授权，配合下面的 CORE_DEFAULT_BASELINE 仍能覆盖关键路径。
const manifest = fs.existsSync(manifestPath)
  ? JSON.parse(fs.readFileSync(manifestPath, "utf8"))
  : null;

// 前端 API -> 所需 ACL 权限。新增 Tauri API 时必须在此登记，
// 否则下面的测试会失败并指出缺哪一条映射。
const DIALOG_PERMISSIONS = {
  open: "dialog:allow-open",
  save: "dialog:allow-save",
  confirm: "dialog:allow-confirm",
  message: "dialog:allow-message",
  ask: "dialog:allow-ask",
};

const WINDOW_PERMISSIONS = {
  close: ["core:window:allow-close"],
  // onCloseRequested 本身只是监听，但 JS 包装层在处理函数未 preventDefault 时
  // 会自动调用 destroy()。少了 allow-destroy，点关闭按钮会关不掉窗口。
  onCloseRequested: ["core:event:allow-listen", "core:window:allow-destroy"],
  scaleFactor: ["core:window:allow-scale-factor"],
};

const WEBVIEW_PERMISSIONS = {
  onDragDropEvent: ["core:event:allow-listen"],
};

// core:default 已经提供、因此无需在 capabilities 里重复声明的权限。
// 有 ACL 清单时会反查这份名单，防止它与上游默认值脱节。
const CORE_DEFAULT_BASELINE = [
  "core:event:allow-listen",
  "core:window:allow-scale-factor",
];

// 权限标识形如 `<scope>:<name>`，而 scope 本身可能带冒号（`core:window`）。
// 因此只能按最长前缀匹配清单的顶层键来切分。
function splitIdentifier(identifier) {
  const scopes = Object.keys(manifest ?? {})
    .filter((scope) => identifier.startsWith(`${scope}:`))
    .sort((a, b) => b.length - a.length);
  if (scopes.length === 0) return null;
  return { scope: scopes[0], name: identifier.slice(scopes[0].length + 1) };
}

// 把权限集递归展开成叶子权限。`core:default` 会套好几层，
// 手工阅读极易误判某个 allow-* 是否真的被包含。
function expand(identifier, seen = new Set()) {
  if (seen.has(identifier)) return new Set();
  seen.add(identifier);

  const parsed = splitIdentifier(identifier);
  if (!parsed) return new Set([identifier]);
  const { scope, name } = parsed;
  const module = manifest[scope] ?? {};

  const group = name === "default"
    ? module.default_permission
    : module.permission_sets?.[name];
  if (!group) {
    // 不是集合，那它要么是叶子权限，要么根本不存在。
    return new Set([module.permissions?.[name] ? identifier : `${identifier} (未定义)`]);
  }

  const leaves = new Set();
  for (const entry of group.permissions ?? []) {
    const qualified = splitIdentifier(entry) ? entry : `${scope}:${entry}`;
    for (const leaf of expand(qualified, seen)) leaves.add(leaf);
  }
  return leaves;
}

function capabilityFiles() {
  return fs
    .readdirSync(capabilitiesDir)
    .filter((file) => file.endsWith(".json"))
    .map((file) => JSON.parse(fs.readFileSync(new URL(file, capabilitiesDir), "utf8")));
}

function declaredPermissions() {
  const declared = new Set();
  for (const capability of capabilityFiles()) {
    for (const entry of capability.permissions ?? []) {
      declared.add(typeof entry === "string" ? entry : entry.identifier);
    }
  }
  return declared;
}

function grantedPermissions() {
  const granted = new Set(CORE_DEFAULT_BASELINE);
  for (const identifier of declaredPermissions()) {
    if (manifest) for (const leaf of expand(identifier)) granted.add(leaf);
    else granted.add(identifier);
  }
  return granted;
}

function sourceFiles(dir = srcDir) {
  const files = [];
  for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
    const child = new URL(`${item.name}${item.isDirectory() ? "/" : ""}`, dir);
    if (item.isDirectory()) files.push(...sourceFiles(child));
    else if (/\.tsx?$/.test(item.name)) files.push(child);
  }
  return files;
}

const source = sourceFiles()
  .map((file) => fs.readFileSync(file, "utf8"))
  .join("\n");

function usedDialogFunctions() {
  const used = new Set();
  const importPattern = /import\s*\{([^}]*)\}\s*from\s*"@tauri-apps\/plugin-dialog"/g;
  for (const match of source.matchAll(importPattern)) {
    for (const specifier of match[1].split(",")) {
      const original = specifier.trim().split(/\s+as\s+/)[0].trim();
      if (original) used.add(original);
    }
  }
  return used;
}

function usedMethods(accessor) {
  const used = new Set();
  const pattern = new RegExp(`${accessor}\\(\\)\\.([A-Za-z]+)`, "g");
  for (const match of source.matchAll(pattern)) used.add(match[1]);
  return used;
}

test("每个前端用到的 dialog 函数都有对应授权", () => {
  const granted = grantedPermissions();
  const used = usedDialogFunctions();
  assert.ok(used.size > 0, "未在前端发现 plugin-dialog 的调用，检测逻辑可能已失效");

  for (const fn of used) {
    const permission = DIALOG_PERMISSIONS[fn];
    assert.ok(permission, `前端用到了 dialog.${fn}，请在 DIALOG_PERMISSIONS 中登记它所需的权限`);
    assert.ok(
      granted.has(permission),
      `前端调用了 dialog.${fn}，但 capabilities 未授予 ${permission}`,
    );
  }
});

test("每个前端用到的窗口和 webview 方法都有对应授权", () => {
  const granted = grantedPermissions();
  for (const [accessor, table] of [
    ["getCurrentWindow", WINDOW_PERMISSIONS],
    ["getCurrentWebview", WEBVIEW_PERMISSIONS],
  ]) {
    const used = usedMethods(accessor);
    assert.ok(used.size > 0, `未在前端发现 ${accessor} 的调用，检测逻辑可能已失效`);
    for (const method of used) {
      const required = table[method];
      assert.ok(required, `前端用到了 ${accessor}().${method}，请登记它所需的权限`);
      for (const permission of required) {
        assert.ok(
          granted.has(permission),
          `前端调用了 ${accessor}().${method}，但 capabilities 未授予 ${permission}`,
        );
      }
    }
  }
});

test("关窗权限在 capabilities 中显式声明", () => {
  // core:window:default 只含只读 getter，关窗能力不会随 core:default 一起来。
  const declared = declaredPermissions();
  for (const permission of ["core:window:allow-close", "core:window:allow-destroy"]) {
    assert.ok(declared.has(permission), `capabilities 缺少显式授权 ${permission}`);
  }
});

test("capabilities 与 ACL 清单一致", { skip: manifest ? false : "缺少生成的 ACL 清单" }, () => {
  for (const leaf of grantedPermissions()) {
    assert.doesNotMatch(
      leaf,
      /\(未定义\)$/,
      `capabilities 引用了 ACL 清单中不存在的权限：${leaf.replace(" (未定义)", "")}`,
    );
  }

  // 只读基线必须真的来自 core:default，否则上面的断言会放过缺失的授权。
  const coreDefault = expand("core:default");
  for (const permission of CORE_DEFAULT_BASELINE) {
    assert.ok(
      coreDefault.has(permission),
      `${permission} 已不在 core:default 中，需要在 capabilities 里显式授予`,
    );
  }
});

test("tauri.conf.json 声明的每个窗口都被 capability 覆盖", () => {
  const config = JSON.parse(
    fs.readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
  );
  const covered = new Set(capabilityFiles().flatMap((capability) => capability.windows ?? []));
  for (const window of config.app?.windows ?? []) {
    assert.ok(covered.has(window.label), `窗口 ${window.label} 没有任何 capability 覆盖`);
  }
});
