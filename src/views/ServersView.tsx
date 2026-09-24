import { useMemo, useState } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useStore } from '../state/store';
import type { Server, ServerStatus } from '../types';
import { IconPlus, IconSearch, IconServer, IconX, IconDots, IconCopy, IconPlay } from '../components/Icons';
import ServerIcon from '../components/ServerIcon';
import { PageActions } from '../components/TopBar';
import { EmptyState, Field, Menu, Modal, Segmented, Sparkline, Spinner, ConfirmDialog } from '../components/ui';
import { formatMegabytes, formatUptime, isBusy, softwareLabel, statusLabels } from '../format';
import './ServersView.css';

type Filter = 'all' | 'running' | 'stopped' | 'issues';

export default function ServersView() {
  const store = useStore();
  const { servers, setWizardOpen } = store;
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

  const groups = useMemo(() => {
    const order: { id: string; label: string; dot: string; match: (server: Server) => boolean }[] = [
      { id: 'attention', label: 'Needs attention', dot: 'is-crashed', match: needsAttention },
      { id: 'busy', label: 'In progress', dot: 'is-warning', match: (server) => isBusy(server.status) },
      { id: 'running', label: 'Running', dot: 'is-running', match: (server) => server.status === 'running' },
      { id: 'stopped', label: 'Stopped', dot: '', match: () => true },
    ];
    const remaining = [...visible];
    return order.map((group) => {
      const members = remaining.filter(group.match);
      for (const member of members) remaining.splice(remaining.indexOf(member), 1);
      return { ...group, members };
    }).filter((group) => group.members.length > 0);
  }, [visible]);

  return (
    <div className="view">
      <PageActions>
        <button className="btn btn-primary btn-sm" onClick={() => setWizardOpen(true)}>
          <IconPlus size={14} />
          Add server
        </button>
      </PageActions>

      {servers.length > 0 && (
        <div className="page-toolbar">
          <label className="srv-search">
            <IconSearch size={14} />
            <input
              placeholder="Filter by name, version, or port"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Filter servers"
            />
            {query && (
              <button className="icon-btn" onClick={() => setQuery('')} aria-label="Clear filter">
                <IconX size={12} />
              </button>
            )}
          </label>
          <div className="grow" />
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

      <div className="page">
        {servers.length === 0 ? (
          <div className="page-empty">
            <EmptyState
              icon={<IconServer size={18} />}
              title="No servers yet"
              description="Create a fresh Minecraft server or bring in one you already run from a folder on this computer."
              action={
                <button className="btn btn-primary" onClick={() => setWizardOpen(true)}>
                  <IconPlus size={14} />
                  Add server
                </button>
              }
            />
          </div>
        ) : visible.length === 0 ? (
          <div className="page-empty">
            <EmptyState
              icon={<IconSearch size={18} />}
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
          </div>
        ) : (
          <div className="srv-list" role="list">
            {groups.map((group) => (
              <section key={group.id} className="srv-group" aria-label={group.label}>
                <header className="srv-group-head">
                  <span className={`status-dot ${group.dot}`} aria-hidden="true" />
                  <span className="srv-group-label">{group.label}</span>
                  <span className="srv-group-count">{group.members.length}</span>
                </header>
                {group.members.map((server) => (
                  <ServerRow key={server.id} server={server} onRequestStop={() => setConfirmStop(server.id)} onRemove={(mode) => { setRemoveTarget({ id: server.id, mode }); setRemoveConfirmation(''); setCopiedRemovalName(false); }} />
                ))}
              </section>
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

    </div>
  );
}

function needsAttention(server: Server) {
  return server.status === 'crashed' || server.alerts.some((alert) => alert.severity !== 'info');
}

function ServerRow({ server, onRequestStop, onRemove }: { server: Server; onRequestStop: () => void; onRemove: (mode: 'forget' | 'recycle') => void }) {
  const store = useStore();
  const busy = isBusy(server.status);
  const operationBusy = (store.backupFlow?.serverId === server.id && store.backupFlow.phase === 'running')
    || (store.restoreFlow?.serverId === server.id && (store.restoreFlow.phase === 'safety' || store.restoreFlow.phase === 'restoring'))
    || (store.updateFlow?.serverId === server.id && !['confirm', 'done', 'failed'].includes(store.updateFlow.phase));
  const running = server.status === 'running';
  const starting = server.status === 'starting';
  const removable = server.status === 'stopped' || server.status === 'crashed';
  const cpuHistory = [...server.history.map((sample) => sample.cpu).slice(-40, -1), server.cpu];

  const open = (tab?: Parameters<typeof store.openServer>[1]) => store.openServer(server.id, tab);

  return (
    <div
      className={`srv-row ${server.status === 'crashed' ? 'is-crashed' : ''}`}
      onClick={() => open()}
      role="listitem"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.target !== e.currentTarget) return;
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          open();
        }
      }}
    >
      <div className="srv-cell srv-name-cell">
        <ServerIcon server={server} size={28} />
        <div className="srv-name-text">
          <span className="srv-name">{server.name}</span>
          <span className="srv-sub">
            {softwareLabel(server.type)} {server.version}
            {server.status === 'crashed' && <span className="srv-sub-danger"> · stopped unexpectedly</span>}
            {server.alerts.length > 0 && server.status !== 'crashed' && <span className="srv-sub-warn"> · {server.alerts.length} notice{server.alerts.length !== 1 ? 's' : ''}</span>}
          </span>
        </div>
      </div>

      <div className="srv-cell srv-spark" aria-hidden={!running}>
        {running && <Sparkline data={cpuHistory} color="var(--st-running)" height={22} label={`Processor history for ${server.name}`} maxValue={100} />}
      </div>

      <div className="srv-cell srv-metric">
        <span className="srv-metric-label">CPU</span>
        <span className="srv-metric-value">{running ? `${Math.round(server.cpu)}%` : '—'}</span>
      </div>
      <div className="srv-cell srv-metric">
        <span className="srv-metric-label">Memory</span>
        <span className="srv-metric-value">{running ? formatMegabytes(server.memory) : '—'}</span>
      </div>
      <div className="srv-cell srv-metric">
        <span className="srv-metric-label">Players</span>
        <span className="srv-metric-value">{server.players}/{server.maxPlayers}</span>
      </div>
      <div className="srv-cell srv-metric srv-metric-wide">
        <span className="srv-metric-label">{running ? 'Uptime' : 'Port'}</span>
        <span className="srv-metric-value">{running ? formatUptime(server.startedAt) : <span className="mono">{server.port}</span>}</span>
      </div>

      <div className="srv-cell srv-actions" onClick={(e) => e.stopPropagation()}>
        {busy && <span className="srv-busy"><Spinner size={12} />{statusLabels[server.status]}</span>}
        {(server.status === 'stopped' || server.status === 'crashed') && (
          <button className="btn btn-sm btn-secondary" disabled={busy || operationBusy} onClick={() => store.startServer(server.id)}>
            <IconPlay size={12} /> Start
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
            { label: 'Move files to Recycle Bin', onSelect: () => onRemove('recycle'), disabled: !removable, danger: true },
          ]}
        />
      </div>
    </div>
  );
}

export type { ServerStatus };
