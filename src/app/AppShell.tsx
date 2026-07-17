import { Aperture, ListChecks, ShieldCheck } from "lucide-react";
import type { ReactNode } from "react";

export type AppModule = "cleanup";

interface AppShellProps {
  activeModule: AppModule;
  children: ReactNode;
}

const MODULES = [
  { id: "cleanup", label: "配对清理", detail: "筛选与安全处理", icon: ListChecks },
] as const;

export function AppShell({ activeModule, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <aside className="module-sidebar">
        <div className="sidebar-brand">
          <Aperture aria-hidden="true" size={24} strokeWidth={2.25} />
          <span><strong>FramePair</strong><small>本地摄影工作台</small></span>
        </div>

        <nav className="module-navigation" aria-label="功能模块">
          {MODULES.map(({ id, label, detail, icon: Icon }) => (
            <div key={id} className={id === activeModule ? "module-nav-item is-active" : "module-nav-item"} aria-current={id === activeModule ? "page" : undefined} title={label}>
              <Icon aria-hidden="true" size={19} />
              <span><strong>{label}</strong><small>{detail}</small></span>
            </div>
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
