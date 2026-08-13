import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { IconCloud, IconCopy, IconUpload } from '../components/Icons';
import { Select, Spinner } from '../components/ui';
import { formatClock } from '../format';
import { useConsoleLines, useStore } from '../state/store';
import type { EphemeralWorldScan, OperationEvent, Server, VersionOption } from '../types';
import './SharingView.css';

type QuickPhase = 'idle' | 'scanning' | 'select-version' | 'creating' | 'failed';

export default function QuickServerView() {
  const store = useStore();

  if (store.ephemeralServer) {
    return <QuickWorldSession server={store.ephemeralServer} />;
  }

  return (
    <div className="view">
      <div className="view-header">
        <div>
          <h1 className="view-title">Quick server</h1>
          <p className="view-subtitle">Turn a Minecraft world into a temporary server in one step</p>
        </div>
      </div>

      <div className="dash-body sharing-body">
        <QuickWorldDrop />
      </div>
    </div>
  );
}

function QuickWorldDrop() {
  const store = useStore();
  const { createEphemeralServer, listVersions, scanEphemeralWorld } = store;
  const [phase, setPhase] = useState<QuickPhase>('idle');
  const [dragging, setDragging] = useState(false);
  const [scan, setScan] = useState<EphemeralWorldScan | null>(null);
  const [versions, setVersions] = useState<VersionOption[]>([]);
  const [selectedVersion, setSelectedVersion] = useState('');
  const [progress, setProgress] = useState(0);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [operationId, setOperationId] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);

  const loadVersions = useCallback(async () => {
    const catalog = await listVersions('vanilla', false);
    setVersions(catalog.versions);
    setSelectedVersion((current) => current || catalog.versions[0]?.version || '');
  }, [listVersions]);

  const createWorld = useCallback(async (world: EphemeralWorldScan, version: string) => {
    setPhase('creating');
    setError('');
    setProgress(1);
    setOperationId(null);
    setCancelling(false);
    setMessage(`Preparing ${world.worldName}`);
    try {
      await createEphemeralServer({ sourcePath: world.sourcePath, version }, (event: OperationEvent) => {
        setOperationId(event.data.operationId);
        if (event.event !== 'progress') return;
        const raw = event.data.progress ?? 0;
        setProgress(event.data.phase === 'download' ? 30 + raw * 0.54 : raw);
        setMessage(event.data.message);
      });
    } catch (caught) {
      const cancelled = typeof caught === 'object' && caught !== null && 'code' in caught && (caught as { code?: string }).code === 'cancelled';
      setError(cancelled ? 'The temporary world setup was cancelled and its files were removed.' : errorText(caught));
      setPhase('failed');
      if (!cancelled) throw caught;
    }
  }, [createEphemeralServer]);

  const inspect = useCallback(async (path: string) => {
    setPhase('scanning');
    setScan(null);
    setError('');
    setMessage('Reading world metadata');
    try {
      const result = await scanEphemeralWorld(path);
      setScan(result);
      if (result.detectedVersion) {
        await createWorld(result, result.detectedVersion);
        return;
      }
      setPhase('select-version');
      setMessage('Choose the Minecraft version used by this world.');
      await loadVersions();
    } catch (caught) {
      setError(errorText(caught));
      setPhase('failed');
    }
  }, [createWorld, loadVersions, scanEphemeralWorld]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    getCurrentWindow().onDragDropEvent((event) => {
      if (disposed) return;
      if (event.payload.type === 'enter' || event.payload.type === 'over') setDragging(true);
      if (event.payload.type === 'leave') setDragging(false);
      if (event.payload.type === 'drop') {
        setDragging(false);
        const path = event.payload.paths[0];
        if (path) void inspect(path);
      }
    }).then((stop) => { if (disposed) stop(); else unlisten = stop; }).catch(() => {});
    return () => { disposed = true; unlisten?.(); };
  }, [inspect]);

  const choose = async (directory: boolean) => {
    const selected = await open(directory
      ? { directory: true, multiple: false, title: 'Choose a Minecraft world folder' }
      : { directory: false, multiple: false, title: 'Choose a Minecraft world ZIP', filters: [{ name: 'ZIP archive', extensions: ['zip'] }] });
    if (typeof selected === 'string') void inspect(selected);
  };

  const reset = () => {
    setPhase('idle');
    setScan(null);
    setVersions([]);
    setSelectedVersion('');
    setProgress(0);
    setMessage('');
    setError('');
    setOperationId(null);
    setCancelling(false);
  };

  const cancelCreate = async () => {
    if (!operationId || cancelling) return;
    setCancelling(true);
    setMessage('Cancelling and cleaning up…');
    try { await store.cancelOperation(operationId); } catch { setCancelling(false); }
  };

  return (
    <section className="quick-world">
      <div className="quick-world-heading">
        <div>
          <span className="quick-world-kicker">Quick world</span>
          <h2>Drop in a map. Start playing.</h2>
          <p>Nooki detects the version, gives the server 4 GB, and removes the temporary copy when you stop it.</p>
        </div>
        <span className="quick-world-memory">4 GB</span>
      </div>

      {phase === 'idle' ? (
        <div
          className={`quick-world-drop ${dragging ? 'is-dragging' : ''}`}
          onDragEnter={() => setDragging(true)}
          onDragOver={(event) => { event.preventDefault(); setDragging(true); }}
          onDragLeave={() => setDragging(false)}
        >
          <div className="quick-world-drop-icon"><IconUpload size={22} /></div>
          <strong>Drop a world folder or ZIP here</strong>
          <span>Nested wrapper folders in ZIP files are handled automatically.</span>
          <div className="quick-world-actions">
            <button className="btn btn-secondary" onClick={() => void choose(true)}>Choose folder</button>
            <button className="btn btn-secondary" onClick={() => void choose(false)}>Choose ZIP</button>
          </div>
        </div>
      ) : phase === 'scanning' || phase === 'creating' ? (
        <div className="quick-world-progress">
          <div className="quick-world-progress-top">
            <Spinner size={16} />
            <div><strong>{phase === 'scanning' ? 'Inspecting map' : scan?.worldName ?? 'Preparing world'}</strong><span>{message}</span></div>
            {phase === 'creating' && <span>{Math.round(progress)}%</span>}
            {phase === 'creating' && <button className="btn btn-secondary btn-sm" disabled={!operationId || cancelling} onClick={() => void cancelCreate()}>{cancelling ? 'Cancelling…' : 'Cancel'}</button>}
          </div>
          <div className="quick-world-progress-track"><span style={{ width: `${phase === 'scanning' ? 12 : Math.max(2, progress)}%` }} /></div>
        </div>
      ) : phase === 'select-version' && scan ? (
        <div className="quick-world-version">
          <div>
            <strong>{scan.worldName}</strong>
            <span>{message}</span>
          </div>
          <Select
            value={selectedVersion}
            onChange={setSelectedVersion}
            options={versions.map((version) => ({ value: version.version, label: `Minecraft ${version.version}` }))}
            placeholder={versions.length ? 'Choose a version' : 'Loading versions…'}
            disabled={!versions.length}
            ariaLabel="Minecraft version"
          />
          <button className="btn btn-primary" disabled={!selectedVersion} onClick={() => void createWorld(scan, selectedVersion).catch(() => {})}>Start world</button>
          <button className="btn btn-ghost" onClick={reset}>Cancel</button>
        </div>
      ) : (
        <div className="quick-world-error">
          <div><strong>Couldn&apos;t start that world</strong><span>{error || message}</span></div>
          <button className="btn btn-secondary" onClick={reset}>Try another map</button>
        </div>
      )}
    </section>
  );
}

function QuickWorldSession({ server }: { server: Server }) {
  const store = useStore();
  const lines = useConsoleLines(server.id);
  const [command, setCommand] = useState('');
  const outputRef = useRef<HTMLDivElement>(null);
  const stopping = server.status === 'stopping';
  const publicAddress = server.sharing.address;
  const address = publicAddress ?? (server.status === 'running' ? `localhost:${server.port}` : null);
  const addressKind = publicAddress ? 'Public' : 'Local';
  const addressState = publicAddress ? server.sharing.status : (address ? 'local' : server.sharing.status);

  useEffect(() => {
    if (outputRef.current) outputRef.current.scrollTop = outputRef.current.scrollHeight;
  }, [lines]);

  const copy = async () => {
    if (!address) return;
    await writeText(address);
    store.pushToast({ tone: 'success', title: 'Address copied', detail: address });
  };
  const send = () => {
    const value = command.trim();
    if (!value || server.status !== 'running') return;
    store.sendCommand(server.id, value);
    setCommand('');
  };

  return (
    <div className="view quick-session-view">
      <div className="quick-session-head">
        <div className="quick-session-title">
          <button className="quick-session-mark" onClick={() => address && void copy()} aria-label={address ? `Copy ${addressKind.toLowerCase()} address` : undefined}>
            <IconCloud size={22} />
          </button>
          <div>
            <span className="quick-world-kicker">Quick world</span>
            <h1>{server.name}</h1>
            <p>Minecraft {server.version} <span>·</span> 4 GB memory</p>
          </div>
        </div>
        <button className="btn btn-danger" disabled={stopping} onClick={() => store.stopServer(server.id)}>
          {stopping && <Spinner size={11} />}
          {stopping ? 'Stopping…' : 'Stop'}
        </button>
      </div>

      <div className="quick-session-body">
        <button className={`quick-session-address state-${addressState}`} disabled={!address} onClick={() => void copy()}>
          <span className="quick-session-dot" />
          {address && <span className="quick-session-address-kind">{addressKind}</span>}
          <span className="mono">{address ?? relayStatus(server, store.relayAccess.activated)}</span>
          {address && <IconCopy size={13} />}
        </button>

        <div className="quick-console" ref={outputRef}>
          {lines.length === 0 ? (
            <div className="quick-console-empty"><Spinner size={14} /><span>Waiting for server output…</span></div>
          ) : lines.slice(-600).map((line) => (
            <div className={`quick-console-line level-${line.level}`} key={line.id}>
              <span>{formatClock(line.at)}</span>
              <span>{line.text}</span>
            </div>
          ))}
        </div>

        <div className="quick-console-input">
          <span>/</span>
          <input
            value={command}
            disabled={server.status !== 'running'}
            placeholder={server.status === 'running' ? 'Send a command' : 'Server is starting'}
            onChange={(event) => setCommand(event.target.value)}
            onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); send(); } }}
          />
          <button className="btn btn-sm btn-primary" disabled={server.status !== 'running' || !command.trim()} onClick={send}>Send</button>
        </div>
      </div>
    </div>
  );
}

function relayStatus(server: Server, relayActivated: boolean) {
  if (server.status === 'starting') return 'Starting Minecraft…';
  if (!relayActivated) return 'Preparing local address…';
  if (server.sharing.status === 'error') return server.sharing.lastError ?? 'Relay connection failed';
  if (server.sharing.status === 'connecting') return 'Creating public address…';
  return 'Preparing public address…';
}

function errorText(error: unknown) {
  if (typeof error === 'object' && error && 'message' in error) return String((error as { message: unknown }).message);
  return String(error);
}
