import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { api } from '../../api/tauri';
import {
  IconChevronRight,
  IconDots,
  IconFilePlus,
  IconFileText,
  IconFolder,
  IconFolderOpen,
  IconFolderPlus,
  IconRefresh,
  IconSearch,
  IconX,
} from '../../components/Icons';
import { ConfirmDialog, EmptyState, Menu, Modal, Spinner } from '../../components/ui';
import { formatBytes, formatDateTime } from '../../format';
import { useStore } from '../../state/store';
import type { Server, ServerFileEntry, ServerFileListing, ServerTextFile } from '../../types';
import './FilesTab.css';

const ServerFileEditor = lazy(() => import('./ServerFileEditor'));

type EditDialog =
  | { kind: 'file' | 'folder'; entry?: undefined }
  | { kind: 'rename'; entry: ServerFileEntry };

function errorMessage(error: unknown) {
  if (typeof error === 'object' && error && 'message' in error) return String((error as { message: unknown }).message);
  return String(error);
}

function fileType(entry: ServerFileEntry) {
  if (entry.kind === 'directory') return 'Folder';
  const extension = entry.name.includes('.') ? entry.name.split('.').pop() : '';
  return extension ? `${extension.toUpperCase()} file` : 'File';
}

export default function FilesTab({ server }: { server: Server }) {
  const { pushToast } = useStore();
  const [listing, setListing] = useState<ServerFileListing>({ path: '', entries: [] });
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState('');
  const [editDialog, setEditDialog] = useState<EditDialog | null>(null);
  const [editName, setEditName] = useState('');
  const [mutating, setMutating] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<ServerFileEntry | null>(null);
  const [openFile, setOpenFile] = useState<ServerTextFile | null>(null);
  const [draft, setDraft] = useState('');
  const [saving, setSaving] = useState(false);
  const [discardEditor, setDiscardEditor] = useState(false);
  const [openingPath, setOpeningPath] = useState<string | null>(null);
  const directoryCache = useRef(new Map<string, ServerFileListing>());
  const navigationRequest = useRef(0);

  const dirty = openFile !== null && draft !== openFile.content;

  const loadDirectory = useCallback(async (path: string, quiet = false) => {
    const request = ++navigationRequest.current;
    const cached = directoryCache.current.get(path);
    if (cached) {
      setListing(cached);
      setQuery('');
      setLoading(false);
    } else if (!quiet) {
      setLoading(true);
    }
    try {
      const next = await api.listServerFiles(server.id, path);
      directoryCache.current.set(path, next);
      if (request !== navigationRequest.current && !quiet) return;
      setListing(next);
      setQuery('');
    } catch (error) {
      pushToast({ tone: 'error', title: 'Could not open this folder', detail: errorMessage(error) });
    } finally {
      if (request === navigationRequest.current || quiet) setLoading(false);
    }
  }, [pushToast, server.id]);

  useEffect(() => { void loadDirectory(''); }, [loadDirectory]);

  const entries = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return listing.entries;
    return listing.entries.filter((entry) => entry.name.toLowerCase().includes(normalized));
  }, [listing.entries, query]);

  const breadcrumbs = useMemo(() => {
    const segments = listing.path ? listing.path.split('/') : [];
    return [
      { label: server.name, path: '' },
      ...segments.map((segment, index) => ({ label: segment, path: segments.slice(0, index + 1).join('/') })),
    ];
  }, [listing.path, server.name]);

  const openEntry = async (entry: ServerFileEntry) => {
    if (entry.kind === 'directory') {
      await loadDirectory(entry.path);
      return;
    }
    if (!entry.editable) {
      pushToast({ tone: 'info', title: 'Preview is not available', detail: 'This file is binary or too large for the built-in editor.' });
      return;
    }
    setOpeningPath(entry.path);
    try {
      const file = await api.readServerTextFile(server.id, entry.path);
      setOpenFile(file);
      setDraft(file.content);
    } catch (error) {
      pushToast({ tone: 'error', title: 'Could not open this file', detail: errorMessage(error) });
    } finally {
      setOpeningPath(null);
    }
  };

  const saveFile = useCallback(async () => {
    if (!openFile || !dirty || saving) return;
    setSaving(true);
    try {
      const saved = await api.saveServerTextFile(server.id, openFile.path, draft);
      setOpenFile(saved);
      setDraft(saved.content);
      pushToast({ tone: 'success', title: 'File saved', detail: saved.path });
      void loadDirectory(listing.path, true);
    } catch (error) {
      pushToast({ tone: 'error', title: 'Could not save this file', detail: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  }, [dirty, draft, listing.path, loadDirectory, openFile, pushToast, saving, server.id]);

  const runEditDialog = async () => {
    if (!editDialog || !editName.trim()) return;
    setMutating(true);
    try {
      if (editDialog.kind === 'file') await api.createServerFile(server.id, listing.path, editName);
      if (editDialog.kind === 'folder') await api.createServerFolder(server.id, listing.path, editName);
      if (editDialog.kind === 'rename') await api.renameServerFile(server.id, editDialog.entry.path, editName);
      directoryCache.current.delete(listing.path);
      setEditDialog(null);
      await loadDirectory(listing.path, true);
    } catch (error) {
      pushToast({ tone: 'error', title: 'Could not complete that change', detail: errorMessage(error) });
    } finally {
      setMutating(false);
    }
  };

  const deleteEntry = async () => {
    if (!deleteTarget) return;
    setMutating(true);
    try {
      await api.deleteServerFile(server.id, deleteTarget.path);
      directoryCache.current.delete(listing.path);
      pushToast({ tone: 'success', title: `${deleteTarget.name} moved to the Recycle Bin` });
      setDeleteTarget(null);
      await loadDirectory(listing.path, true);
    } catch (error) {
      pushToast({ tone: 'error', title: 'Could not remove this item', detail: errorMessage(error) });
    } finally {
      setMutating(false);
    }
  };

  const closeEditor = () => {
    if (dirty) setDiscardEditor(true);
    else setOpenFile(null);
  };

  if (openFile) {
    const fileName = openFile.path.split('/').pop() ?? openFile.path;
    return (
      <div className="tab files-tab files-editor-view">
        <Suspense fallback={<div className="files-editor-skeleton"><div className="files-editor-skeleton-head" /><div className="files-editor-skeleton-body"><Spinner size={18} /><span>Preparing editor</span></div></div>}>
          <ServerFileEditor serverId={server.id} file={openFile} draft={draft} dirty={dirty} saving={saving} onDraftChange={setDraft} onSave={() => void saveFile()} onClose={closeEditor} />
        </Suspense>
        <ConfirmDialog
          open={discardEditor}
          title="Discard unsaved changes?"
          description={`Your edits to ${fileName} have not been saved.`}
          confirmLabel="Discard changes"
          tone="danger"
          onCancel={() => setDiscardEditor(false)}
          onConfirm={() => { setDiscardEditor(false); setOpenFile(null); }}
        />
      </div>
    );
  }

  return (
    <div className="tab files-tab">
      <div className="files-topbar">
        <div className="files-heading">
          <h2>Files</h2>
          <p>Browse and edit files inside this server.</p>
        </div>
        <div className="files-actions">
          <button className="btn btn-secondary btn-sm" onClick={() => { setEditName(''); setEditDialog({ kind: 'file' }); }}><IconFilePlus size={14} /> New file</button>
          <button className="btn btn-secondary btn-sm" onClick={() => { setEditName(''); setEditDialog({ kind: 'folder' }); }}><IconFolderPlus size={14} /> New folder</button>
          <button className="icon-btn" onClick={() => void loadDirectory(listing.path)} aria-label="Refresh files"><IconRefresh size={14} /></button>
        </div>
      </div>

      <div className="files-browser">
        <div className="files-browser-toolbar">
          <nav className="files-breadcrumbs" aria-label="Current folder">
            {breadcrumbs.map((crumb, index) => (
              <span key={crumb.path || 'root'}>
                {index > 0 && <IconChevronRight size={13} />}
                <button
                  className={index === 0 ? 'files-root-crumb' : ''}
                  style={{ '--crumb-font-size': `${Math.max(9.5, 12 - Math.max(0, crumb.label.length - 20) * 0.08)}px` } as CSSProperties}
                  onClick={() => void loadDirectory(crumb.path)}
                  title={crumb.label}
                >{index === 0 && <IconFolderOpen size={14} />}<span>{crumb.label}</span></button>
              </span>
            ))}
          </nav>
          <div className="files-search">
            <IconSearch size={14} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search this folder" />
            {query && <button onClick={() => setQuery('')} aria-label="Clear search"><IconX size={12} /></button>}
          </div>
        </div>

        <div className="files-list-head">
          <span>Name</span><span>Type</span><span>Size</span><span>Modified</span><span />
        </div>
        <div className="files-list">
          {loading ? (
            <div className="files-skeletons">{Array.from({ length: 7 }, (_, index) => <div key={index} className="files-skeleton" />)}</div>
          ) : entries.length === 0 ? (
            <EmptyState
              icon={query ? <IconSearch size={34} /> : <IconFolderOpen size={34} />}
              title={query ? 'No matching files' : 'This folder is empty'}
              description={query ? 'Try another search.' : 'Create a file or folder to get started.'}
            />
          ) : entries.map((entry) => (
            <div className="files-row" key={entry.path}>
              <button className="files-name" onClick={() => void openEntry(entry)} title={entry.editable || entry.kind === 'directory' ? `Open ${entry.name}` : 'This file cannot be edited in Nooki'}>
                <span className={`files-kind-icon ${entry.kind === 'directory' ? 'is-folder' : ''}`}>
                  {openingPath === entry.path ? <Spinner size={14} /> : entry.kind === 'directory' ? <IconFolder size={17} /> : <IconFileText size={17} />}
                </span>
                <span>{entry.name}</span>
              </button>
              <span className="files-meta">{fileType(entry)}</span>
              <span className="files-meta mono">{entry.kind === 'directory' ? '—' : formatBytes(entry.size)}</span>
              <span className="files-meta">{entry.modifiedAt ? formatDateTime(entry.modifiedAt) : '—'}</span>
              <Menu
                trigger={<button className="icon-btn files-menu" aria-label={`Actions for ${entry.name}`}><IconDots size={15} /></button>}
                items={[
                  ...(entry.kind === 'directory' || entry.editable ? [{ label: 'Open', onSelect: () => { void openEntry(entry); } }] : []),
                  { label: 'Rename', onSelect: () => { setEditName(entry.name); setEditDialog({ kind: 'rename', entry }); } },
                  { label: 'Move to Recycle Bin', danger: true, onSelect: () => setDeleteTarget(entry) },
                ]}
              />
            </div>
          ))}
        </div>
      </div>

      <Modal
        open={editDialog !== null}
        onClose={() => setEditDialog(null)}
        title={editDialog?.kind === 'rename' ? 'Rename item' : editDialog?.kind === 'folder' ? 'Create folder' : 'Create file'}
        description={editDialog?.kind === 'rename' ? 'Choose a new name. The item stays in the same folder.' : `Add it inside ${listing.path || server.name}.`}
        width={420}
        footer={<><button className="btn btn-secondary" onClick={() => setEditDialog(null)}>Cancel</button><button className="btn btn-primary" disabled={!editName.trim() || mutating} onClick={() => void runEditDialog()}>{mutating && <Spinner size={12} />}{editDialog?.kind === 'rename' ? 'Rename' : 'Create'}</button></>}
      >
        <label className="field"><span className="field-label">Name</span><input className="input" value={editName} onChange={(event) => setEditName(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') void runEditDialog(); }} autoFocus /></label>
      </Modal>

      <ConfirmDialog
        open={deleteTarget !== null}
        title={`Move ${deleteTarget?.name ?? 'this item'} to the Recycle Bin?`}
        description={deleteTarget?.kind === 'directory' ? 'The folder and everything inside it will be removed from the server.' : 'The file will be removed from the server.'}
        confirmLabel="Move to Recycle Bin"
        tone="danger"
        notes={[deleteTarget?.path ?? '']}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => void deleteEntry()}
      />
    </div>
  );
}
