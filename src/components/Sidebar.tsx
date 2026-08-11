import type { ReactNode } from 'react';
import type { NavView } from '../types';
import { useStore } from '../state/store';
import {
  IconGrid,
  IconServer,
  IconBox,
  IconCloud,
  IconSettings,
} from './Icons';
import { formatMegabytes } from '../format';
import './Sidebar.css';

interface SidebarProps {
  currentView: NavView;
  onNavigate: (view: NavView) => void;
}

export default function Sidebar({ currentView, onNavigate }: SidebarProps) {
  const store = useStore();
  const { host, servers } = store;

  const navItems: { id: NavView; label: string; icon: ReactNode; badge?: number }[] = [
    { id: 'dashboard', label: 'Dashboard', icon: <IconGrid size={16} /> },
    { id: 'servers', label: 'Servers', icon: <IconServer size={16} />, badge: servers.length },
    { id: 'backups', label: 'Backups', icon: <IconBox size={16} /> },
    { id: 'quick-server', label: 'Quick server', icon: <IconCloud size={16} /> },
    { id: 'settings', label: 'Settings', icon: <IconSettings size={16} /> },
  ];

  const runningCount = servers.filter((s) => s.status === 'running').length;
  const memPct = (host.usedMemory / host.totalMemory) * 100;

  return (
    <aside className="sidebar">
      <nav className="sidebar-nav" aria-label="Main">
        {navItems.map((item) => (
          <button
            key={item.id}
            className={`nav-item ${currentView === item.id ? 'active' : ''}`}
            onClick={() => onNavigate(item.id)}
            aria-current={currentView === item.id ? 'page' : undefined}
          >
            <span className="nav-icon">{item.icon}</span>
            <span className="nav-label">{item.label}</span>
            {item.badge !== undefined && item.badge > 0 && <span className="nav-badge">{item.badge}</span>}
          </button>
        ))}
      </nav>

      <div className="sidebar-foot">
        <div className="host-card">
          <div className="host-top">
            <span className="host-title">This computer</span>
            <span className={`host-pill ${runningCount > 0 ? 'is-active' : ''}`}>
              {runningCount > 0 ? `${runningCount} active` : 'idle'}
            </span>
          </div>
          <div className="host-meter">
            <div className="host-meter-head">
              <span>Memory</span>
              <span>
                {formatMegabytes(host.usedMemory)} / {formatMegabytes(host.totalMemory)}
              </span>
            </div>
            <div className="host-bar">
              <div className="host-bar-fill" style={{ width: `${memPct}%` }} />
            </div>
          </div>
          <div className="host-meter">
            <div className="host-meter-head">
              <span>Processor</span>
              <span>{host.cpu}%</span>
            </div>
            <div className="host-bar">
              <div className="host-bar-fill" style={{ width: `${host.cpu}%` }} />
            </div>
          </div>
          <div className="host-meter">
            <div className="host-meter-head">
              <span>Disk</span>
              <span>{formatMegabytes(host.diskTotal - host.diskUsed)} free</span>
            </div>
            <div className="host-bar">
              <div
                className="host-bar-fill is-warm"
                style={{ width: `${(host.diskUsed / host.diskTotal) * 100}%` }}
              />
            </div>
          </div>
        </div>
      </div>
    </aside>
  );
}
