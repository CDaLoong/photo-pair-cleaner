import { isTauri } from "@tauri-apps/api/core";
import { useEffect } from "react";

interface DirectoryDropOptions {
  /** 为 false 时不挂载监听——用于「当前不可见的子任务」。 */
  active: boolean;
  /** 拖拽经过或离开时切换高亮。 */
  onHoverChange: (hovering: boolean) => void;
  /** 拖入了恰好一个路径。是否为目录由后端校验，这里不做判断。 */
  onDropDirectory: (path: string) => void;
  /** 一次拖入多个路径。本应用的目录选择语义上只接受一个。 */
  onRejectMultiple: () => void;
  /** 监听挂载失败（非 Tauri 环境或权限问题）。 */
  onError: (error: unknown) => void;
}

/**
 * 订阅 Tauri 的窗口级文件拖放事件，用于「把文件夹拖进来选目录」。
 *
 * 注意这是**窗口级**事件而非 DOM 的 dragover/drop：Tauri 的原生拖放不会
 * 冒泡到 WebView 的 DOM，所以拖放区的高亮必须由这里回调驱动，
 * 光靠 CSS `:hover` 是不会亮的。
 *
 * `@tauri-apps/api/webview` 采用动态 import，这样在浏览器里跑 Vite 预览时
 * 不会因为加载不到 Tauri 运行时而整个组件崩掉。
 */
export function useDirectoryDrop({
  active,
  onHoverChange,
  onDropDirectory,
  onRejectMultiple,
  onError,
}: DirectoryDropOptions): void {
  useEffect(() => {
    if (!active || !isTauri()) return;
    // 监听是异步注册的，组件可能在注册完成前就卸载了；
    // `disposed` 保证晚到的回调不再触碰已卸载组件的 state。
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) => getCurrentWebview().onDragDropEvent((event) => {
        if (disposed) return;
        if (event.payload.type === "leave") {
          onHoverChange(false);
          return;
        }
        if (event.payload.type === "over") {
          onHoverChange(true);
          return;
        }
        onHoverChange(false);
        if (event.payload.paths.length !== 1) {
          onRejectMultiple();
          return;
        }
        onDropDirectory(event.payload.paths[0]);
      }))
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(onError);

    return () => {
      disposed = true;
      unlisten?.();
    };
    // 回调由调用方以内联箭头函数传入，列进依赖会导致每次渲染都重新注册监听。
    // 这里只跟随 active 变化。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);
}
