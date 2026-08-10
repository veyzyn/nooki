import { useEffect, useMemo, useState } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { save } from '@tauri-apps/plugin-dialog';
import { useStore } from '../../state/store';
import type { LogLevel, LogSession, Server } from '../../types';
import { EmptyState, Modal, Segmented } from '../../components/ui';
import { IconFileText, IconSearch, IconX, IconCopy, IconDownload } from '../../components/Icons';
import { formatBytes, formatClock, formatDateOnly, formatDuration } from '../../format';
import './LogsTab.css';

const outcomeLabels: Record<LogSession['outcome'], string> = {
  running: 'Still running',
  'clean-stop': 'Stopped normally',
  crashed: 'Crashed',
};

const outcomeTone: Record<LogSession['outcome'], string> = {
  running: 'running',
  'clean-stop': 'stopped',
  crashed: 'crashed',
};

export default function LogsTab({ server }: { server: Server }) {
  const store = useStore();
  const [query, setQuery] = useState('');
  const [level, setLevel] = useState<'all' | LogLevel>('all');
  const [openSession, setOpenSession] = useState<LogSession | null>(null);
  const [viewerSource, setViewerSource] = useState<LogSession | null>(null);

  const sessions = useMemo(
    () => store.logSessions.filter((s) => s.serverId === server.id).sort((a, b) => b.startedAt - a.startedAt),
    [server.id, store.logSessions],
  );

  useEffect(() => { void store.refreshLogs(server.id); }, [server.id, store.refreshLogs]);

  /* The live console doubles as the newest session's contents. */
  const liveLines = store.consoleLines[server.id] ?? [];

  const viewerLines = useMemo(() => {
    if (!openSession) return [];
    const base = openSession.outcome === 'running' ? liveLines : (viewerSource?.lines ?? []);
    return base.filter((l) => {
      if (level !== 'all' && l.level !== level) return false;
      if (query && !l.text.toLowerCase().includes(query.toLowerCase())) return false;
      return true;
    });
  }, [openSession, viewerSource, liveLines, level, query]);

  const openLog = async (session: LogSession) => {
    setOpenSession(session);
    if (session.outcome === 'running') return;
    try {
      const lines = await store.readLog(session.id);
      setViewerSource({ ...session, lines });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Could not read this log', detail: String((error as { message?: string })?.message ?? error) });
    }
  };

  const filteredSessions = sessions.filter((s) => {
    if (!query) return true;
    return (
      formatDateOnly(s.startedAt).toLowerCase().includes(query.toLowerCase()) ||
      outcomeLabels[s.outcome].toLowerCase().includes(query.toLowerCase())
    );
  });

  return (
    <div className="tab logs-tab">
      <div className="logs-toolbar">
        <div className="console-search">
          <IconSearch size={14} />
          <input
            className="console-search-input"
            placeholder="Search log sessions"
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
          value={level}
          onChange={setLevel}
          options={[
            { value: 'all', label: 'All levels' },
            { value: 'warn', label: 'Warnings' },
            { value: 'error', label: 'Errors' },
          ]}
        />
      </div>

      {filteredSessions.length === 0 ? (
        <div className="logs-panel">
          <EmptyState
            icon={<IconFileText size={40} />}
            title={query ? 'No sessions match your search' : 'No logs yet'}
            description={
              query
                ? 'Try a different date or outcome.'
                : 'Each time this server runs, Nooki keeps the log so you can look back at what happened.'
            }
          />
        </div>
      ) : (
        <div className="logs-panel">
          <div className="logs-head-row">
            <span>Session</span>
            <span>Ran for</span>
            <span>Size</span>
            <span>Outcome</span>
            <span />
          </div>
          {filteredSessions.map((session) => (
            <div key={session.id} className="logs-row">
              <div className="logs-when">
                <span className="logs-date">{formatDateOnly(session.startedAt)}</span>
                <span className="logs-time">{formatClock(session.startedAt)}</span>
              </div>
              <span className="logs-cell">{formatDuration(session.duration)}</span>
              <span className="logs-cell mono">{formatBytes(session.size)}</span>
              <span className={`status-badge status-${outcomeTone[session.outcome]}`}>
                {outcomeLabels[session.outcome]}
              </span>
              <div className="logs-actions">
                <button className="btn btn-sm btn-secondary" onClick={() => void openLog(session)}>
                  Open
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      <Modal
        open={openSession !== null}
        onClose={() => setOpenSession(null)}
        title={openSession ? `Log from ${formatDateOnly(openSession.startedAt)}` : ''}
        description={
          openSession
            ? `${formatClock(openSession.startedAt)} · ran for ${formatDuration(openSession.duration)} · ${outcomeLabels[openSession.outcome]}`
            : undefined
        }
        width={760}
        footer={
          <>
            <button
              className="btn btn-secondary"
              onClick={() => void writeText(viewerLines.map((line) => `[${formatClock(line.at)}] [${line.source}] ${line.text}`).join('\n')).then(() => store.pushToast({ tone: 'success', title: 'Log copied to the clipboard' }))}
            >
              <IconCopy size={13} />
              Copy
            </button>
            <button
              className="btn btn-secondary"
              onClick={() => void (async () => {
                if (!openSession) return;
                const destination = await save({ defaultPath: `nooki-${server.name}-${openSession.startedAt}.log`, filters: [{ name: 'Log', extensions: ['log', 'txt'] }] });
                if (!destination) return;
                await store.exportLog(openSession.id, destination);
                store.pushToast({ tone: 'success', title: 'Log exported', detail: destination });
              })().catch((error) => store.pushToast({ tone: 'error', title: 'Could not export log', detail: String((error as { message?: string })?.message ?? error) }))}
            >
              <IconDownload size={13} />
              Export
            </button>
            <div style={{ flex: 1 }} />
            <button className="btn btn-primary" onClick={() => setOpenSession(null)}>
              Close
            </button>
          </>
        }
      >
        <div className="log-viewer">
          {viewerLines.length === 0 ? (
            <EmptyState title="Nothing to show" description="No lines match the current filters." />
          ) : (
            viewerLines.map((line) => (
              <div key={line.id} className={`log-line level-${line.level}`}>
                <span className="log-time">{formatClock(line.at)}</span>
                <span className="log-source">{line.source}</span>
                <span className="log-text">{line.text}</span>
              </div>
            ))
          )}
        </div>
      </Modal>
    </div>
  );
}
