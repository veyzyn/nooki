import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useStore } from '../state/store';
import type { Server, ServerTab } from '../types';
import { IconCheck, IconCopy, IconPlay } from '../components/Icons';
import { PageActions } from '../components/TopBar';
import { ConfirmDialog, Spinner } from '../components/ui';
import { isBusy, statusLabels } from '../format';
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

const FilesTab = lazy(() => import('./tabs/FilesTab'));

const baseTabs: { id: ServerTab; label: string }[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'console', label: 'Console' },
  { id: 'players', label: 'Players' },
  { id: 'worlds', label: 'Worlds' },
  { id: 'files', label: 'Files' },
  { id: 'databases', label: 'Databases' },
  { id: 'settings', label: 'Settings' },
  { id: 'logs', label: 'Logs' },
  { id: 'backups', label: 'Backups' },
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
  const starting = server.status === 'starting';
  const shutdownStuck = (server.status === 'stopping' || server.status === 'restarting')
    && server.alerts.some((alert) => alert.kind === 'stop-timeout');
  const address = server.sharing.address ?? `localhost:${server.port}`;
  const tabs = server.type === 'paper'
    ? [
      ...baseTabs.slice(0, 3),
      { id: 'plugins' as const, label: 'Plugins' },
      ...baseTabs.slice(3),
    ]
    : (server.type === 'fabric' || server.type === 'forge' || server.type === 'neoforge')
      ? [
        ...baseTabs.slice(0, 3),
        { id: 'mods' as const, label: 'Mods' },
        ...baseTabs.slice(3),
      ]
      : baseTabs;

  const copyAddress = () => {
    void writeText(address);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  return (
    <div className="view detail">
      <PageActions>
        <button
          className={`address-chip ${copied ? 'is-copied' : ''}`}
          onClick={copyAddress}
          title={server.sharing.address ? 'Copy public address' : 'Copy local address'}
          aria-label={copied ? 'Address copied' : `Copy address ${address}`}
        >
          <span className="mono">{address}</span>
          <span className="icon-swap" data-swapped={copied || undefined} aria-hidden="true">
            <IconCopy size={12} />
            <IconCheck size={13} />
          </span>
        </button>
        {busy && (
          <span className="busy-note">
            <Spinner size={12} />
            {statusLabels[server.status]}
          </span>
        )}
        {shutdownStuck && (
          <button className="btn btn-sm btn-danger" onClick={() => store.forceStopServer(server.id)}>Force stop</button>
        )}
        {(server.status === 'stopped' || server.status === 'crashed') && (
          <button className="btn btn-sm btn-primary" disabled={busy || operationBusy} onClick={() => store.startServer(server.id)}>
            <IconPlay size={12} /> Start
          </button>
        )}
        {(running || starting) && (
          <>
            {running && (
              <button className="btn btn-sm btn-secondary" disabled={operationBusy} onClick={() => store.restartServer(server.id)}>
                Restart
              </button>
            )}
            <button className="btn btn-sm btn-secondary" disabled={operationBusy} onClick={() => setConfirmStop(true)}>
              Stop
            </button>
          </>
        )}
      </PageActions>

      <TabBar
        tabs={tabs.map((tab) => ({
          ...tab,
          count: tab.id === 'players' && running && server.players > 0 ? server.players : undefined,
        }))}
        value={serverTab}
        onChange={setServerTab}
      />

      <div key={serverTab} className="detail-body tab-enter">
        {serverTab === 'overview' && <OverviewTab server={server} />}
        {serverTab === 'console' && <ConsoleTab server={server} />}
        {serverTab === 'players' && <PlayersTab server={server} />}
        {server.type === 'paper' && serverTab === 'plugins' && <PluginsTab server={server} />}
        {(server.type === 'fabric' || server.type === 'forge' || server.type === 'neoforge') && serverTab === 'mods' && <ModsTab server={server} />}
        {serverTab === 'databases' && <DatabasesTab server={server} />}
        {serverTab === 'worlds' && <WorldsTab server={server} />}
        {serverTab === 'files' && (
          <Suspense fallback={<div className="files-tab-loader"><div className="files-tab-loader-head"><span /><span /></div><div className="files-tab-loader-panel"><div className="files-tab-loader-toolbar" />{Array.from({ length: 7 }, (_, index) => <div key={index} className="files-tab-loader-row" />)}</div></div>}>
            <FilesTab server={server} />
          </Suspense>
        )}
        {serverTab === 'settings' && <ServerSettingsTab server={server} />}
        {serverTab === 'logs' && <LogsTab server={server} />}
        {serverTab === 'backups' && <ServerBackupsTab server={server} />}
      </div>

      <ConfirmDialog
        open={confirmStop}
        title={`Stop ${server.name}?`}
        description={starting ? 'Nooki will stop Java even though Minecraft has not finished starting.' : 'Everyone currently playing will be disconnected.'}
        confirmLabel="Stop server"
        tone="danger"
        notes={
          starting
            ? ['If Minecraft does not exit within 60 seconds, you can force stop it.']
            : server.players > 0
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

/* Underline tabs. The indicator is a 1px-wide bar positioned and stretched
   with transform only, so sliding it never touches layout. It moves between
   tabs (spatial consistency: where did the selection go) with ease-in-out. */
function TabBar({ tabs, value, onChange }: { tabs: { id: ServerTab; label: string; count?: number }[]; value: ServerTab; onChange: (tab: ServerTab) => void }) {
  const navRef = useRef<HTMLElement>(null);
  const indicatorRef = useRef<HTMLSpanElement>(null);
  const [settled, setSettled] = useState(false);

  useLayoutEffect(() => {
    const nav = navRef.current;
    const indicator = indicatorRef.current;
    if (!nav || !indicator) return undefined;
    const place = () => {
      const button = nav.querySelector<HTMLElement>(`[data-tab="${value}"]`);
      if (!button) { indicator.style.opacity = '0'; return; }
      indicator.style.opacity = '1';
      indicator.style.transform = `translateX(${button.offsetLeft + 8}px) scaleX(${Math.max(0, button.offsetWidth - 16)})`;
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(nav);
    return () => observer.disconnect();
  }, [value, tabs.length]);

  useEffect(() => {
    const frame = requestAnimationFrame(() => setSettled(true));
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <nav ref={navRef} className="detail-tabs" aria-label="Server sections" data-settled={settled || undefined}>
      {tabs.map((tab) => (
        <button
          key={tab.id}
          data-tab={tab.id}
          className={`detail-tab ${value === tab.id ? 'active' : ''}`}
          onClick={() => onChange(tab.id)}
          aria-current={value === tab.id ? 'page' : undefined}
        >
          <span>{tab.label}</span>
          {tab.count !== undefined && <span className="tab-count">{tab.count}</span>}
        </button>
      ))}
      <span ref={indicatorRef} className="detail-tab-indicator" aria-hidden="true" />
    </nav>
  );
}
