import { useStore } from '../state/store';
import type { Server } from '../types';
import { IconPlus } from '../components/Icons';
import ServerIcon from '../components/ServerIcon';
import { formatUptime, formatRelative, formatMegabytes, statusLabels, statusTone, isBusy, softwareLabel } from '../format';
import { Callout, Sparkline, Spinner } from '../components/ui';
import AddServerWizard from './AddServerWizard';
import './Dashboard.css';

export default function Dashboard() {
  const store = useStore();
  const { servers, activity, wizardOpen, setWizardOpen } = store;

  const runningCount = servers.filter((s) => s.status === 'running').length;
  const totalPlayers = servers.reduce((sum, s) => sum + s.players, 0);
  const totalMemory = servers.reduce((sum, s) => sum + s.memory, 0);
  const allocatedMemory = servers.reduce((sum, s) => sum + s.maxMemory, 0);
  const crashedCount = servers.filter((s) => s.status === 'crashed').length;
  const lastBackupEvent = activity.find((e) => e.kind === 'backup' && !e.message.toLowerCase().includes('failed'));

  return (
    <div className="view dashboard">
      <div className="view-header">
        <div>
          <h1 className="view-title">Dashboard</h1>
          <p className="view-subtitle">
            {runningCount === 0 ? 'No servers running' : `${runningCount} server${runningCount !== 1 ? 's' : ''} running`}
            {totalPlayers > 0 && ` · ${totalPlayers} player${totalPlayers !== 1 ? 's' : ''} online`}
          </p>
        </div>
        <button className="btn btn-primary" onClick={() => setWizardOpen(true)}>
          <IconPlus size={15} />
          Add server
        </button>
      </div>

      <div className="dash-body">
        <div className="dashboard-stats">
          <div className="stat-card">
            <div className="stat-label">Running</div>
            <div className="stat-value">{runningCount} <span className="stat-denom">/ {servers.length}</span></div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Players online</div>
            <div className="stat-value">{totalPlayers}</div>
          </div>
          <div className={`stat-card ${crashedCount > 0 ? 'stat-card-warn' : ''}`}>
            <div className="stat-label">Issues</div>
            <div className="stat-value">{crashedCount > 0 ? `${crashedCount} crashed` : 'None'}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Memory in use</div>
            <div className="stat-value">{formatMegabytes(totalMemory)} <span className="stat-denom">/ {formatMegabytes(allocatedMemory)}</span></div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Last backup</div>
            <div className="stat-value">{lastBackupEvent ? '3h ago' : 'Never'}</div>
          </div>
        </div>

        {crashedCount > 0 && (
          <div className="dash-section">
            {servers.filter((s) => s.status === 'crashed').map((s) => (
              <Callout key={s.id} tone="error" title={`${s.name} stopped unexpectedly`} action={
                <button className="btn btn-sm btn-secondary" onClick={() => store.startServer(s.id)}>Start</button>
              }>
                {s.lastExit ?? 'No crash details available. Check the console log for more information.'}
              </Callout>
            ))}
          </div>
        )}

        <div className="dash-section">
          <h2 className="section-title">Servers</h2>
          <div className="server-list">
            {servers.map((server) => (
              <ServerRow key={server.id} server={server} />
            ))}
          </div>
        </div>

        <div className="dash-section">
          <h2 className="section-title">Recent activity</h2>
          <div className="activity-list">
            {activity.slice(0, 8).map((event) => {
              const icons: Record<string, string> = {
                backup: '⬛',
                restart: '↺',
                crash: '!',
                update: '↑',
                start: '▶',
                stop: '■',
                restore: '←',
                settings: '⚙',
              };
              return (
                <div key={event.id} className="activity-item">
                  <span className={`activity-dot dot-${event.kind}`}>{icons[event.kind] ?? '·'}</span>
                  <div className="activity-body">
                    <span className="activity-msg">{event.message}</span>
                    {event.serverName && <span className="activity-srv">{event.serverName}</span>}
                  </div>
                  <span className="activity-time">{formatRelative(event.at)}</span>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {wizardOpen && <AddServerWizard onClose={() => setWizardOpen(false)} />}
    </div>
  );
}

function ServerRow({ server }: { server: Server }) {
  const store = useStore();
  const busy = isBusy(server.status);
  const storedCpu = server.history.map((sample) => sample.cpu);
  const cpuHistory = [...storedCpu.slice(0, -1), server.cpu];

  return (
    <div className="server-row" onClick={() => store.openServer(server.id)}>
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
        </div>
        <div className="server-row-sub">
          <span>{softwareLabel(server.type)} {server.version}</span>
          <span>·</span>
          <span>{server.players}/{server.maxPlayers} players</span>
          <span>·</span>
          <span>:{server.port}</span>
          {server.status === 'running' && (
            <>
              <span>·</span>
              <span>{formatUptime(server.startedAt)} up</span>
            </>
          )}
        </div>
        {server.status === 'running' && (
          <div className="server-row-history">
            <Sparkline data={cpuHistory} color="var(--accent)" height={32} label={`cpu-${server.id}`} />
          </div>
        )}
      </div>
      <div className="server-row-meta">
        {server.status === 'running' && (
          <div className="server-row-stats">
            <div className="sstat">
              <span className="sstat-l">CPU</span>
              <span className="sstat-v">{server.cpu}%</span>
            </div>
            <div className="sstat">
              <span className="sstat-l">Mem</span>
              <span className="sstat-v">{formatMegabytes(server.memory)}</span>
            </div>
          </div>
        )}
        <div className="server-row-actions" onClick={(e) => e.stopPropagation()}>
          {(server.status === 'stopped' || server.status === 'crashed') && (
            <button className="btn btn-sm btn-primary" disabled={busy} onClick={() => store.startServer(server.id)}>
              Start
            </button>
          )}
          {server.status === 'running' && (
            <>
              <button className="btn btn-sm btn-ghost" disabled={busy} onClick={() => store.restartServer(server.id)}>
                Restart
              </button>
              <button className="btn btn-sm btn-ghost" disabled={busy} onClick={() => store.stopServer(server.id)}>
                Stop
              </button>
            </>
          )}
          {busy && <Spinner size={14} />}
        </div>
      </div>
    </div>
  );
}
