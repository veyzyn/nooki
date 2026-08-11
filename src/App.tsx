import { StoreProvider, useStore } from './state/store';
import Sidebar from './components/Sidebar';
import Dashboard from './views/Dashboard';
import ServersView from './views/ServersView';
import BackupsView from './views/BackupsView';
import QuickServerView from './views/SharingView';
import SettingsView from './views/SettingsView';
import ServerDetail from './views/ServerDetail';
import Titlebar from './components/Titlebar';
import { IconServer, IconX } from './components/Icons';
import { ConfirmDialog, EmptyState, Spinner } from './components/ui';
import './styles/global.css';
import './App.css';

function AppContent() {
  const store = useStore();
  const selectedServer = store.servers.find((s) => s.id === store.openServerId);

  if (!store.ready) {
    return <div className="app-shell">
      <Titlebar />
      <div className="app" style={{ alignItems: 'center', justifyContent: 'center' }}>
        {store.initError ? <EmptyState icon={<IconServer size={44} />} title="Nooki could not start" description={store.initError.message} action={<button className="btn btn-primary" onClick={store.retryInitialize}>Try again</button>} /> : <div className="stack-sm" style={{ alignItems: 'center' }}><Spinner size={24} /><span className="text-muted text-sm">Loading your servers</span></div>}
      </div>
    </div>;
  }

  return (
    <div className="app-shell">
      <Titlebar />
      <div className="app">
        <Sidebar currentView={store.nav} onNavigate={store.setNav} />
        <main className="main-content">
          {store.nav === 'dashboard' && <Dashboard />}
          {store.nav === 'servers' && !selectedServer && <ServersView />}
          {store.nav === 'servers' && selectedServer && <ServerDetail server={selectedServer} />}
          {store.nav === 'backups' && <BackupsView />}
          {store.nav === 'quick-server' && <QuickServerView />}
          {store.nav === 'settings' && <SettingsView />}
        </main>
        {store.toasts.length > 0 && (
          <div className="toast-container">
            {store.toasts.map((toast) => (
              <div key={toast.id} className={`toast toast-${toast.tone}`}>
                <div className="toast-content">
                  <div className="toast-title">{toast.title}</div>
                  {toast.detail && <div className="toast-detail">{toast.detail}</div>}
                  {toast.progress !== undefined && (
                    <div className="toast-progress">
                      <div className="toast-progress-fill" style={{ width: `${toast.progress}%` }} />
                    </div>
                  )}
                </div>
                <button className="toast-close" onClick={() => store.dismissToast(toast.id)}>
                  <IconX size={12} />
                </button>
              </div>
            ))}
          </div>
        )}
        <ConfirmDialog open={store.quitDialog === 'quit'} title="Stop servers and quit Nooki?" description="Nooki will save each running world and wait for its server process to exit." confirmLabel="Stop servers and quit" tone="danger" onCancel={() => store.setQuitDialog('closed')} onConfirm={() => { void store.quit(false).then((closed) => { if (!closed) store.setQuitDialog('tray'); }); }} />
        <ConfirmDialog open={store.quitDialog === 'tray'} title="Some servers did not stop" description="Force quitting terminates the remaining Java processes. Recent unsaved world changes may be lost." confirmLabel="Force quit" tone="danger" onCancel={() => store.setQuitDialog('closed')} onConfirm={() => { void store.quit(true); }} />
      </div>
    </div>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <AppContent />
    </StoreProvider>
  );
}
