import assert from "node:assert/strict";
import test from "node:test";
import * as utils from "../src/utils.ts";

const candidates = [
  { status: "delete", kind: "raw", sizeBytes: 12 },
  { status: "delete", kind: "sidecar", sizeBytes: 3 },
  { status: "keep", kind: "raw", sizeBytes: 100 },
];

test("reclaimable bytes exclude sidecars when the option is disabled", () => {
  assert.equal(typeof utils.reclaimableBytes, "function");
  assert.equal(utils.reclaimableBytes(candidates, false), 12);
});

test("reclaimable bytes include sidecars when the option is enabled", () => {
  assert.equal(typeof utils.reclaimableBytes, "function");
  assert.equal(utils.reclaimableBytes(candidates, true), 15);
});

test("a rescan failure keeps the completed deletion result", () => {
  assert.equal(typeof utils.noticeAfterRescanFailure, "function");
  assert.deepEqual(
    utils.noticeAfterRescanFailure(
      { tone: "success", title: "2 个文件已移入回收站/废纸篓" },
      "目录不可访问",
    ),
    {
      tone: "success",
      title: "2 个文件已移入回收站/废纸篓",
      detail: "清理已执行，但自动重新扫描失败：目录不可访问",
    },
  );
});
