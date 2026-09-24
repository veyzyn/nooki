import { createContext, useContext, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useStore } from '../state/store';
import type { NavView } from '../types';
import { IconChevronRight } from './Icons';
import ServerIcon from './ServerIcon';
import WindowControls, { toggleMaximize } from './WindowControls';
import { statusLabels, statusTone } from '../format';
import './TopBar.css';

const viewTitles: Record<NavView, string> = {
  dashboard: 'Dashboard',
  servers: 'Servers',
  backups: 'Backups',
  'quick-server': 'Quick server',
  settings: 'Settings',
};

const SlotContext = createContext<HTMLElement | null>(null);

export function TopBarSlotProvider({ element, children }: { element: HTMLElement | null; children: ReactNode }) {
  return <SlotContext.Provider value={element}>{children}</SlotContext.Provider>;
}

/** Renders a view's primary actions into the panel's top bar. */
export function PageActions({ children }: { children: ReactNode }) {
  const slot = useContext(SlotContext);
  return slot ? createPortal(children, slot) : null;
}

export default function TopBar({ onSlot, bare = false }: { onSlot?: (element: HTMLElement | null) => void; bare?: boolean }) {
  return (
    <header className="topbar" data-tauri-drag-region onDoubleClick={(event) => { if (event.target === event.currentTarget) toggleMaximize(); }}>
      {!bare && <Breadcrumbs />}
      <div className="topbar-spacer" data-tauri-drag-region onDoubleClick={toggleMaximize} />
      {!bare && <div className="topbar-actions" ref={onSlot} />}
      <WindowControls />
    </header>
  );
}

function Breadcrumbs() {
  const store = useStore();
  const server = store.nav === 'servers' ? store.servers.find((item) => item.id === store.openServerId) : undefined;
  const count = store.nav === 'servers' && !server ? store.servers.length : undefined;

  return (
    <nav className="crumbs" aria-label="Breadcrumb">
      {server ? (
        <>
          <button className="crumb crumb-link" onClick={store.closeServer}>Servers</button>
          <IconChevronRight size={13} className="crumb-sep" aria-hidden="true" />
          <span className="crumb crumb-current crumb-server" aria-current="page">
            <ServerIcon server={server} size={18} />
            <span className="crumb-name" title={server.name}>{server.name}</span>
            <span className={`status-badge status-${statusTone(server.status)}`}>{statusLabels[server.status]}</span>
          </span>
        </>
      ) : (
        <span className="crumb crumb-current" aria-current="page">
          {viewTitles[store.nav]}
          {count !== undefined && count > 0 && <span className="crumb-count">{count}</span>}
        </span>
      )}
    </nav>
  );
}
