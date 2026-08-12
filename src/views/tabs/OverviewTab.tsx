import { useEffect, useState } from 'react';
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from 'recharts';
import { useStore } from '../../state/store';
import type { Server } from '../../types';
import { Callout, Meter, Avatar, EmptyState } from '../../components/ui';
import { ChartContainer, ChartTooltip, ChartTooltipContent, type ChartConfig } from '../../components/ui/chart';
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
  const chartEnd = running ? now : server.history[server.history.length - 1]?.at ?? now;
  const sessionStart = server.startedAt ?? server.history[0]?.at ?? chartEnd;
  const chartStart = Math.max(sessionStart, chartEnd - 3_600_000);
  const loadHistory = server.history
    .filter((sample) => sample.at >= chartStart)
    .map((sample) => ({ at: sample.at, cpu: sample.cpu, memory: sample.memory }));
  if (running) {
    if (loadHistory.length === 0 && server.startedAt && server.startedAt < now) {
      loadHistory.push({ at: server.startedAt, cpu: server.cpu, memory: memPct });
    }
    loadHistory.push({ at: now, cpu: server.cpu, memory: memPct });
  }
  const chartDomain: [number, number] = chartStart === chartEnd ? [chartStart - 1_000, chartEnd] : [chartStart, chartEnd];

  return (
    <div className="tab overview-tab">
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
          <Fact
            label="Public address"
            value={server.sharing.address ?? 'None'}
            mono
            wide
          />
          <Fact label="Local address" value={`localhost:${server.port}`} mono />
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

      <div className="tab-section ov-load-section">
        <h3 className="tab-section-title">Recent load</h3>
        <div className="ov-chart">
          <div className="ov-chart-head">
            <span>Processor and memory</span>
            <div className="ov-chart-values" aria-label="Current resource usage">
              <span><i className="is-cpu" />CPU <strong>{running ? `${Math.round(server.cpu)}%` : '—'}</strong></span>
              <span><i className="is-memory" />RAM <strong>{running ? `${Math.round(memPct)}%` : '—'}</strong></span>
            </div>
          </div>
          <ChartContainer config={loadChartConfig} className="ov-load-chart">
            <LineChart accessibilityLayer data={loadHistory} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
              <CartesianGrid vertical={false} strokeDasharray="3 3" />
              <XAxis
                dataKey="at"
                type="number"
                scale="time"
                domain={chartDomain}
                tickLine={false}
                axisLine={false}
                tickMargin={9}
                minTickGap={42}
                tickFormatter={formatChartTick}
              />
              <YAxis
                domain={[0, 100]}
                ticks={[0, 25, 50, 75, 100]}
                tickLine={false}
                axisLine={false}
                width={34}
                tickFormatter={(value) => `${value}%`}
              />
              <ChartTooltip
                cursor={{ stroke: 'var(--border-strong)', strokeDasharray: '3 3' }}
                content={
                  <ChartTooltipContent
                    labelFormatter={(_, payload) => formatChartTooltipTime(Number(payload?.[0]?.payload?.at ?? now))}
                    formatter={(value, name, item) => (
                      <div className="ov-chart-tooltip-row">
                        <i style={{ background: item.color }} />
                        <span>{loadChartConfig[name === 'cpu' ? 'cpu' : 'memory'].label}</span>
                        <strong>{Math.round(Number(value))}%</strong>
                      </div>
                    )}
                  />
                }
              />
              <Line dataKey="cpu" type="monotone" stroke="var(--color-cpu)" strokeWidth={2} dot={false} activeDot={{ r: 3 }} isAnimationActive={false} />
              <Line dataKey="memory" type="monotone" stroke="var(--color-memory)" strokeWidth={2} dot={false} activeDot={{ r: 3 }} isAnimationActive={false} />
            </LineChart>
          </ChartContainer>
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
                    ? 'Share the address above and players will show up here as they join.'
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

const loadChartConfig = {
  cpu: { label: 'Processor', color: 'var(--st-running)' },
  memory: { label: 'Memory', color: 'var(--st-updating)' },
} satisfies ChartConfig;

function formatChartTick(value: number): string {
  return new Date(value).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

function formatChartTooltipTime(value: number): string {
  return new Date(value).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
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

function Fact({ label, value, mono, wide }: { label: string; value: string; mono?: boolean; wide?: boolean }) {
  return (
    <div className={`fact ${wide ? 'fact-wide' : ''}`}>
      <span className="fact-label">{label}</span>
      <span className={`fact-value ${mono ? 'mono' : ''}`} title={value}>{value}</span>
    </div>
  );
}
