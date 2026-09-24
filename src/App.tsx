import { useEffect, useState } from 'react';
import { StoreProvider, useStore } from './state/store';
import Sidebar from './components/Sidebar';
import Dashboard from './views/Dashboard';
import ServersView from './views/ServersView';
import BackupsView from './views/BackupsView';
import QuickServerView from './views/SharingView';
import SettingsView from './views/SettingsView';
import ServerDetail from './views/ServerDetail';
import AddServerWizard from './views/AddServerWizard';
import TopBar, { TopBarSlotProvider } from './components/TopBar';
import CommandMenu from './components/CommandMenu';
import Toaster from './components/Toaster';
import { IconServer } from './components/Icons';
import { ConfirmDialog, EmptyState, Spinner } from './components/ui';
import { TooltipProvider } from './components/ui/tooltip';
import './styles/global.css';
import './App.css';
import './styles/shadcn-overrides.css';

function AppContent() {
  const store = useStore();
  const selectedServer = store.servers.find((s) => s.id === store.openServerId);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const [commandOpen, setCommandOpen] = useState(false);

  useEffect(() => {
    if (!store.ready && !store.initError) return;
    const startup = document.getElementById('nooki-startup');
    if (!startup) return;
    startup.classList.add('is-ready');
    const remove = () => startup.remove();
    startup.addEventListener('transitionend', remove, { once: true });
    const fallback = window.setTimeout(remove, 300);
    return () => window.clearTimeout(fallback);
  }, [store.ready, store.initError]);

  if (!store.ready) {
    return (
      <div className="app-shell">
        <div className="app-panel is-solo">
          <TopBar bare />
          <div className="app-boot">
            {store.initError
              ? <EmptyState icon={<IconServer size={22} />} title="Nooki could not start" description={store.initError.message} action={<button className="btn btn-primary" onClick={store.retryInitialize}>Try again</button>} />
              : <div className="stack-sm" style={{ alignItems: 'center' }}><Spinner size={18} /><span className="text-muted text-sm">Loading your servers</span></div>}
          </div>
        </div>
      </div>
    );
  }

  // Keying the view host on location replays the short entrance whenever the
  // content swaps, so a new page settles in rather than teleporting.
  const viewKey = store.nav === 'servers' && selectedServer ? `server:${selectedServer.id}` : store.nav;

  return (
    <div className="app-shell">
      <Sidebar currentView={store.nav} onNavigate={store.setNav} onOpenCommand={() => setCommandOpen(true)} />
      <TopBarSlotProvider element={slot}>
        <main className="app-panel">
          <TopBar onSlot={setSlot} />
          <div key={viewKey} className="view-host view-enter">
            {store.nav === 'dashboard' && <Dashboard />}
            {store.nav === 'servers' && !selectedServer && <ServersView />}
            {store.nav === 'servers' && selectedServer && <ServerDetail server={selectedServer} />}
            {store.nav === 'backups' && <BackupsView />}
            {store.nav === 'quick-server' && <QuickServerView />}
            {store.nav === 'settings' && <SettingsView />}
          </div>
        </main>
      </TopBarSlotProvider>
      {store.wizardOpen && <AddServerWizard onClose={() => store.setWizardOpen(false)} />}
      <CommandMenu open={commandOpen} onOpenChange={setCommandOpen} />
      <Toaster />
      <ConfirmDialog open={store.quitDialog === 'quit'} title="Stop servers and quit Nooki?" description="Nooki will save each running world and wait for its server process to exit." confirmLabel="Stop servers and quit" tone="danger" onCancel={() => store.setQuitDialog('closed')} onConfirm={() => { void store.quit(false).then((closed) => { if (!closed) store.setQuitDialog('tray'); }); }} />
      <ConfirmDialog open={store.quitDialog === 'tray'} title="Some servers did not stop" description="Force quitting terminates the remaining Java processes. Recent unsaved world changes may be lost." confirmLabel="Force quit" tone="danger" onCancel={() => store.setQuitDialog('closed')} onConfirm={() => { void store.quit(true); }} />
    </div>
  );
}

export default function App() {
  return (
    <TooltipProvider>
      <StoreProvider>
        <AppContent />
      </StoreProvider>
    </TooltipProvider>
  );
}
