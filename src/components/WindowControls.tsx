import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
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
        <CaptionGlyph kind="minimize" />
      </button>
      <button
        type="button"
        className="window-control"
        aria-label={maximized ? 'Restore window' : 'Maximize window'}
        title={maximized ? 'Restore' : 'Maximize'}
        onClick={toggleMaximize}
      >
        <CaptionGlyph kind={maximized ? 'restore' : 'maximize'} />
      </button>
      <button
        type="button"
        className="window-control window-close"
        aria-label="Close window"
        title="Close"
        onClick={() => { void appWindow.close(); }}
      >
        <CaptionGlyph kind="close" />
      </button>
    </div>
  );
}

/* Windows-style caption glyphs: 10px, 1px hairlines on half-pixel coordinates
   so every stroke lands on a whole device pixel and sits dead centre. */
function CaptionGlyph({ kind }: { kind: 'minimize' | 'maximize' | 'restore' | 'close' }) {
  return (
    <svg className="caption-glyph" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      {kind === 'minimize' && <path d="M0 5.5h10" />}
      {kind === 'maximize' && <rect x="0.5" y="0.5" width="9" height="9" />}
      {kind === 'restore' && <><rect x="0.5" y="2.5" width="7" height="7" /><path d="M2.5 2.5v-2h7v7h-2" /></>}
      {kind === 'close' && <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />}
    </svg>
  );
}
