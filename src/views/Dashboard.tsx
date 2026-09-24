import { useEffect, useState, type ReactNode } from 'react';
import { useStore } from '../state/store';
import type { Server } from '../types';
import { IconChevronRight, IconPlay, IconPlus } from '../components/Icons';
import Timeline from '../components/Timeline';
import ServerIcon from '../components/ServerIcon';
import { PageActions } from '../components/TopBar';
import { formatUptime, formatRelative, formatUntil, formatMegabytes, statusLabels, statusTone, isBusy, softwareLabel } from '../format';
import { Callout, EmptyState, Sparkline, Spinner } from '../components/ui';
import './Dashboard.css';

// The card grid staggers in once per session (first paint after launch). After
// that the dashboard is a place people return to often, so it just appears.
let dashboardHasEntered = false;

export default function Dashboard() {
  const store = useStore();
  const { servers, activity, backups } = store;
  const [stagger] = useState(() => !dashboardHasEntered);
  useEffect(() => { dashboardHasEntered = true; }, []);

  const runningCount = servers.filter((s) => s.status === 'running').length;
  const totalPlayers = servers.reduce((sum, s) => sum + (s.status === 'running' ? s.players : 0), 0);
  const totalMemory = servers.reduce((sum, s) => sum + (s.status === 'running' ? s.memory : 0), 0);
  const allocatedMemory = servers.filter((s) => s.status === 'running').reduce((sum, s) => sum + s.maxMemory, 0);
  const crashed = servers.filter((s) => s.status === 'crashed');
  const lastBackup = backups.find((backup) => !backup.failed);
  const nextSchedule = Object.values(store.schedules)
    .filter((schedule) => schedule.enabled && schedule.nextRunAt)
    .sort((a, b) => (a.nextRunAt ?? 0) - (b.nextRunAt ?? 0))[0];

  return (
    <div className="view dashboard">
      <PageActions>
        <button className="btn btn-primary btn-sm" onClick={() => store.setWizardOpen(true)}>
          <IconPlus size={14} />
          Add server
        </button>
      </PageActions>

      <div className="page">
        <div className="page-inner">
          <section className="dash-stats" aria-label="Summary">
            <Stat label="Servers running" value={runningCount} suffix={`/ ${servers.length}`} live={runningCount > 0} />
            <Stat label="Players online" value={totalPlayers} />
            <Stat label="Memory in use" value={formatMegabytes(totalMemory)} suffix={allocatedMemory > 0 ? `/ ${formatMegabytes(allocatedMemory)}` : undefined} />
            <Stat
              label="Last backup"
              value={lastBackup ? formatRelative(lastBackup.createdAt) : 'Never'}
              suffix={nextSchedule?.nextRunAt ? `next ${formatUntil(nextSchedule.nextRunAt)}` : undefined}
            />
          </section>

          {crashed.length > 0 && (
            <div className="dash-alerts">
              {crashed.map((s) => (
                <Callout key={s.id} tone="error" title={`${s.name} stopped unexpectedly`} action={
                  <>
                    <button className="btn btn-sm btn-ghost" onClick={() => store.openServer(s.id, 'logs')}>View log</button>
                    <button className="btn btn-sm btn-secondary" onClick={() => store.startServer(s.id)}>Start</button>
                  </>
                }>
                  {s.lastExit ?? 'No crash details available. Check the console log for more information.'}
                </Callout>
              ))}
            </div>
          )}

          <div className="dash-columns">
            <section className="dash-section" aria-labelledby="dash-servers">
              <header className="section-head">
                <h2 id="dash-servers" className="section-title">Servers</h2>
                {servers.length > 0 && (
                  <button className="section-link" onClick={() => { store.setNav('servers'); store.closeServer(); }}>
                    View all <IconChevronRight size={13} />
                  </button>
                )}
              </header>
              {servers.length === 0 ? (
                <div className="dash-empty">
                  <EmptyState
                    title="No servers yet"
                    description="Create a fresh Minecraft server, install a modpack, or bring in one you already run."
                    action={<button className="btn btn-primary" onClick={() => store.setWizardOpen(true)}><IconPlus size={14} /> Add server</button>}
                  />
                </div>
              ) : (
                <div className={`server-grid ${stagger ? 'stagger' : ''}`}>
                  {servers.map((server) => <ServerCard key={server.id} server={server} />)}
                </div>
              )}
            </section>

            <section className="dash-section dash-activity" aria-labelledby="dash-activity">
              <header className="section-head">
                <h2 id="dash-activity" className="section-title">Activity</h2>
              </header>
              {activity.length === 0 ? (
                <p className="activity-empty">Starts, stops, backups, and updates will show up here.</p>
              ) : (
                <Timeline events={activity.slice(0, 9)} showServer />
              )}
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}

function Stat({ label, value, suffix, live }: { label: string; value: ReactNode; suffix?: string; live?: boolean }) {
  return (
    <div className="dash-stat">
      <span className="dash-stat-label">
        {live && <span className="status-dot is-running" aria-hidden="true" />}
        {label}
      </span>
      <span className="dash-stat-value">
        {value}
        {suffix && <span className="dash-stat-suffix">{suffix}</span>}
      </span>
    </div>
  );
}

function ServerCard({ server }: { server: Server }) {
  const store = useStore();
  const busy = isBusy(server.status);
  const running = server.status === 'running';
  const cpuHistory = [...server.history.map((sample) => sample.cpu).slice(-60, -1), server.cpu];

  return (
    <article
      className={`server-card ${server.status === 'crashed' ? 'is-crashed' : ''}`}
      onClick={() => store.openServer(server.id)}
      onKeyDown={(event) => { if (event.key === 'Enter') store.openServer(server.id); }}
      tabIndex={0}
      aria-label={`${server.name}, ${statusLabels[server.status]}`}
    >
      <div className="server-card-head">
        <ServerIcon server={server} size={36} />
        <div className="server-card-titles">
          <span className="server-card-name" title={server.name}>{server.name}</span>
          <span className="server-card-sub">{softwareLabel(server.type)} {server.version} · <span className="mono">:{server.port}</span></span>
        </div>
        <span className={`status-badge status-${statusTone(server.status)}`}>
          {busy && <Spinner size={10} />}
          {statusLabels[server.status]}
        </span>
      </div>

      <div className="server-card-chart">
        {running
          ? <Sparkline data={cpuHistory} color="var(--st-running)" height={44} label={`Processor history for ${server.name}`} maxValue={100} />
          : <span className="server-card-idle">{server.status === 'crashed' ? 'Stopped unexpectedly' : busy ? `${statusLabels[server.status]}…` : 'Not running'}</span>}
      </div>

      <div className="server-card-foot">
        <dl className="server-card-stats">
          <div><dt>CPU</dt><dd>{running ? `${Math.round(server.cpu)}%` : '—'}</dd></div>
          <div><dt>Memory</dt><dd>{running ? formatMegabytes(server.memory) : '—'}</dd></div>
          <div><dt>Players</dt><dd>{server.players}/{server.maxPlayers}</dd></div>
          {running && <div className="server-card-uptime"><dt>Up</dt><dd>{formatUptime(server.startedAt)}</dd></div>}
        </dl>
        {(server.status === 'stopped' || server.status === 'crashed') && (
          <button className="btn btn-sm btn-secondary server-card-action" onClick={(event) => { event.stopPropagation(); store.startServer(server.id); }}>
            <IconPlay size={12} /> Start
          </button>
        )}
      </div>
    </article>
  );
}
