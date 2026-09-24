import type { CSSProperties, ReactNode } from 'react';
import type { NavView, Server } from '../types';
import { useStore } from '../state/store';
import {
  IconGrid,
  IconServer,
  IconBox,
  IconCloud,
  IconSettings,
  IconPlus,
  IconSearch,
  NookiLogo,
} from './Icons';
import ServerIcon from './ServerIcon';
import { toggleMaximize } from './WindowControls';
import { formatMegabytes, statusLabels } from '../format';
import './Sidebar.css';

interface SidebarProps {
  currentView: NavView;
  onNavigate: (view: NavView) => void;
  onOpenCommand: () => void;
}

const dotClass: Record<Server['status'], string> = {
  running: 'is-running',
  stopped: '',
  crashed: 'is-crashed',
  starting: 'is-warning',
  stopping: 'is-warning',
  restarting: 'is-warning',
  updating: 'is-updating',
};

export default function Sidebar({ currentView, onNavigate, onOpenCommand }: SidebarProps) {
  const store = useStore();
  const { host, servers } = store;

  const navItems: { id: NavView; label: string; icon: ReactNode }[] = [
    { id: 'dashboard', label: 'Dashboard', icon: <IconGrid size={15} /> },
    { id: 'servers', label: 'Servers', icon: <IconServer size={15} /> },
    { id: 'backups', label: 'Backups', icon: <IconBox size={15} /> },
    { id: 'quick-server', label: 'Quick server', icon: <IconCloud size={15} /> },
  ];

  const memPct = host.totalMemory > 0 ? (host.usedMemory / host.totalMemory) * 100 : 0;
  const diskPct = host.diskTotal > 0 ? (host.diskUsed / host.diskTotal) * 100 : 0;
  const serversListActive = currentView === 'servers' && !store.openServerId;

  return (
    <aside className="sidebar">
      <div className="sidebar-head" data-tauri-drag-region onDoubleClick={toggleMaximize}>
        <span className="sidebar-brand">
          <NookiLogo size={20} />
          <span>Nooki</span>
        </span>
        <button className="sidebar-icon-btn" onClick={() => store.setWizardOpen(true)} aria-label="Add server" title="Add server">
          <IconPlus size={15} />
        </button>
      </div>

      <button className="sidebar-search" onClick={onOpenCommand}>
        <IconSearch size={14} />
        <span>Search</span>
        <span className="sidebar-search-keys"><kbd className="kbd">Ctrl</kbd><kbd className="kbd">K</kbd></span>
      </button>

      <nav className="sidebar-nav" aria-label="Main">
        {navItems.map((item) => {
          const active = item.id === 'servers' ? serversListActive : currentView === item.id;
          return (
            <button
              key={item.id}
              className={`nav-item ${active ? 'active' : ''}`}
              onClick={() => item.id === 'servers' ? (store.setNav('servers'), store.closeServer()) : onNavigate(item.id)}
              aria-current={active ? 'page' : undefined}
            >
              <span className="nav-icon">{item.icon}</span>
              <span className="nav-label">{item.label}</span>
            </button>
          );
        })}

        {servers.length > 0 && (
          <div className="sidebar-section">
            <div className="sidebar-section-head">
              <span>Your servers</span>
              <span className="sidebar-section-count">{servers.length}</span>
            </div>
            {servers.map((server) => {
              const active = currentView === 'servers' && store.openServerId === server.id;
              return (
                <button
                  key={server.id}
                  className={`nav-item nav-server ${active ? 'active' : ''}`}
                  onClick={() => store.openServer(server.id)}
                  aria-current={active ? 'page' : undefined}
                  title={`${server.name} — ${statusLabels[server.status]}`}
                >
                  <span className="nav-server-icon">
                    <ServerIcon server={server} size={18} />
                    <span className={`status-dot ${dotClass[server.status]}`} aria-label={statusLabels[server.status]} />
                  </span>
                  <span className="nav-label">{server.name}</span>
                  {server.status === 'running' && server.players > 0 && (
                    <span className="nav-meta">{server.players}</span>
                  )}
                </button>
              );
            })}
          </div>
        )}
      </nav>

      <div className="sidebar-foot">
        <button
          className={`nav-item ${currentView === 'settings' ? 'active' : ''}`}
          onClick={() => onNavigate('settings')}
          aria-current={currentView === 'settings' ? 'page' : undefined}
        >
          <span className="nav-icon"><IconSettings size={15} /></span>
          <span className="nav-label">Settings</span>
        </button>

        <div className="host" aria-label="This computer">
          <HostMeter label="CPU" pct={host.cpu} value={`${Math.round(host.cpu)}%`} />
          <HostMeter label="Memory" pct={memPct} value={`${formatMegabytes(host.usedMemory)} / ${formatMegabytes(host.totalMemory)}`} />
          <HostMeter label="Disk" pct={diskPct} value={`${formatMegabytes(host.diskTotal - host.diskUsed)} free`} warn={diskPct > 90} />
        </div>
      </div>
    </aside>
  );
}

function HostMeter({ label, pct, value, warn }: { label: string; pct: number; value: string; warn?: boolean }) {
  const clamped = Math.max(0, Math.min(100, pct));
  return (
    <div className={`host-meter ${warn || clamped > 90 ? 'is-hot' : ''}`}>
      <span className="host-label">{label}</span>
      <span className="host-value">{value}</span>
      <span className="host-bar"><span className="host-bar-fill" style={{ '--pct': clamped / 100 } as CSSProperties} /></span>
    </div>
  );
}
