import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from 'react';
import { Dialog as DialogPrimitive } from '@base-ui/react/dialog';
import { useStore } from '../state/store';
import type { NavView, ServerTab } from '../types';
import { IconBox, IconCloud, IconGrid, IconPlay, IconPlus, IconSearch, IconServer, IconSettings, IconTerminal } from './Icons';
import ServerIcon from './ServerIcon';
import { softwareLabel, statusLabels } from '../format';
import './CommandMenu.css';

interface Command {
  id: string;
  group: string;
  label: string;
  hint?: string;
  keywords?: string;
  icon: ReactNode;
  run: () => void;
}

/* Opened by keyboard many times a day, so it has no open/close animation:
   it appears the instant Ctrl+K is pressed and vanishes just as fast. */
export default function CommandMenu({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const store = useStore();
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        // Leave Ctrl+K chords to the code editor when it has focus.
        if (event.target instanceof Element && event.target.closest('.monaco-editor')) return;
        event.preventDefault();
        onOpenChange(!open);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onOpenChange]);

  useEffect(() => {
    if (open) { setQuery(''); setActive(0); }
  }, [open]);

  const commands = useMemo<Command[]>(() => {
    const go = (view: NavView) => () => store.setNav(view);
    const openServer = (id: string, tab?: ServerTab) => () => store.openServer(id, tab);
    const list: Command[] = [
      { id: 'nav-dashboard', group: 'Go to', label: 'Dashboard', icon: <IconGrid size={15} />, run: go('dashboard') },
      { id: 'nav-servers', group: 'Go to', label: 'Servers', icon: <IconServer size={15} />, run: () => { store.setNav('servers'); store.closeServer(); } },
      { id: 'nav-backups', group: 'Go to', label: 'Backups', icon: <IconBox size={15} />, run: go('backups') },
      { id: 'nav-quick', group: 'Go to', label: 'Quick server', icon: <IconCloud size={15} />, run: go('quick-server') },
      { id: 'nav-settings', group: 'Go to', label: 'Settings', keywords: 'preferences java folders', icon: <IconSettings size={15} />, run: go('settings') },
      { id: 'add-server', group: 'Actions', label: 'Add server', keywords: 'new create import modpack', icon: <IconPlus size={15} />, run: () => store.setWizardOpen(true) },
    ];
    for (const server of store.servers) {
      list.push({
        id: `open-${server.id}`, group: 'Servers', label: server.name,
        hint: statusLabels[server.status], keywords: `${softwareLabel(server.type)} ${server.version} ${server.port}`,
        icon: <ServerIcon server={server} size={18} />, run: openServer(server.id),
      });
    }
    for (const server of store.servers) {
      if (server.status === 'stopped' || server.status === 'crashed') {
        list.push({ id: `start-${server.id}`, group: 'Actions', label: `Start ${server.name}`, icon: <IconPlay size={15} />, run: () => store.startServer(server.id) });
      }
      if (server.status === 'running') {
        list.push({ id: `console-${server.id}`, group: 'Actions', label: `Open console for ${server.name}`, keywords: 'command terminal', icon: <IconTerminal size={15} />, run: openServer(server.id, 'console') });
      }
    }
    return list;
  }, [store]);

  const results = useMemo(() => {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0) return commands;
    return commands.filter((command) => {
      const haystack = `${command.group} ${command.label} ${command.keywords ?? ''}`.toLowerCase();
      return terms.every((term) => haystack.includes(term));
    });
  }, [commands, query]);

  useEffect(() => { setActive(0); }, [query]);

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  }, [active]);

  const run = (command: Command | undefined) => {
    if (!command) return;
    onOpenChange(false);
    command.run();
  };

  const onKeyDown = (event: ReactKeyboardEvent) => {
    if (event.key === 'ArrowDown') { event.preventDefault(); setActive((index) => Math.min(results.length - 1, index + 1)); }
    else if (event.key === 'ArrowUp') { event.preventDefault(); setActive((index) => Math.max(0, index - 1)); }
    else if (event.key === 'Enter') { event.preventDefault(); run(results[active]); }
  };

  let lastGroup = '';

  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Backdrop className="cmdk-backdrop" />
        <DialogPrimitive.Popup className="cmdk" aria-label="Command menu">
          <div className="cmdk-input-row">
            <IconSearch size={15} />
            <input
              className="cmdk-input"
              autoFocus
              value={query}
              placeholder="Search servers, pages, and actions…"
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={onKeyDown}
              role="combobox"
              aria-expanded="true"
              aria-controls="cmdk-list"
              aria-activedescendant={results[active] ? `cmdk-${results[active].id}` : undefined}
            />
            <kbd className="kbd">Esc</kbd>
          </div>
          <div className="cmdk-list" id="cmdk-list" role="listbox" ref={listRef}>
            {results.length === 0 && <div className="cmdk-empty">No results for “{query}”</div>}
            {results.map((command, index) => {
              const header = command.group !== lastGroup ? command.group : null;
              lastGroup = command.group;
              return (
                <div key={command.id} role="presentation">
                  {header && <div className="cmdk-group">{header}</div>}
                  <div
                    id={`cmdk-${command.id}`}
                    role="option"
                    aria-selected={index === active}
                    data-index={index}
                    className={`cmdk-item ${index === active ? 'is-active' : ''}`}
                    onPointerMove={() => setActive(index)}
                    onClick={() => run(command)}
                  >
                    <span className="cmdk-item-icon">{command.icon}</span>
                    <span className="cmdk-item-label">{command.label}</span>
                    {command.hint && <span className="cmdk-item-hint">{command.hint}</span>}
                  </div>
                </div>
              );
            })}
          </div>
        </DialogPrimitive.Popup>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
