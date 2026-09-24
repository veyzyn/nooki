import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Copy, Minus, Square, X } from 'lucide-react';
import './WindowControls.css';

const appWindow = getCurrentWindow();

export function toggleMaximize() {
  void appWindow.toggleMaximize().catch(() => undefined);
}

/* Native-feeling caption buttons. They live in the content panel's top bar so
   the app has no separate title strip. Hover is an instant colour change, as
   on Windows itself: these are pressed constantly and must not lag. */
export default function WindowControls() {
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

  return (
    <div className="window-controls">
      <button
        type="button"
        className="window-control"
        aria-label="Minimize window"
        title="Minimize"
        onClick={() => { void appWindow.minimize(); }}
      >
        <Minus aria-hidden="true" />
      </button>
      <button
        type="button"
        className="window-control"
        aria-label={maximized ? 'Restore window' : 'Maximize window'}
        title={maximized ? 'Restore' : 'Maximize'}
        onClick={toggleMaximize}
      >
        {maximized ? <Copy aria-hidden="true" className="is-restore" /> : <Square aria-hidden="true" />}
      </button>
      <button
        type="button"
        className="window-control window-close"
        aria-label="Close window"
        title="Close"
        onClick={() => { void appWindow.close(); }}
      >
        <X aria-hidden="true" />
      </button>
    </div>
  );
}
