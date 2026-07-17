import { AppShell } from "./app/AppShell";
import { CleanupModule } from "./features/cleanup/CleanupModule";

function App() {
  return (
    <AppShell activeModule="cleanup">
      <CleanupModule />
    </AppShell>
  );
}

export default App;
