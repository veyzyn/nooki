import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { NookiLogo } from './Icons';
import './Titlebar.css';

const appWindow = getCurrentWindow();

export default function Titlebar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const syncMaximized = () => {
      void appWindow.isMaximized().then(setMaximized).catch(() => undefined);
    };

    syncMaximized();
    void appWindow.onResized(syncMaximized).then((stopListening) => {
      unlisten = stopListening;
    }).catch(() => undefined);

    return () => unlisten?.();
  }, []);

  const toggleMaximize = () => {
    void appWindow.toggleMaximize().then(() => appWindow.isMaximized()).then(setMaximized).catch(() => undefined);
  };

  return (
    <header className="titlebar">
      <div
        className="titlebar-drag"
        data-tauri-drag-region
        onDoubleClick={toggleMaximize}
      >
        <NookiLogo size={18} />
        <span className="titlebar-name">Nooki</span>
      </div>
      <div className="titlebar-controls">
        <button
          type="button"
          className="titlebar-control"
          aria-label="Minimize window"
          title="Minimize"
          onClick={() => { void appWindow.minimize(); }}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 8.5h8" /></svg>
        </button>
        <button
          type="button"
          className="titlebar-control"
          aria-label={maximized ? 'Restore window' : 'Maximize window'}
          title={maximized ? 'Restore' : 'Maximize'}
          onClick={toggleMaximize}
        >
          {maximized ? (
            <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3.5 4.5V2.5h6v6h-2M2.5 4.5h5v5h-5z" /></svg>
          ) : (
            <svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.5" y="2.5" width="7" height="7" /></svg>
          )}
        </button>
        <button
          type="button"
          className="titlebar-control titlebar-close"
          aria-label="Close window"
          title="Close"
          onClick={() => { void appWindow.close(); }}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 2.5 7 7m0-7-7 7" /></svg>
        </button>
      </div>
    </header>
  );
}
