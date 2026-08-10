import type { ServerStatus } from './types';

const MIN = 60 * 1000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

export function formatUptime(startedAt: number | null, nowTs = Date.now()): string {
  if (!startedAt) return '—';
  const ms = Math.max(0, nowTs - startedAt);
  const days = Math.floor(ms / DAY);
  const hours = Math.floor((ms % DAY) / HOUR);
  const mins = Math.floor((ms % HOUR) / MIN);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m`;
  return 'less than a minute';
}

export function formatDuration(ms: number): string {
  const hours = Math.floor(ms / HOUR);
  const mins = Math.floor((ms % HOUR) / MIN);
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m`;
  return `${Math.round(ms / 1000)}s`;
}

export function formatBytes(bytes: number): string {
  if (bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const value = bytes / 1024 ** i;
  const digits = value < 10 && i > 1 ? 2 : value < 100 ? 1 : 0;
  return `${value.toFixed(digits)} ${units[i]}`;
}

export function formatMegabytes(mb: number): string {
  if (mb >= 1024) {
    const gb = mb / 1024;
    return `${gb.toFixed(gb < 10 ? 1 : 0)} GB`;
  }
  return `${Math.round(mb)} MB`;
}

export function formatRelative(ts: number, nowTs = Date.now()): string {
  const diff = nowTs - ts;
  if (diff < MIN) return 'just now';
  if (diff < HOUR) return `${Math.floor(diff / MIN)} min ago`;
  if (diff < DAY) {
    const h = Math.floor(diff / HOUR);
    return h === 1 ? '1 hour ago' : `${h} hours ago`;
  }
  const d = Math.floor(diff / DAY);
  if (d === 1) return 'yesterday';
  if (d < 7) return `${d} days ago`;
  return new Date(ts).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

export function formatClock(ts: number): string {
  return new Date(ts).toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

export function formatDateTime(ts: number): string {
  return new Date(ts).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

export function formatDateOnly(ts: number): string {
  return new Date(ts).toLocaleDateString(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
  });
}

export const statusLabels: Record<ServerStatus, string> = {
  running: 'Running',
  stopped: 'Stopped',
  crashed: 'Crashed',
  starting: 'Starting',
  stopping: 'Stopping',
  restarting: 'Restarting',
  updating: 'Updating',
};

export function statusTone(status: ServerStatus): string {
  switch (status) {
    case 'running':
      return 'running';
    case 'stopped':
      return 'stopped';
    case 'crashed':
      return 'crashed';
    case 'updating':
      return 'updating';
    default:
      return 'warning';
  }
}

export function isBusy(status: ServerStatus): boolean {
  return status === 'starting' || status === 'stopping' || status === 'restarting' || status === 'updating';
}

export function softwareLabel(type: 'vanilla' | 'paper' | 'forge' | 'neoforge' | 'fabric'): string {
  return {
    vanilla: 'Vanilla',
    paper: 'Paper',
      forge: 'Forge',
      neoforge: 'NeoForge',
    fabric: 'Fabric',
  }[type];
}

export function uid(prefix: string): string {
  return `${prefix}-${Math.random().toString(36).slice(2, 9)}`;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}
