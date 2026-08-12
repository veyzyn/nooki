import { useMemo, useState } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useStore } from '../state/store';
import type { Server, ServerStatus } from '../types';
import { IconPlus, IconSearch, IconServer, IconX, IconDots, IconCopy } from '../components/Icons';
import ServerIcon from '../components/ServerIcon';
import { EmptyState, Field, Menu, Modal, Segmented, Sparkline, Spinner, ConfirmDialog } from '../components/ui';
import { formatMegabytes, formatUptime, isBusy, softwareLabel, statusLabels, statusTone } from '../format';
import AddServerWizard from './AddServerWizard';
import './Dashboard.css';
import './ServersView.css';

type Filter = 'all' | 'running' | 'stopped' | 'issues';

export default function ServersView() {
  const store = useStore();
  const { servers, wizardOpen, setWizardOpen } = store;
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [confirmStop, setConfirmStop] = useState<string | null>(null);
  const [removeTarget, setRemoveTarget] = useState<{ id: string; mode: 'forget' | 'recycle' } | null>(null);
  const [removeConfirmation, setRemoveConfirmation] = useState('');
  const [removing, setRemoving] = useState(false);
  const [copiedRemovalName, setCopiedRemovalName] = useState(false);

  const visible = useMemo(() => {
    return servers.filter((s) => {
      if (filter === 'running' && s.status !== 'running') return false;
      if (filter === 'stopped' && s.status !== 'stopped') return false;
      if (filter === 'issues' && s.status !== 'crashed' && s.alerts.length === 0) return false;
      if (query) {
        const hay = `${s.name} ${softwareLabel(s.type)} ${s.version} ${s.port}`.toLowerCase();
        if (!hay.includes(query.toLowerCase())) return false;
      }
      return true;
    });
  }, [servers, filter, query]);

  const stopTarget = servers.find((s) => s.id === confirmStop);
  const removalServer = servers.find((s) => s.id === removeTarget?.id);

  const copyRemovalName = () => {
    if (!removalServer) return;
    void writeText(removalServer.name).then(() => {
      setCopiedRemovalName(true);
      window.setTimeout(() => setCopiedRemovalName(false), 1400);
    }).catch((error) => store.pushToast({ tone: 'error', title: 'Name was not copied', detail: String(error) }));
  };

  const recycleServer = async () => {
    if (!removalServer || removeConfirmation !== removalServer.name || removing) return;
    setRemoving(true);
    try {
      await store.removeServer(removalServer.id, 'recycle', removeConfirmation);
      setRemoveTarget(null);
      setRemoveConfirmation('');
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Server was not removed', detail: String((error as { message?: string })?.message ?? error) });
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div className="view">
      <div className="view-header">
        <div>
          <h1 className="view-title">Servers</h1>
          <p className="view-subtitle">
            {servers.length} server{servers.length !== 1 ? 's' : ''} on this computer
          </p>
        </div>
        <button className="btn btn-primary" onClick={() => setWizardOpen(true)}>
          <IconPlus size={15} />
          Add server
        </button>
      </div>

      <div className="dash-body">
        {servers.length > 0 && (
          <div className="srv-toolbar">
            <div className="console-search">
              <IconSearch size={14} />
              <input
                className="console-search-input"
                placeholder="Search by name, version, or port"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              {query && (
                <button className="icon-btn" onClick={() => setQuery('')} aria-label="Clear search">
                  <IconX size={12} />
                </button>
              )}
            </div>
            <Segmented
              value={filter}
              onChange={setFilter}
              options={[
                { value: 'all', label: 'All' },
                { value: 'running', label: 'Running' },
                { value: 'stopped', label: 'Stopped' },
                { value: 'issues', label: 'Needs attention' },
              ]}
            />
          </div>
        )}

        {servers.length === 0 ? (
          <EmptyState
            icon={<IconServer size={44} />}
            title="No servers yet"
            description="Create a fresh Minecraft server or bring in one you already run from a folder on this computer."
            action={
              <button className="btn btn-primary" onClick={() => setWizardOpen(true)}>
                <IconPlus size={15} />
                Add server
              </button>
            }
          />
        ) : visible.length === 0 ? (
          <EmptyState
            icon={<IconSearch size={40} />}
            title="Nothing matches that"
            description="Try a different search or clear the filter to see all your servers."
            action={
              <button
                className="btn btn-secondary"
                onClick={() => {
                  setQuery('');
                  setFilter('all');
                }}
              >
                Clear filters
              </button>
            }
          />
        ) : (
          <div className="server-list">
            {visible.map((server) => (
              <ServerCard key={server.id} server={server} onRequestStop={() => setConfirmStop(server.id)} onRemove={(mode) => { setRemoveTarget({ id: server.id, mode }); setRemoveConfirmation(''); setCopiedRemovalName(false); }} />
            ))}
          </div>
        )}
      </div>

      <ConfirmDialog
        open={stopTarget !== undefined}
        title={`Stop ${stopTarget?.name}?`}
        description={stopTarget?.status === 'starting'
          ? 'Nooki will stop Java even though Minecraft has not finished starting.'
          : 'Everyone currently playing will be disconnected.'}
        confirmLabel="Stop server"
        tone="danger"
        notes={
          stopTarget?.status === 'starting'
            ? ['If Minecraft does not exit within 60 seconds, you can force stop it.']
            : stopTarget && stopTarget.players > 0
            ? [`${stopTarget.players} player${stopTarget.players !== 1 ? 's are' : ' is'} online.`, 'The world is saved first.']
            : ['The world is saved first.']
        }
        onCancel={() => setConfirmStop(null)}
        onConfirm={() => {
          if (stopTarget) store.stopServer(stopTarget.id);
          setConfirmStop(null);
        }}
      />

      <ConfirmDialog
        open={removeTarget?.mode === 'forget' && Boolean(removalServer)}
        title={`Remove ${removalServer?.name} from Nooki?`}
        description="The server folder and all backup files stay where they are. You can import it again later."
        confirmLabel="Remove from Nooki"
        tone="danger"
        onCancel={() => setRemoveTarget(null)}
        onConfirm={() => { if (removalServer) void store.removeServer(removalServer.id, 'forget').catch((error) => store.pushToast({ tone: 'error', title: 'Server was not removed', detail: String((error as { message?: string })?.message ?? error) })); setRemoveTarget(null); }}
      />

      <Modal
        open={removeTarget?.mode === 'recycle' && Boolean(removalServer)}
        onClose={() => { if (!removing) setRemoveTarget(null); }}
        dismissable={!removing}
        title="Move server files to the Recycle Bin?"
        description="The exact registered folder will be removed from this computer. External backups are kept."
        tone="danger"
        width={520}
        footer={<>
          <button className="btn btn-secondary" disabled={removing} onClick={() => setRemoveTarget(null)}>Cancel</button>
          <button className="btn btn-danger" disabled={removeConfirmation !== removalServer?.name || removing} onClick={() => void recycleServer()}>
            {removing && <Spinner size={12} />}{removing ? 'Moving files…' : 'Move to Recycle Bin'}
          </button>
        </>}
      >
        <div className="recycle-confirm">
          <div className="recycle-path-block">
            <span>Folder being removed</span>
            <code title={removalServer?.folder}>{removalServer?.folder}</code>
          </div>
          <div className="recycle-confirm-copy">
            <div>
              <strong>Confirm the server name</strong>
              <span>Copy the exact name, then type it below.</span>
            </div>
            <button className={`recycle-name-chip ${copiedRemovalName ? 'is-copied' : ''}`} type="button" onClick={copyRemovalName} title="Copy server name">
              <span>{removalServer?.name}</span>
              <IconCopy size={13} />
              <em>{copiedRemovalName ? 'Copied' : 'Copy'}</em>
            </button>
          </div>
          <Field label="Server name">
            <input className="input" autoFocus value={removeConfirmation} onChange={(event) => setRemoveConfirmation(event.target.value)} placeholder="Type the server name exactly" disabled={removing} />
          </Field>
        </div>
      </Modal>

      {wizardOpen && <AddServerWizard onClose={() => setWizardOpen(false)} />}
    </div>
  );
}

function ServerCard({ server, onRequestStop, onRemove }: { server: Server; onRequestStop: () => void; onRemove: (mode: 'forget' | 'recycle') => void }) {
  const store = useStore();
  const busy = isBusy(server.status);
  const operationBusy = (store.backupFlow?.serverId === server.id && store.backupFlow.phase === 'running')
    || (store.restoreFlow?.serverId === server.id && (store.restoreFlow.phase === 'safety' || store.restoreFlow.phase === 'restoring'))
    || (store.updateFlow?.serverId === server.id && !['confirm', 'done', 'failed'].includes(store.updateFlow.phase));
  const running = server.status === 'running';
  const starting = server.status === 'starting';
  const removable = server.status === 'stopped' || server.status === 'crashed';
  const attention = server.status === 'crashed' || server.alerts.some((a) => a.severity !== 'info');
  const storedCpu = server.history.map((sample) => sample.cpu);
  const cpuHistory = [...storedCpu.slice(0, -1), server.cpu];

  const open = (tab?: Parameters<typeof store.openServer>[1]) => store.openServer(server.id, tab);

  return (
    <div
      className={`server-row ${attention ? 'needs-attention' : ''}`}
      onClick={() => open()}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          open();
        }
      }}
    >
      <div className="server-row-icon">
        <ServerIcon server={server} size={40} />
      </div>

      <div className="server-row-main">
        <div className="server-row-top">
          <span className="server-row-name">{server.name}</span>
          <span className={`status-badge status-${statusTone(server.status)}`}>
            {busy && <Spinner size={10} />}
            {statusLabels[server.status]}
          </span>
          {server.alerts.length > 0 && server.status !== 'crashed' && (
            <span className="alert-count">
              {server.alerts.length} notice{server.alerts.length !== 1 ? 's' : ''}
            </span>
          )}
        </div>

        <div className="server-row-sub">
          <span>
            {softwareLabel(server.type)} {server.version}
          </span>
          <span>·</span>
          <span>
            {server.players}/{server.maxPlayers} players
          </span>
          <span>·</span>
          <span className="mono">:{server.port}</span>
          {running && (
            <>
              <span>·</span>
              <span>{formatUptime(server.startedAt)} up</span>
            </>
          )}
          {server.status === 'crashed' && (
            <>
              <span>·</span>
              <span className="sub-danger">stopped unexpectedly</span>
            </>
          )}
        </div>

        {running && (
          <div className="server-row-history">
            <Sparkline data={cpuHistory} color={server.accent} height={32} label={server.id} />
          </div>
        )}
      </div>

      <div className="server-row-meta">
        {running && (
          <div className="server-row-stats">
            <div className="sstat">
              <span className="sstat-l">CPU</span>
              <span className="sstat-v">{server.cpu}%</span>
            </div>
            <div className="sstat">
              <span className="sstat-l">Memory</span>
              <span className="sstat-v">{formatMegabytes(server.memory)}</span>
            </div>
          </div>
        )}

        <div className="server-row-actions" onClick={(e) => e.stopPropagation()}>
          {(server.status === 'stopped' || server.status === 'crashed') && (
            <button className="btn btn-sm btn-primary" disabled={busy || operationBusy} onClick={() => store.startServer(server.id)}>
              Start
            </button>
          )}
          {(running || starting) && (
            <>
              {running && (
                <button className="btn btn-sm btn-ghost" disabled={operationBusy} onClick={() => store.restartServer(server.id)}>
                  Restart
                </button>
              )}
              <button className="btn btn-sm btn-ghost" disabled={operationBusy} onClick={onRequestStop}>
                Stop
              </button>
            </>
          )}
          {busy && <Spinner size={14} />}

          <Menu
            trigger={
              <button className="btn btn-sm btn-icon btn-ghost" aria-label={`More actions for ${server.name}`}>
                <IconDots size={14} />
              </button>
            }
            items={[
              { label: 'Open console', onSelect: () => open('console') },
              { label: 'Players', onSelect: () => open('players') },
              { label: 'Backups', onSelect: () => open('backups') },
              { label: 'Settings', onSelect: () => open('settings') },
              {
                label: 'Create backup',
                onSelect: () => open('backups'),
                disabled: operationBusy,
                hint: running && server.players > 0 ? 'players online' : undefined,
              },
              {
                label: 'Open folder',
                onSelect: () => store.revealPath(server.folder),
              },
              { label: 'Remove from Nooki', onSelect: () => onRemove('forget'), disabled: !removable },
              { label: 'Move files to Recycle Bin', onSelect: () => onRemove('recycle'), disabled: !removable },
            ]}
          />
        </div>
      </div>
    </div>
  );
}

export type { ServerStatus };
