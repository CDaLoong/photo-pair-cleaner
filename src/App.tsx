import { useState } from "react";
import { AppShell } from "./app/AppShell";
import type { AppModule } from "./app/AppShell";
import { CleanupModule } from "./features/cleanup/CleanupModule";
import { PreviewModule } from "./features/preview/PreviewModule";

function App() {
  const [activeModule, setActiveModule] = useState<AppModule>("preview");

  return (
    <AppShell activeModule={activeModule} onModuleChange={setActiveModule}>
      <div className="module-panel" hidden={activeModule !== "preview"}>
        <PreviewModule active={activeModule === "preview"} />
      </div>
      <div className="module-panel" hidden={activeModule !== "cleanup"}>
        <CleanupModule active={activeModule === "cleanup"} />
      </div>
    </AppShell>
  );
}

export default App;
