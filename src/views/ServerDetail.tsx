import { useState, type ReactNode } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useStore } from '../state/store';
import type { Server, ServerTab } from '../types';
import {
  IconArrowLeft,
  IconBlock,
  IconBox,
  IconCopy,
  IconDatabase,
  IconFileText,
  IconGrid,
  IconGlobe,
  IconMod,
  IconPlug,
  IconSettings,
  IconTerminal,
  IconUsers,
} from '../components/Icons';
import { ConfirmDialog, Spinner } from '../components/ui';
import { isBusy, softwareLabel, statusLabels, statusTone } from '../format';
import OverviewTab from './tabs/OverviewTab';
import ConsoleTab from './tabs/ConsoleTab';
import PlayersTab from './tabs/PlayersTab';
import ServerSettingsTab from './tabs/ServerSettingsTab';
import LogsTab from './tabs/LogsTab';
import ServerBackupsTab from './tabs/ServerBackupsTab';
import PluginsTab from './tabs/PluginsTab';
import ModsTab from './tabs/ModsTab';
import DatabasesTab from './tabs/DatabasesTab';
import WorldsTab from './tabs/WorldsTab';
import './ServerDetail.css';

const baseTabs: { id: ServerTab; label: string; icon: ReactNode }[] = [
  { id: 'overview', label: 'Overview', icon: <IconGrid size={14} /> },
  { id: 'console', label: 'Console', icon: <IconTerminal size={14} /> },
  { id: 'players', label: 'Players', icon: <IconUsers size={14} /> },
  { id: 'worlds', label: 'Worlds', icon: <IconGlobe size={14} /> },
  { id: 'databases', label: 'Databases', icon: <IconDatabase size={14} /> },
  { id: 'settings', label: 'Settings', icon: <IconSettings size={14} /> },
  { id: 'logs', label: 'Logs', icon: <IconFileText size={14} /> },
  { id: 'backups', label: 'Backups', icon: <IconBox size={14} /> },
];

export default function ServerDetail({ server }: { server: Server }) {
  const store = useStore();
  const { serverTab, setServerTab } = store;
  const [confirmStop, setConfirmStop] = useState(false);
  const [copied, setCopied] = useState(false);

  const busy = isBusy(server.status);
  const operationBusy = (store.backupFlow?.serverId === server.id && store.backupFlow.phase === 'running')
    || (store.restoreFlow?.serverId === server.id && (store.restoreFlow.phase === 'safety' || store.restoreFlow.phase === 'restoring'))
    || (store.updateFlow?.serverId === server.id && !['confirm', 'done', 'failed'].includes(store.updateFlow.phase));
  const running = server.status === 'running';
  const shutdownStuck = (server.status === 'stopping' || server.status === 'restarting')
    && server.alerts.some((alert) => alert.kind === 'stop-timeout');
  const address = server.sharing.address ?? `localhost:${server.port}`;
  const tabs = server.type === 'paper'
    ? [
      ...baseTabs.slice(0, 3),
      { id: 'plugins' as const, label: 'Plugins', icon: <IconPlug size={14} /> },
      ...baseTabs.slice(3),
    ]
    : (server.type === 'fabric' || server.type === 'forge' || server.type === 'neoforge')
      ? [
        ...baseTabs.slice(0, 3),
        { id: 'mods' as const, label: 'Mods', icon: <IconMod size={14} /> },
        ...baseTabs.slice(3),
      ]
      : baseTabs;

  const copyAddress = () => {
    void writeText(address);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
    store.pushToast({ tone: 'info', title: 'Address copied', detail: address });
  };

  return (
    <div className="view detail">
      <header className="detail-head">
        <div className="detail-head-top">
          <button className="back-btn" onClick={store.closeServer}>
            <IconArrowLeft size={14} />
            All servers
          </button>
        </div>

        <div className="detail-identity">
          <div className="detail-icon">
            <IconBlock size={48} color={server.accent} />
          </div>
          <div className="detail-titles">
            <div className="detail-name-row">
              <h1 className="detail-name">{server.name}</h1>
              <span className={`status-badge status-${statusTone(server.status)}`}>
                {busy && <Spinner size={10} />}
                {statusLabels[server.status]}
              </span>
            </div>
            <div className="detail-meta">
              <span>
                {softwareLabel(server.type)} {server.version}
              </span>
              <span className="sep">·</span>
              <button className={`address-chip ${copied ? 'is-copied' : ''}`} onClick={copyAddress} title={server.sharing.address ? 'Copy public address' : 'Copy local address'}>
                <span className="mono">{address}</span>
                <IconCopy size={12} />
              </button>
              {running && (
                <>
                  <span className="sep">·</span>
                  <span>
                    {server.players}/{server.maxPlayers} online
                  </span>
                </>
              )}
            </div>
          </div>

          <div className="detail-controls">
            {shutdownStuck && (
              <button className="btn btn-danger" onClick={() => store.forceStopServer(server.id)}>Force stop</button>
            )}
            {(server.status === 'stopped' || server.status === 'crashed') && (
              <button className="btn btn-primary" disabled={busy || operationBusy} onClick={() => store.startServer(server.id)}>
                Start server
              </button>
            )}
            {running && (
              <>
                <button className="btn btn-secondary" disabled={busy || operationBusy} onClick={() => store.restartServer(server.id)}>
                  Restart
                </button>
                <button className="btn btn-secondary" disabled={busy || operationBusy} onClick={() => setConfirmStop(true)}>
                  Stop
                </button>
              </>
            )}
            {busy && (
              <span className="busy-note">
                <Spinner size={13} />
                {statusLabels[server.status]}
              </span>
            )}
          </div>
        </div>

        <nav className="detail-tabs" aria-label="Server sections">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              className={`detail-tab ${serverTab === tab.id ? 'active' : ''}`}
              onClick={() => setServerTab(tab.id)}
              aria-current={serverTab === tab.id ? 'page' : undefined}
            >
              {tab.icon}
              <span>{tab.label}</span>
              {tab.id === 'players' && running && server.players > 0 && (
                <span className="tab-count">{server.players}</span>
              )}
            </button>
          ))}
        </nav>
      </header>

      <div className="detail-body">
        {serverTab === 'overview' && <OverviewTab server={server} />}
        {serverTab === 'console' && <ConsoleTab server={server} />}
        {serverTab === 'players' && <PlayersTab server={server} />}
        {server.type === 'paper' && serverTab === 'plugins' && <PluginsTab server={server} />}
        {(server.type === 'fabric' || server.type === 'forge' || server.type === 'neoforge') && serverTab === 'mods' && <ModsTab server={server} />}
        {serverTab === 'databases' && <DatabasesTab server={server} />}
        {serverTab === 'worlds' && <WorldsTab server={server} />}
        {serverTab === 'settings' && <ServerSettingsTab server={server} />}
        {serverTab === 'logs' && <LogsTab server={server} />}
        {serverTab === 'backups' && <ServerBackupsTab server={server} />}
      </div>

      <ConfirmDialog
        open={confirmStop}
        title={`Stop ${server.name}?`}
        description="Everyone currently playing will be disconnected."
        confirmLabel="Stop server"
        tone="danger"
        notes={
          server.players > 0
            ? [`${server.players} player${server.players !== 1 ? 's are' : ' is'} online right now.`, 'The world is saved before stopping.']
            : ['The world is saved before stopping.']
        }
        onCancel={() => setConfirmStop(false)}
        onConfirm={() => {
          setConfirmStop(false);
          store.stopServer(server.id);
        }}
      />
    </div>
  );
}
