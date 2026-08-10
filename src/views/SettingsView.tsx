import { useState } from 'react';
import { useStore } from '../state/store';
import { Field, FolderPicker, ProgressBar, Select, Toggle } from '../components/ui';
import { formatBytes, formatMegabytes } from '../format';
import './SettingsView.css';

export default function SettingsView() {
  const store = useStore();
  const { settings, patchSettings } = store;
  const [activationKey, setActivationKey] = useState('');
  const [activatingRelay, setActivatingRelay] = useState(false);
  const [javaProgress, setJavaProgress] = useState<{ progress: number; message: string; operationId?: string; cancelling?: boolean } | null>(null);
  const backupBytes = store.backups.filter((backup) => !backup.failed).reduce((total, backup) => total + backup.size, 0);

  return (
    <div className="view">
      <div className="view-header">
        <div>
          <h1 className="view-title">Settings</h1>
          <p className="view-subtitle">Application preferences</p>
        </div>
      </div>

      <div className="dash-body">
        <div className="dash-section settings-section">
          <h2 className="section-title">Relay access</h2>
          <div className={`settings-card relay-access-card ${store.relayAccess.activated ? 'is-active' : ''}`}>
            <div className="relay-access-status">
              <span className="relay-access-dot" aria-hidden="true" />
              <div>
                <div className="font-semibold">{store.relayAccess.activated ? 'Activated on this device' : 'Activation required'}</div>
                <p className="text-muted text-sm">
                  {store.relayAccess.activated
                    ? 'One running server can use a Nooki public address. Stop it to automatically free the relay for another running server.'
                    : 'Enter a single-use activation key to unlock one adaptive relay slot for this installation.'}
                </p>
              </div>
            </div>
            {store.relayAccess.activated ? (
              <div className="relay-access-meta">
                <div><span>Activation</span><strong className="mono">{store.relayAccess.activationId}</strong></div>
                <div><span>Relay slots</span><strong>{store.relayAccess.serversAllowed} server</strong></div>
                <div><span>Device</span><strong className="mono">{store.relayAccess.deviceId}</strong></div>
              </div>
            ) : (
              <form className="relay-activation-form" onSubmit={(event) => {
                event.preventDefault();
                if (!activationKey.trim() || activatingRelay) return;
                setActivatingRelay(true);
                void store.activateRelay(activationKey).then(() => {
                  setActivationKey('');
                  store.pushToast({ tone: 'success', title: 'Relay activated', detail: 'One running server can now receive a public Nooki address.' });
                }).catch((error) => {
                  store.pushToast({ tone: 'error', title: 'Activation failed', detail: String((error as { message?: string })?.message ?? error) });
                }).finally(() => setActivatingRelay(false));
              }}>
                <input
                  className="input mono relay-activation-input"
                  value={activationKey}
                  onChange={(event) => setActivationKey(event.target.value.toUpperCase())}
                  placeholder="NK-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX"
                  aria-label="Relay activation key"
                  autoComplete="off"
                  spellCheck={false}
                  disabled={activatingRelay}
                />
                <button className="btn btn-primary" type="submit" disabled={!activationKey.trim() || activatingRelay}>
                  {activatingRelay ? 'Activating...' : 'Activate relay'}
                </button>
              </form>
            )}
          </div>
        </div>

        <div className="dash-section settings-section">
          <h2 className="section-title">Folders</h2>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--s-5)' }}>
            <Field label="Default server folder" hint="New servers are created here unless you pick a different location.">
              <FolderPicker value={settings.serverFolder} onChange={(serverFolder) => patchSettings({ serverFolder })} />
            </Field>
            <Field label="Backup folder" hint="Nooki saves all backups in this folder.">
              <FolderPicker value={settings.backupFolder} onChange={(backupFolder) => patchSettings({ backupFolder })} />
            </Field>
          </div>
        </div>

        <div className="dash-section settings-section">
          <h2 className="section-title">Behavior</h2>
          <div>
            <Toggle
              checked={settings.minimizeToTray}
              onChange={(minimizeToTray) => patchSettings({ minimizeToTray })}
              label="Minimize to system tray"
              hint="When you close the window, Nooki stays in the tray so running servers keep going."
            />
            <Toggle
              checked={settings.launchOnLogin}
              onChange={(launchOnLogin) => patchSettings({ launchOnLogin })}
              label="Launch on Windows login"
              hint="Start Nooki automatically when you log in."
            />
          </div>
        </div>

        <div className="dash-section settings-section">
          <h2 className="section-title">Java runtimes</h2>
          <div className="settings-toolbar">
            <button className="btn btn-secondary" onClick={() => void store.detectJava()}>Scan this computer</button>
            <div className="settings-install-control">
              <span className="settings-control-label">Install managed runtime</span>
              <Select
                className="settings-runtime-select"
                value=""
                placeholder="Choose version"
                ariaLabel="Install a managed Java runtime"
                options={[8, 17, 21, 25].map((major) => ({ value: String(major), label: `Java ${major}` }))}
                onChange={(value) => { const major = Number(value); if (!major) return; setJavaProgress({ progress: 0, message: `Preparing Java ${major}` }); void store.installJava(major, (update) => setJavaProgress((current) => ({ progress: update.data.progress ?? 0, message: update.data.message, operationId: update.data.operationId, cancelling: current?.cancelling }))).then(() => setJavaProgress(null)).catch((error) => { setJavaProgress(null); if ((error as { code?: string })?.code !== 'cancelled') store.pushToast({ tone: 'error', title: 'Java installation failed', detail: String((error as { message?: string })?.message ?? error) }); }); }}
              />
            </div>
          </div>
          {javaProgress && <div className="settings-progress"><ProgressBar value={javaProgress.progress} tone="info" /><span className="text-muted text-sm">{javaProgress.message}</span><button className="btn btn-secondary btn-sm" disabled={!javaProgress.operationId || javaProgress.cancelling} onClick={() => { if (!javaProgress.operationId) return; setJavaProgress({ ...javaProgress, cancelling: true, message: 'Cancelling Java installation…' }); void store.cancelOperation(javaProgress.operationId).catch(() => setJavaProgress((current) => current ? { ...current, cancelling: false } : null)); }}>{javaProgress.cancelling ? 'Cancelling…' : 'Cancel'}</button></div>}
          <div className="settings-card runtime-list">
            {store.javaRuntimes.length === 0 ? (
              <p className="settings-empty text-muted text-sm">No Java runtimes detected yet. Nooki will offer a managed runtime when a server needs one.</p>
            ) : store.javaRuntimes.map((runtime) => (
              <div key={runtime.id} className="runtime-row">
                <div className="runtime-details">
                  <div className="runtime-title">
                    <span className="font-semibold">{runtime.label}</span>
                    <span className="runtime-source">{runtime.bundled ? 'Managed' : 'System'}</span>
                  </div>
                  <div className="runtime-path text-muted text-sm mono" title={runtime.path}>{runtime.path}</div>
                </div>
                <div className="runtime-meta text-muted text-sm">
                  <span className="runtime-architecture">{runtime.architecture}</span>
                  {runtime.usedBy ? <span>Used by {runtime.usedBy} server{runtime.usedBy === 1 ? '' : 's'}</span> : <span>Not in use</span>}
                  {runtime.bundled && runtime.usedBy === 0 && <button className="btn btn-sm btn-ghost" onClick={() => void store.removeJava(runtime.id)}>Remove</button>}
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="dash-section settings-section">
          <h2 className="section-title">Storage</h2>
          <div className="settings-card storage-list">
            <div className="storage-row"><span className="storage-label">Server files</span><span className="storage-value mono">{formatMegabytes(store.servers.reduce((total, server) => total + server.diskUsed, 0))}</span></div>
            <div className="storage-row"><span className="storage-label">Backups</span><span className="storage-value mono">{formatBytes(backupBytes)}</span></div>
            <div className="storage-row"><span className="storage-label">Computer disk use</span><span className="storage-value mono">{formatMegabytes(store.host.diskUsed)} / {formatMegabytes(store.host.diskTotal)}</span></div>
          </div>
        </div>

        <div className="dash-section settings-section">
          <h2 className="section-title">About</h2>
          <p className="text-muted text-sm">Nooki {store.appVersion}. Application updates are installed manually.</p>
        </div>
      </div>
    </div>
  );
}
