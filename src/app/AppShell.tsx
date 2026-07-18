import {
  Aperture,
  Images,
  ListChecks,
  PanelLeftClose,
  PanelLeftOpen,
  ShieldCheck,
} from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import { storedBooleanPreference } from "../utils";

export type AppModule = "preview" | "cleanup";

interface AppShellProps {
  activeModule: AppModule;
  onModuleChange: (module: AppModule) => void;
  children: ReactNode;
}

const MODULES = [
  { id: "preview", label: "照片浏览", detail: "预览与筛选", icon: Images },
  { id: "cleanup", label: "配对清理", detail: "筛选与安全处理", icon: ListChecks },
] as const;

const MODULE_SIDEBAR_STORAGE_KEY = "framepair.layout.module-sidebar-collapsed.v1";

export function AppShell({ activeModule, onModuleChange, children }: AppShellProps) {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    try {
      return storedBooleanPreference(localStorage.getItem(MODULE_SIDEBAR_STORAGE_KEY));
    } catch {
      return false;
    }
  });

  function toggleSidebar() {
    setSidebarCollapsed((current) => {
      const next = !current;
      try {
        localStorage.setItem(MODULE_SIDEBAR_STORAGE_KEY, String(next));
      } catch {
        // The current session remains usable if layout preferences cannot be stored.
      }
      return next;
    });
  }

  return (
    <div className={sidebarCollapsed ? "app-shell is-module-sidebar-collapsed" : "app-shell"}>
      <aside className={sidebarCollapsed ? "module-sidebar is-collapsed" : "module-sidebar"} aria-label="功能模块侧边栏">
        <div className="sidebar-brand">
          <Aperture aria-hidden="true" size={24} strokeWidth={2.25} />
          <span><strong>FramePair</strong><small>本地摄影工作台</small></span>
          <button
            className="sidebar-collapse-control"
            type="button"
            onClick={toggleSidebar}
            aria-label={sidebarCollapsed ? "展开功能模块侧边栏" : "收起功能模块侧边栏"}
            aria-expanded={!sidebarCollapsed}
            aria-controls="module-navigation"
            title={sidebarCollapsed ? "展开功能栏" : "收起功能栏"}
          >
            {sidebarCollapsed
              ? <PanelLeftOpen aria-hidden="true" size={17} />
              : <PanelLeftClose aria-hidden="true" size={17} />}
          </button>
        </div>

        <nav id="module-navigation" className="module-navigation" aria-label="功能模块">
          {MODULES.map(({ id, label, detail, icon: Icon }) => (
            <button
              key={id}
              className={id === activeModule ? "module-nav-item is-active" : "module-nav-item"}
              type="button"
              onClick={() => onModuleChange(id)}
              aria-current={id === activeModule ? "page" : undefined}
              title={label}
            >
              <Icon aria-hidden="true" size={19} />
              <span><strong>{label}</strong><small>{detail}</small></span>
            </button>
          ))}
        </nav>

        <div className="sidebar-safety" title="所有照片仅在本机处理">
          <ShieldCheck aria-hidden="true" size={17} />
          <span><strong>仅在本机处理</strong><small>不会上传照片</small></span>
        </div>
      </aside>

      <div className="module-frame">{children}</div>
    </div>
  );
}
