import { useEffect, useState } from 'react';
import { useStore } from '../../state/store';
import type { Server } from '../../types';
import { Callout, Meter, MetricChart, Avatar, EmptyState, chartDomainMax } from '../../components/ui';
import { IconUsers } from '../../components/Icons';
import { formatMegabytes, formatRelative, formatUptime, softwareLabel, statusLabels } from '../../format';
import './OverviewTab.css';

export default function OverviewTab({ server }: { server: Server }) {
  const store = useStore();
  const online = store.players.filter((p) => p.serverId === server.id);
  const events = store.activity.filter((e) => e.serverId === server.id).slice(0, 6);
  const running = server.status === 'running';
  const visibleAlerts = server.alerts.filter((alert) => (
    alert.kind !== 'stop-timeout' || server.status === 'stopping' || server.status === 'restarting'
  ));
  const now = useChartClock(running);

  const memPct = (server.memory / server.maxMemory) * 100;
  const memTone = memPct > 88 ? 'danger' : memPct > 70 ? 'warning' : 'accent';
  const cpuTone = server.cpu > 80 ? 'danger' : server.cpu > 60 ? 'warning' : 'accent';
  const cpuHistory = server.history.map((sample) => ({ at: sample.at, value: sample.cpu }));
  const memoryHistory = server.history.map((sample) => ({ at: sample.at, value: sample.memory }));
  if (running) {
    cpuHistory.push({ at: now, value: server.cpu });
    memoryHistory.push({ at: now, value: memPct });
  }
  const cpuChartMax = chartDomainMax(cpuHistory.map((sample) => sample.value), 10);
  const memoryChartMax = chartDomainMax(memoryHistory.map((sample) => sample.value), 25);
  const chartStart = server.startedAt ?? server.history[0]?.at ?? now;
  const chartEnd = running ? now : server.history[server.history.length - 1]?.at ?? now;

  return (
    <div className="tab">
      {visibleAlerts.length > 0 && (
        <div className="tab-section">
          {visibleAlerts.map((alert) => (
            <Callout
              key={alert.id}
              tone={alert.severity}
              title={alert.title}
              onDismiss={() => store.dismissAlert(server.id, alert.id)}
              action={
                alert.kind === 'restart-required' ? (
                  <button
                    className="btn btn-sm btn-secondary"
                    disabled={!running}
                    onClick={() => store.restartServer(server.id)}
                  >
                    Restart now
                  </button>
                ) : alert.kind === 'port-conflict' ? (
                  <button className="btn btn-sm btn-secondary" onClick={() => store.setServerTab('settings')}>
                    Change port
                  </button>
                ) : alert.kind === 'crash' ? (
                  <button className="btn btn-sm btn-secondary" onClick={() => store.setServerTab('logs')}>
                    See the log
                  </button>
                ) : alert.kind === 'stop-timeout' ? (
                  <button className="btn btn-sm btn-danger" onClick={() => store.forceStopServer(server.id)}>
                    Force stop
                  </button>
                ) : undefined
              }
            >
              {alert.detail}
            </Callout>
          ))}
        </div>
      )}

      <div className="ov-grid">
        <div className="ov-facts">
          <Fact label="Status" value={statusLabels[server.status]} />
          <Fact label="Uptime" value={running ? formatUptime(server.startedAt) : '—'} />
          <Fact label="Software" value={`${softwareLabel(server.type)} ${server.version}`} />
          <Fact label="Build" value={server.build} />
          <Fact label="Address" value={`localhost:${server.port}`} mono />
          <Fact label="Players" value={`${server.players} of ${server.maxPlayers}`} />
        </div>

        <div className="ov-resources">
          <Meter
            label="Processor"
            value={server.cpu}
            max={100}
            display={running ? `${server.cpu}%` : 'idle'}
            tone={cpuTone}
          />
          <Meter
            label="Memory"
            value={server.memory}
            max={server.maxMemory}
            display={running ? `${formatMegabytes(server.memory)} of ${formatMegabytes(server.maxMemory)}` : 'idle'}
            tone={memTone}
          />
          <Meter
            label="Disk"
            value={server.diskUsed}
            max={20 * 1024}
            display={formatMegabytes(server.diskUsed)}
            tone="info"
          />
        </div>
      </div>

      <div className="tab-section">
        <h3 className="tab-section-title">Recent load</h3>
        <div className="ov-charts">
          <div className="ov-chart">
            <div className="ov-chart-head">
              <span>Processor</span>
              <span className="mono">{running ? `${server.cpu}%` : '—'}</span>
            </div>
            <MetricChart data={cpuHistory} color="var(--accent)" label="Processor" maxValue={cpuChartMax} startAt={chartStart} endAt={chartEnd} />
          </div>
          <div className="ov-chart">
            <div className="ov-chart-head">
              <span>Memory</span>
              <span className="mono">{running ? `${Math.round(memPct)}%` : '—'}</span>
            </div>
            <MetricChart data={memoryHistory} color="var(--st-updating)" label="Memory" maxValue={memoryChartMax} startAt={chartStart} endAt={chartEnd} />
          </div>
        </div>
      </div>

      <div className="ov-lower">
        <div className="tab-section">
          <h3 className="tab-section-title">Who is online</h3>
          <div className="ov-panel">
            {online.length === 0 ? (
              <EmptyState
                icon={<IconUsers size={40} />}
                title={running ? 'Nobody is playing right now' : 'Server is not running'}
                description={
                  running
                    ? 'Players will show up here as they join.'
                    : 'Start the server to let players connect.'
                }
              />
            ) : (
              <ul className="ov-players">
                {online.map((p) => (
                  <li key={p.id} className="ov-player">
                    <Avatar name={p.username} color={p.avatar} size={30} />
                    <div className="ov-player-body">
                      <span className="ov-player-name">
                        {p.username}
                        {p.isOp && <span className="op-tag">operator</span>}
                      </span>
                      <span className="ov-player-meta">Joined {formatRelative(p.connectedAt)}</span>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>

        <div className="tab-section">
          <h3 className="tab-section-title">What happened recently</h3>
          <div className="ov-panel">
            {events.length === 0 ? (
              <EmptyState title="Nothing yet" description="Starts, stops, backups, and updates will show up here." />
            ) : (
              <ul className="ov-events">
                {events.map((e) => (
                  <li key={e.id} className="ov-event">
                    <span className={`ov-event-dot dot-${e.kind}`} />
                    <span className="ov-event-msg">{e.message}</span>
                    <span className="ov-event-time">{formatRelative(e.at)}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function useChartClock(active: boolean) {
  const [now, setNow] = useState(Date.now);

  useEffect(() => {
    setNow(Date.now());
    if (!active) return undefined;
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [active]);

  return now;
}

function Fact({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="fact">
      <span className="fact-label">{label}</span>
      <span className={`fact-value ${mono ? 'mono' : ''}`}>{value}</span>
    </div>
  );
}
