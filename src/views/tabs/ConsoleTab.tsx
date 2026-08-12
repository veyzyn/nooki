import { memo, useEffect, useMemo, useRef, useState } from 'react';
import { useConsoleLines, useStore } from '../../state/store';
import type { Server } from '../../types';
import { IconSearch, IconX, IconChevronDown } from '../../components/Icons';
import { Segmented, EmptyState } from '../../components/ui';
import { formatClock } from '../../format';
import type { LogLevel } from '../../types';
import './ConsoleTab.css';

type LevelFilter = 'all' | LogLevel;
const MAX_RENDERED_LINES = 600;

const levelColors: Record<LogLevel, string> = {
  info: 'var(--text-secondary)',
  warn: 'var(--st-warning)',
  error: 'var(--st-crashed)',
};

const ConsoleLine = memo(function ConsoleLine({ line }: { line: { id: string; at: number; level: LogLevel; source: string; text: string } }) {
  return (
    <div className={`log-line level-${line.level}`}>
      <span className="log-time">{formatClock(line.at)}</span>
      <span className="log-source">{line.source}</span>
      <span className="log-text" style={{ color: levelColors[line.level] }}>{line.text}</span>
    </div>
  );
});

export default function ConsoleTab({ server }: { server: Server }) {
  const store = useStore();
  const lines = useConsoleLines(server.id);
  const [command, setCommand] = useState('');
  const [search, setSearch] = useState('');
  const [levelFilter, setLevelFilter] = useState<LevelFilter>('all');
  const [paused, setPaused] = useState(false);
  const [historyIdx, setHistoryIdx] = useState(-1);
  const [cmdHistory, setCmdHistory] = useState<string[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const running = server.status === 'running';

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return lines.filter((line) => {
      if (levelFilter !== 'all' && line.level !== levelFilter) return false;
      if (query && !line.text.toLowerCase().includes(query) && !line.source.toLowerCase().includes(query)) return false;
      return true;
    });
  }, [lines, levelFilter, search]);
  const renderedLines = useMemo(() => filtered.slice(-MAX_RENDERED_LINES), [filtered]);

  useEffect(() => {
    if (paused || !scrollRef.current) return;
    const frame = window.requestAnimationFrame(() => {
      if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [renderedLines, paused]);

  const sendCommand = () => {
    const cmd = command.trim();
    if (!cmd || !running) return;
    store.sendCommand(server.id, cmd);
    setCmdHistory((prev) => [cmd, ...prev.slice(0, 49)]);
    setCommand('');
    setHistoryIdx(-1);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      sendCommand();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const next = Math.min(historyIdx + 1, cmdHistory.length - 1);
      setHistoryIdx(next);
      if (cmdHistory[next] !== undefined) setCommand(cmdHistory[next]);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      const next = Math.max(historyIdx - 1, -1);
      setHistoryIdx(next);
      setCommand(next === -1 ? '' : (cmdHistory[next] ?? ''));
    }
  };

  return (
    <div className="tab console-tab">
      <div className="console-toolbar">
        <div className="console-search">
          <IconSearch size={14} />
          <input
            className="console-search-input"
            placeholder="Filter logs"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {search && (
            <button className="icon-btn" onClick={() => setSearch('')} aria-label="Clear search">
              <IconX size={12} />
            </button>
          )}
        </div>

        <Segmented
          value={levelFilter}
          onChange={setLevelFilter}
          options={[
            { value: 'all', label: 'All' },
            { value: 'warn', label: 'Warn' },
            { value: 'error', label: 'Error' },
          ]}
        />

        <button
          className={`btn btn-sm btn-ghost ${paused ? 'is-paused' : ''}`}
          onClick={() => setPaused((v) => !v)}
          title={paused ? 'Resume scrolling' : 'Pause scrolling'}
        >
          {paused ? 'Resume' : 'Pause'}
        </button>

        <button
          className="btn btn-sm btn-ghost"
          onClick={() => store.clearConsole(server.id)}
          title="Clear the visible log (does not affect saved logs)"
        >
          Clear view
        </button>
      </div>

      <div
        className="console-output"
        ref={scrollRef}
        onClick={() => inputRef.current?.focus()}
      >
        {filtered.length === 0 ? (
          <EmptyState
            title={search || levelFilter !== 'all' ? 'No matching lines' : 'Console is clear'}
            description={search ? `Nothing matched "${search}". Try a different filter.` : 'Lines will appear here when the server is running.'}
          />
        ) : (
          renderedLines.map((line) => <ConsoleLine key={line.id} line={line} />)
        )}
      </div>

      <div className="console-input-row">
        <span className="console-prompt">/</span>
        <input
          ref={inputRef}
          className="console-input"
          placeholder={running ? 'Type a command and press Enter' : 'Server is not running'}
          disabled={!running}
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          onKeyDown={onKeyDown}
          spellCheck={false}
          autoComplete="off"
        />
        {cmdHistory.length > 0 && (
          <button
            className="btn btn-sm btn-ghost history-btn"
            title="Previous command"
            onClick={() => {
              const next = Math.min(historyIdx + 1, cmdHistory.length - 1);
              setHistoryIdx(next);
              setCommand(cmdHistory[next] ?? '');
              inputRef.current?.focus();
            }}
          >
            <IconChevronDown size={13} />
          </button>
        )}
        <button className="btn btn-sm btn-primary" disabled={!running || !command.trim()} onClick={sendCommand}>
          Send
        </button>
      </div>
    </div>
  );
}
