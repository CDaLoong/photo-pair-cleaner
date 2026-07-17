import { Aperture, Images, ListChecks, ShieldCheck } from "lucide-react";
import type { ReactNode } from "react";

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

export function AppShell({ activeModule, onModuleChange, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <aside className="module-sidebar">
        <div className="sidebar-brand">
          <Aperture aria-hidden="true" size={24} strokeWidth={2.25} />
          <span><strong>FramePair</strong><small>本地摄影工作台</small></span>
        </div>

        <nav className="module-navigation" aria-label="功能模块">
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
