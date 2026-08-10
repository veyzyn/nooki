import { useEffect, useState } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useStore } from '../../state/store';
import type { DatabaseEnvironment, DatabaseKind, ManagedDatabase, Server } from '../../types';
import { Callout, ConfirmDialog, EmptyState, Field, Modal, ProgressBar, Spinner } from '../../components/ui';
import { IconCopy, IconDatabase, IconPlay, IconPlus, IconRefresh, IconStop, IconTrash } from '../../components/Icons';
import './DatabasesTab.css';

const engines: { kind: DatabaseKind; label: string; description: string; mark: string }[] = [
  { kind: 'mysql', label: 'MySQL', description: 'Popular relational database', mark: 'my' },
  { kind: 'postgresql', label: 'PostgreSQL', description: 'Advanced relational database', mark: 'pg' },
  { kind: 'mongodb', label: 'MongoDB', description: 'Document database', mark: 'm' },
  { kind: 'redis', label: 'Redis', description: 'Fast in-memory data store', mark: 'r' },
];

const statusLabels: Record<ManagedDatabase['status'], string> = {
  running: 'Running', stopped: 'Stopped', creating: 'Creating', error: 'Needs attention', missing: 'Container missing',
};

const creationStages = [
  { id: 'pull', label: 'Image' },
  { id: 'storage', label: 'Storage' },
  { id: 'configure', label: 'Configure' },
  { id: 'start', label: 'Start' },
  { id: 'ready', label: 'Ready' },
] as const;

function messageFrom(error: unknown) {
  if (typeof error === 'object' && error && 'message' in error) return String((error as { message: unknown }).message);
  return String(error);
}

function engineFor(kind: DatabaseKind) {
  return engines.find((engine) => engine.kind === kind) ?? engines[0];
}

function environmentTitle(environment: DatabaseEnvironment) {
  switch (environment.code) {
    case 'cli-not-found': return 'Docker command line tools were not found';
    case 'daemon-not-ready': return 'Docker engine is still starting';
    case 'daemon-unavailable': return 'Docker engine is unavailable';
    case 'permission-denied': return 'Docker access was denied';
    case 'cli-launch-failed': return 'Docker could not be launched';
    default: return 'Docker is unavailable';
  }
}

export default function DatabasesTab({ server }: { server: Server }) {
  const store = useStore();
  const [environment, setEnvironment] = useState<DatabaseEnvironment | null>(null);
  const [databases, setDatabases] = useState<ManagedDatabase[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [kind, setKind] = useState<DatabaseKind>('mysql');
  const [name, setName] = useState('minecraft');
  const [creating, setCreating] = useState(false);
  const [createProgress, setCreateProgress] = useState(0);
  const [createMessage, setCreateMessage] = useState('Preparing database');
  const [createPhase, setCreatePhase] = useState('pull');
  const [createElapsed, setCreateElapsed] = useState(0);
  const [operationId, setOperationId] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [changing, setChanging] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ManagedDatabase | null>(null);
  const [revealed, setRevealed] = useState<Set<string>>(new Set());
  const [copied, setCopied] = useState<string | null>(null);

  const refresh = async (quiet = false) => {
    if (quiet) setRefreshing(true); else setLoading(true);
    try {
      const [nextEnvironment, nextDatabases] = await Promise.all([
        store.databaseEnvironment(),
        store.listDatabases(server.id),
      ]);
      setEnvironment(nextEnvironment);
      setDatabases(nextDatabases);
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Could not load databases', detail: messageFrom(error) });
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => { void refresh(); }, [server.id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!creating) { setCreateElapsed(0); return; }
    const started = Date.now();
    const timer = window.setInterval(() => setCreateElapsed(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [creating]);

  const openCreate = () => {
    setKind('mysql');
    setName('minecraft');
    setCreateProgress(0);
    setCreateMessage('Preparing database');
    setCreatePhase('pull');
    setOperationId(null);
    setCancelling(false);
    setCreateOpen(true);
  };

  const create = async () => {
    setCreating(true);
    setCreateProgress(0);
    setOperationId(null);
    try {
      const database = await store.createDatabase(server.id, { kind, name: name.trim() }, (event) => {
        setOperationId(event.data.operationId);
        setCreateMessage(event.data.message);
        if (event.data.phase) setCreatePhase(event.data.phase);
        if (event.event === 'progress') setCreateProgress(event.data.progress ?? 0);
        if (event.event === 'finished') setCreateProgress(100);
      });
      setDatabases((current) => [database, ...current]);
      setCreateOpen(false);
      store.pushToast({ tone: 'success', title: `${engineFor(kind).label} database ready`, detail: `${database.host}:${database.port}` });
    } catch (error) {
      if ((error as { code?: string })?.code === 'cancelled') {
        setCreateOpen(false);
      } else {
        store.pushToast({ tone: 'error', title: 'Database was not created', detail: messageFrom(error) });
      }
    } finally {
      setCreating(false);
      setCancelling(false);
      setOperationId(null);
    }
  };

  const action = async (database: ManagedDatabase, nextAction: 'start' | 'stop' | 'restart') => {
    setChanging(database.id);
    try {
      const next = await store.databaseAction(database.id, nextAction);
      setDatabases((current) => current.map((item) => item.id === next.id ? next : item));
    } catch (error) {
      store.pushToast({ tone: 'error', title: `Could not ${nextAction} database`, detail: messageFrom(error) });
    } finally {
      setChanging(null);
    }
  };

  const remove = async () => {
    if (!deleteTarget) return;
    const target = deleteTarget;
    setDeleteTarget(null);
    setChanging(target.id);
    try {
      await store.deleteDatabase(target.id);
      setDatabases((current) => current.filter((database) => database.id !== target.id));
      store.pushToast({ tone: 'success', title: `${target.name} deleted`, detail: 'Its container and persistent data volume were removed.' });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Database was not deleted', detail: messageFrom(error) });
    } finally {
      setChanging(null);
    }
  };

  const copy = (key: string, value: string) => {
    void writeText(value);
    setCopied(key);
    window.setTimeout(() => setCopied((current) => current === key ? null : current), 1400);
  };

  const validName = /^[A-Za-z][A-Za-z0-9_]{0,31}$/.test(name.trim());
  const currentStage = Math.max(0, creationStages.findIndex((stage) => stage.id === createPhase));
  const elapsedLabel = createElapsed < 60 ? `${createElapsed}s` : `${Math.floor(createElapsed / 60)}m ${String(createElapsed % 60).padStart(2, '0')}s`;

  return (
    <div className="tab databases-tab">
      <div className="databases-toolbar">
        <div>
          <h2>Databases</h2>
          <p>Local data services for this server, kept separate from its files and lifecycle.</p>
          {environment?.available && <span className="docker-ready"><span /> Docker {environment.version}{environment.context ? ` · ${environment.context}` : ''}</span>}
        </div>
        <div className="databases-toolbar-actions">
          <button className="btn btn-secondary btn-sm" disabled={refreshing} onClick={() => void refresh(true)}>
            {refreshing ? <Spinner size={12} /> : <IconRefresh size={13} />} Refresh
          </button>
          <button className="btn btn-primary" disabled={!environment?.available} onClick={openCreate}>
            <IconPlus size={14} /> Create database
          </button>
        </div>
      </div>

      {!loading && environment && !environment.available && (
        <Callout tone="warning" title={environmentTitle(environment)} action={<button className="btn btn-secondary btn-sm" onClick={() => void refresh(true)}>Run diagnostics again</button>}>
          <div className="docker-diagnostic">
            <p>{environment.message ?? 'Nooki could not connect to Docker Desktop.'}</p>
            {(environment.cliPath || environment.context) && (
              <div className="docker-diagnostic-facts">
                {environment.cliPath && <div><span>Docker CLI</span><code>{environment.cliPath}</code></div>}
                {environment.context && <div><span>Context</span><code>{environment.context}</code></div>}
              </div>
            )}
            {environment.suggestions?.length > 0 && <ul>{environment.suggestions.map((suggestion) => <li key={suggestion}>{suggestion}</li>)}</ul>}
            {environment.details?.length > 0 && <details><summary>Technical details</summary>{environment.details.map((detail) => <code key={detail}>{detail}</code>)}</details>}
          </div>
        </Callout>
      )}

      {loading ? (
        <div className="database-list" aria-label="Loading databases">
          {[0, 1].map((item) => <div className="database-card database-skeleton" key={item}><i /><div><i /><i /></div><i /></div>)}
        </div>
      ) : databases.length === 0 ? (
        <div className="databases-empty">
          <EmptyState
            icon={<IconDatabase size={38} />}
            title="No databases yet"
            description="Create MySQL, PostgreSQL, MongoDB, or Redis with generated credentials and persistent local storage."
            action={environment?.available ? <button className="btn btn-primary" onClick={openCreate}>Create database</button> : undefined}
          />
        </div>
      ) : (
        <div className="database-list">
          {databases.map((database) => {
            const engine = engineFor(database.kind);
            const visible = revealed.has(database.id);
            const busy = changing === database.id;
            return (
              <article className="database-card" key={database.id}>
                <div className={`database-engine database-engine-${database.kind}`}>{engine.mark}</div>
                <div className="database-content">
                  <div className="database-title-row">
                    <div>
                      <strong>{database.name}</strong>
                      <span>{engine.label}</span>
                    </div>
                    <span className={`database-status is-${database.status}`}>{statusLabels[database.status]}</span>
                  </div>
                  <div className="database-details">
                    <div><span>Endpoint</span><code>{database.host}:{database.port}</code></div>
                    <div><span>Username</span><code>{database.username}</code></div>
                    <div><span>Password</span><code>{visible ? database.password : '••••••••••••'}</code></div>
                  </div>
                  <div className="database-connection">
                    <code>{visible ? database.connectionUri : `${database.kind}://••••••••@${database.host}:${database.port}/${database.database}`}</code>
                    <button className="btn btn-secondary btn-sm" onClick={() => copy(`uri-${database.id}`, database.connectionUri)}><IconCopy size={12} /> {copied === `uri-${database.id}` ? 'Copied' : 'Copy URI'}</button>
                    <button className="btn btn-ghost btn-sm" onClick={() => setRevealed((current) => { const next = new Set(current); if (next.has(database.id)) next.delete(database.id); else next.add(database.id); return next; })}>{visible ? 'Hide' : 'Reveal'}</button>
                  </div>
                  {database.lastError && <p className="database-error">{database.lastError}</p>}
                </div>
                <div className="database-actions">
                  {busy ? <Spinner size={14} /> : database.status === 'running' ? (
                    <button className="btn btn-secondary btn-sm" disabled={!environment?.available} onClick={() => void action(database, 'stop')}><IconStop size={12} /> Stop</button>
                  ) : (
                    <button className="btn btn-primary btn-sm" disabled={!environment?.available || database.status === 'missing'} onClick={() => void action(database, 'start')}><IconPlay size={12} /> Start</button>
                  )}
                  {database.status === 'running' && <button className="btn btn-ghost btn-sm" disabled={busy || !environment?.available} onClick={() => void action(database, 'restart')}>Restart</button>}
                  <button className="icon-btn database-delete" disabled={busy || !environment?.available} onClick={() => setDeleteTarget(database)} aria-label={`Delete ${database.name}`}><IconTrash size={14} /></button>
                </div>
              </article>
            );
          })}
        </div>
      )}

      <Modal
        open={createOpen}
        onClose={creating ? () => {} : () => setCreateOpen(false)}
        dismissable={!creating}
        title={creating ? 'Creating database' : 'Create a database'}
        description={creating ? undefined : 'Pick an engine and Nooki will generate the credentials and choose a free local port.'}
        width={540}
        className={creating ? 'database-progress-modal' : ''}
        footer={creating ? (
          <><span className="text-muted text-sm">The Minecraft server can stay running</span><button className="btn btn-secondary" disabled={!operationId || cancelling} onClick={() => { if (!operationId) return; setCancelling(true); setCreateMessage('Cancelling…'); void store.cancelOperation(operationId); }}>{cancelling ? 'Cancelling…' : 'Cancel'}</button></>
        ) : (
          <><button className="btn btn-secondary" onClick={() => setCreateOpen(false)}>Cancel</button><button className="btn btn-primary" disabled={!validName} onClick={() => void create()}>Create database</button></>
        )}
      >
        {creating ? (
          <div className="database-create-progress">
            <div className="database-progress-head">
              <div className={`database-engine database-engine-${kind}`}>{engineFor(kind).mark}</div>
              <div><span>{engineFor(kind).label}</span><strong>{createMessage}</strong></div>
              <b>{Math.round(createProgress)}%</b>
            </div>
            <ProgressBar value={createProgress} />
            <div className="database-progress-meta"><span>Step {currentStage + 1} of {creationStages.length}</span><span>{elapsedLabel} elapsed</span></div>
            <ol className="database-progress-stages">
              {creationStages.map((stage, index) => <li key={stage.id} className={index < currentStage ? 'is-done' : index === currentStage ? 'is-active' : ''}><span>{index < currentStage ? '✓' : index + 1}</span>{stage.label}</li>)}
            </ol>
          </div>
        ) : (
          <div className="database-create-form">
            <div className="database-engine-grid">
              {engines.map((engine) => <button type="button" key={engine.kind} className={`database-engine-choice ${kind === engine.kind ? 'is-selected' : ''}`} onClick={() => setKind(engine.kind)}><span className={`database-engine database-engine-${engine.kind}`}>{engine.mark}</span><strong>{engine.label}</strong><small>{engine.description}</small></button>)}
            </div>
            <Field label="Database name" hint="Letters, numbers, and underscores; begins with a letter.">
              <input className="input" value={name} maxLength={32} onChange={(event) => setName(event.target.value)} placeholder="minecraft" />
            </Field>
            <p className="database-local-note"><IconDatabase size={14} /> Only apps on this computer can connect. The database continues running independently of the Minecraft server.</p>
          </div>
        )}
      </Modal>

      <ConfirmDialog
        open={deleteTarget !== null}
        title={`Delete ${deleteTarget?.name ?? 'database'}?`}
        description="This permanently removes the database container and all data in its persistent volume."
        confirmLabel="Delete database"
        tone="danger"
        notes={['This cannot be undone.', 'Back up any data you need before continuing.']}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => void remove()}
      />
    </div>
  );
}
