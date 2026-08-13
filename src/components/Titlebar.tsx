import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Maximize2, Minimize2, Minus, X } from 'lucide-react';
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
          <Minus aria-hidden="true" />
        </button>
        <button
          type="button"
          className="titlebar-control"
          aria-label={maximized ? 'Restore window' : 'Maximize window'}
          title={maximized ? 'Restore' : 'Maximize'}
          onClick={toggleMaximize}
        >
          {maximized ? (
            <Minimize2 aria-hidden="true" />
          ) : (
            <Maximize2 aria-hidden="true" />
          )}
        </button>
        <button
          type="button"
          className="titlebar-control titlebar-close"
          aria-label="Close window"
          title="Close"
          onClick={() => { void appWindow.close(); }}
        >
          <X aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}
