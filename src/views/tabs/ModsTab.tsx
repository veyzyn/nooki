import { useEffect, useRef, useState } from 'react';
import { open as openFile } from '@tauri-apps/plugin-dialog';
import { useStore } from '../../state/store';
import type { AddonVersionOption, ManualModDownload, ModCatalog, ModFile, ModProject, ModProvider, Server } from '../../types';
import { ConfirmDialog, EmptyState, Modal, Segmented, Spinner } from '../../components/ui';
import { IconCheck, IconDownload, IconMod, IconPlus, IconRefresh, IconSearch, IconTrash, IconX } from '../../components/Icons';
import { formatBytes, formatRelative } from '../../format';
import AddonVersionDialog from './AddonVersionDialog';
import './PluginsTab.css';

type Mode = 'installed' | ModProvider;

function messageFrom(error: unknown) {
  if (typeof error === 'object' && error && 'message' in error) return String((error as { message: unknown }).message);
  return String(error);
}

function compactNumber(value: number) {
  return new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(value);
}

export default function ModsTab({ server }: { server: Server }) {
  const store = useStore();
  const [mode, setMode] = useState<Mode>('installed');
  const [mods, setMods] = useState<ModFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [adding, setAdding] = useState(false);
  const [changing, setChanging] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ModFile | null>(null);
  const [query, setQuery] = useState('');
  const [catalog, setCatalog] = useState<ModCatalog | null>(null);
  const [searching, setSearching] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [installing, setInstalling] = useState<string | null>(null);
  const [installProgress, setInstallProgress] = useState(0);
  const [operationId, setOperationId] = useState<string | null>(null);
  const [manual, setManual] = useState<ManualModDownload | null>(null);
  const [versionTarget, setVersionTarget] = useState<ModProject | null>(null);
  const [versions, setVersions] = useState<AddonVersionOption[]>([]);
  const [selectedVersionId, setSelectedVersionId] = useState('');
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [versionError, setVersionError] = useState('');
  const polling = useRef(false);
  const searchSequence = useRef(0);
  const editable = server.status === 'stopped' || server.status === 'crashed';
  const running = server.status === 'running';
  const installable = editable || running;
  const provider = mode === 'installed' ? null : mode;

  const refresh = async (quiet = false) => {
    if (quiet) setRefreshing(true); else setLoading(true);
    try {
      setMods(await store.listMods(server.id));
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Could not read mods', detail: messageFrom(error) });
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => { void refresh(); }, [server.id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!provider) return;
    const sequence = ++searchSequence.current;
    const timer = window.setTimeout(() => {
      setSearching(true);
      setCatalog(null);
      void store.searchMods(provider, server.type as 'fabric' | 'forge' | 'neoforge', server.version, query, 0)
        .then((result) => { if (searchSequence.current === sequence) setCatalog(result); })
        .catch((error) => {
          if (searchSequence.current === sequence) {
            store.pushToast({ tone: 'error', title: `${provider === 'modrinth' ? 'Modrinth' : 'CurseForge'} is unavailable`, detail: messageFrom(error) });
          }
        })
        .finally(() => { if (searchSequence.current === sequence) setSearching(false); });
    }, query ? 280 : 0);
    return () => window.clearTimeout(timer);
  }, [provider, query, server.type, server.version, store.searchMods, store.pushToast]);

  useEffect(() => {
    if (!manual) return;
    const timer = window.setInterval(() => {
      if (polling.current) return;
      polling.current = true;
      void store.checkManualModDownload(manual.token).then((result) => {
        if (!result.manualDownload) {
          setMods(result.mods);
          setManual(null);
          setInstalling(null);
          store.pushToast({
            tone: 'success', title: `${manual.projectName} installed`,
            detail: running ? 'Restart the server to load the mod.' : 'It will load the next time the server starts.',
          });
        }
      }).catch((error) => {
        setManual(null);
        setInstalling(null);
        store.pushToast({ tone: 'error', title: 'Manual mod install failed', detail: messageFrom(error) });
      }).finally(() => { polling.current = false; });
    }, 1200);
    return () => window.clearInterval(timer);
  }, [manual, running, store.checkManualModDownload, store.pushToast]);

  const toggle = async (mod: ModFile) => {
    setChanging(mod.fileName);
    try {
      setMods(await store.setModEnabled(server.id, mod.fileName, !mod.enabled));
      store.pushToast({
        tone: 'success', title: mod.enabled ? `${mod.name} disabled` : `${mod.name} enabled`,
        detail: 'The change takes effect the next time the server starts.',
      });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Mod was not changed', detail: messageFrom(error) });
    } finally { setChanging(null); }
  };

  const addFiles = async () => {
    const selected = await openFile({
      multiple: true,
      directory: false,
      title: 'Add mod JARs',
      filters: [{ name: 'Mod JARs', extensions: ['jar'] }],
    });
    const paths = typeof selected === 'string' ? [selected] : selected ?? [];
    if (paths.length === 0) return;
    setAdding(true);
    try {
      setMods(await store.addModFiles(server.id, paths));
      store.pushToast({
        tone: 'success',
        title: `${paths.length} mod${paths.length === 1 ? '' : 's'} added`,
        detail: running ? 'The files were copied. Restart the server to load them.' : 'The files were copied into this server.',
      });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Mods were not added', detail: messageFrom(error) });
    } finally {
      setAdding(false);
    }
  };

  const remove = async () => {
    if (!deleteTarget) return;
    const target = deleteTarget;
    setDeleteTarget(null);
    setChanging(target.fileName);
    try {
      setMods(await store.deleteMod(server.id, target.fileName));
      store.pushToast({ tone: 'success', title: `${target.name} moved to the Recycle Bin` });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Mod was not deleted', detail: messageFrom(error) });
    } finally { setChanging(null); }
  };

  const chooseVersion = async (project: ModProject) => {
    setVersionTarget(project);
    setVersions([]);
    setSelectedVersionId('');
    setVersionError('');
    setVersionsLoading(true);
    try {
      const options = await store.listModVersions(server.id, project.provider, project.projectId);
      setVersions(options);
      setSelectedVersionId(options.find((version) => version.releaseType.toLowerCase() === 'release')?.id ?? options[0]?.id ?? '');
    } catch (error) {
      setVersionError(messageFrom(error));
    } finally {
      setVersionsLoading(false);
    }
  };

  const install = async (project: ModProject, versionId: string) => {
    const key = `${project.provider}:${project.projectId}`;
    setInstalling(key);
    setInstallProgress(0);
    setOperationId(null);
    try {
      const result = await store.installMod(server.id, project.provider, project.projectId, versionId, (event) => {
        setOperationId(event.data.operationId);
        if (event.event === 'progress') setInstallProgress(event.data.progress ?? 0);
      });
      setMods(result.mods);
      if (result.manualDownload) {
        setManual(result.manualDownload);
        return;
      }
      store.pushToast({
        tone: 'success', title: `${project.name} installed`,
        detail: running ? 'The download is complete; restart the server to load it.' : 'It will load the next time the server starts.',
      });
      setInstalling(null);
    } catch (error) {
      if ((error as { code?: string })?.code !== 'cancelled') store.pushToast({ tone: 'error', title: `${project.name} was not installed`, detail: messageFrom(error) });
      setInstalling(null);
    } finally { setInstallProgress(0); setOperationId(null); }
  };

  const loadMore = async () => {
    if (!provider || !catalog?.hasMore) return;
    setLoadingMore(true);
    try {
      const next = await store.searchMods(provider, server.type as 'fabric' | 'forge' | 'neoforge', server.version, query, catalog.offset + catalog.projects.length);
      setCatalog({ ...next, projects: [...catalog.projects, ...next.projects] });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Could not load more mods', detail: messageFrom(error) });
    } finally { setLoadingMore(false); }
  };

  const closeManual = () => {
    if (!manual) return;
    void store.cancelManualModDownload(manual.token);
    setManual(null);
    setInstalling(null);
  };

  return (
    <div className="tab plugins-tab">
      <div className="plugins-toolbar">
        <Segmented
          value={mode}
          onChange={setMode}
          options={[
            { value: 'installed', label: `Installed${mods.length ? ` (${mods.length})` : ''}` },
            { value: 'modrinth', label: 'Modrinth' },
            { value: 'curseforge', label: 'CurseForge' },
          ]}
        />
        {mode === 'installed' ? (
          <div className="plugin-toolbar-actions">
            <button className="btn btn-secondary btn-sm" disabled={!installable || adding} onClick={() => void addFiles()}>
              {adding ? <Spinner size={12} /> : <IconPlus size={13} />} Add mods
            </button>
            <button className="btn btn-secondary btn-sm" disabled={refreshing || adding} onClick={() => void refresh(true)}>
              {refreshing ? <Spinner size={12} /> : <IconRefresh size={13} />} Refresh
            </button>
          </div>
        ) : (
          <div className="plugin-search">
            <IconSearch size={14} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Search ${mode === 'modrinth' ? 'Modrinth' : 'CurseForge'} mods`} aria-label="Search mods" />
            {query && <button className="icon-btn" onClick={() => setQuery('')} aria-label="Clear search"><IconX size={12} /></button>}
          </div>
        )}
      </div>

      {!editable && (
        <div className="plugin-running-note">
          {running
            ? 'You can install mods while the server is running, but they will not load until you restart it. Stop the server to enable, disable, or delete mods.'
            : 'Wait for the current server operation to finish before changing mods.'}
        </div>
      )}

      {mode === 'installed' ? (
        <div className="plugins-surface">
          {loading ? <ModSkeleton count={4} /> : mods.length === 0 ? (
            <EmptyState
              icon={<IconMod size={40} />}
              title="No mods installed"
              description={`Browse server-compatible ${server.type === 'fabric' ? 'Fabric' : server.type === 'neoforge' ? 'NeoForge' : 'Forge'} mods for Minecraft ${server.version}.`}
              action={<button className="btn btn-primary" onClick={() => setMode('modrinth')}>Browse mods</button>}
            />
          ) : mods.map((mod) => {
            const metadata = mod.metadata;
            return <div className={`plugin-row ${mod.enabled ? '' : 'is-disabled'}`} key={mod.fileName}>
              {metadata?.iconUrl ? <ModLogo provider={metadata.provider} iconUrl={metadata.iconUrl} name={metadata.name} /> : <div className="plugin-mark"><IconMod size={18} /></div>}
              <div className="plugin-main">
                <div className="plugin-name-line">
                  <span className="plugin-name">{metadata?.name || mod.name}</span>
                  {(metadata?.version || mod.version) && <span className="plugin-version">v{metadata?.version || mod.version}</span>}
                  {metadata && <span className="plugin-source-tag">{metadata.provider === 'modrinth' ? 'Modrinth' : 'CurseForge'}</span>}
                  {!mod.enabled && <span className="plugin-disabled-tag">disabled</span>}
                </div>
                <span className="plugin-description">{metadata?.description || mod.description || mod.fileName}</span>
                <span className="plugin-meta">
                  {metadata?.author ? `By ${metadata.author} · ` : mod.authors.length ? `By ${mod.authors.join(', ')} · ` : ''}{formatBytes(mod.size)} · changed {formatRelative(mod.modifiedAt)}
                </span>
              </div>
              <div className="plugin-actions">
                {changing === mod.fileName && <Spinner size={13} />}
                <button type="button" role="switch" aria-checked={mod.enabled} aria-label={`${mod.enabled ? 'Disable' : 'Enable'} ${mod.name}`} className={`switch ${mod.enabled ? 'on' : ''}`} disabled={!editable || changing !== null} onClick={() => void toggle(mod)}>
                  <span className="switch-knob" />
                </button>
                <button className="icon-btn danger-text" disabled={!editable || changing !== null} onClick={() => setDeleteTarget(mod)} aria-label={`Delete ${mod.name}`} title="Move to Recycle Bin"><IconTrash size={14} /></button>
              </div>
            </div>;
          })}
        </div>
      ) : (
        <div className="plugins-surface plugin-browser" aria-busy={searching}>
          {searching ? <ModSkeleton count={6} /> : catalog?.projects.length === 0 ? (
            <EmptyState icon={<IconSearch size={40} />} title="No matching mods" description={`No compatible ${server.type} ${server.version} results were found.`} />
          ) : catalog?.projects.map((project) => {
            const key = `${project.provider}:${project.projectId}`;
            const busy = installing === key;
            const installed = mods.some((mod) => mod.metadata?.provider === project.provider && mod.metadata.projectId === project.projectId);
            return <div className="plugin-row plugin-project-row" key={key}>
              {project.iconUrl ? <ModLogo provider={project.provider} iconUrl={project.iconUrl} name={project.name} /> : <div className="plugin-mark plugin-project-mark">{project.name.slice(0, 1).toUpperCase()}</div>}
              <div className="plugin-main">
                <div className="plugin-name-line"><span className="plugin-name">{project.name}</span><span className="plugin-author">by {project.author}</span></div>
                <span className="plugin-description">{project.description}</span>
                <span className="plugin-meta">{compactNumber(project.downloads)} downloads{project.followers ? ` · ${compactNumber(project.followers)} followers` : ''} · updated {formatRelative(project.lastUpdated)}</span>
                {busy && !manual && <div className="plugin-install-progress"><span style={{ width: `${installProgress}%` }} /></div>}
              </div>
              <button className={`btn btn-sm ${busy ? 'btn-secondary' : installed ? 'btn-secondary plugin-installed-btn' : 'btn-primary'}`} disabled={installed || !installable || (installing !== null && !busy) || (busy && (Boolean(manual) || !operationId))} onClick={() => busy && operationId ? void store.cancelOperation(operationId) : void chooseVersion(project)}>
                {busy && !manual ? <IconX size={12} /> : installed ? <IconCheck size={13} /> : <IconDownload size={13} />}
                {busy ? (manual ? 'Waiting' : 'Cancel') : installed ? 'Installed' : 'Install'}
              </button>
            </div>;
          })}
          {loadingMore && <ModSkeleton count={3} />}
          {catalog?.hasMore && !loadingMore && <div className="plugin-load-more"><button className="btn btn-secondary" onClick={() => void loadMore()}>Load more</button></div>}
        </div>
      )}

      <ConfirmDialog
        open={deleteTarget !== null}
        title={`Delete ${deleteTarget?.name ?? 'mod'}?`}
        description="The mod JAR will be moved to the Windows Recycle Bin. Configuration files will be kept."
        confirmLabel="Delete mod"
        tone="danger"
        notes={deleteTarget ? [deleteTarget.fileName, 'The change takes effect the next time the server starts.'] : undefined}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => void remove()}
      />

      <AddonVersionDialog
        open={versionTarget !== null}
        projectName={versionTarget?.name ?? 'mod'}
        kind="mod"
        versions={versions}
        selectedId={selectedVersionId}
        loading={versionsLoading}
        error={versionError}
        onSelect={setSelectedVersionId}
        onClose={() => setVersionTarget(null)}
        onInstall={() => {
          if (!versionTarget || !selectedVersionId) return;
          const project = versionTarget;
          const versionId = selectedVersionId;
          setVersionTarget(null);
          void install(project, versionId);
        }}
      />

      <Modal
        open={manual !== null}
        onClose={closeManual}
        title={`Download ${manual?.projectName ?? 'mod'} manually`}
        description="This author does not allow third-party launchers to download the file directly."
        width={520}
        footer={<>
          <button className="btn btn-ghost" onClick={closeManual}>Cancel</button>
          <button className="btn btn-primary" onClick={() => manual && void store.openManualModDownload(manual.token)}><IconDownload size={13} /> Open download page</button>
        </>}
      >
        <div className="manual-mod-download">
          <div className="manual-mod-status"><Spinner size={15} /><div><strong>Waiting for the download</strong><span>Nooki is watching your Downloads folder and will install the verified file automatically.</span></div></div>
          <div className="manual-mod-file"><span>Expected file</span><code>{manual?.fileName}</code></div>
          <div className="manual-mod-file"><span>Watching</span><code>{manual?.downloadsFolder}</code></div>
        </div>
      </Modal>
    </div>
  );
}

function ModSkeleton({ count }: { count: number }) {
  return <>{Array.from({ length: count }, (_, index) => <div className="plugin-row plugin-skeleton" key={index}>
    <span className="skeleton-block skeleton-icon" /><span className="skeleton-copy"><i /><i /></span><span className="skeleton-block skeleton-button" />
  </div>)}</>;
}

function ModLogo({ provider, iconUrl, name }: { provider: ModProvider; iconUrl: string; name: string }) {
  const store = useStore();
  const [source, setSource] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    let active = true;
    setSource(null); setLoaded(false);
    void store.loadModIcon(provider, iconUrl).then((icon) => {
      if (!active) return; setSource(icon); if (!icon) setLoaded(true);
    }).catch(() => { if (active) setLoaded(true); });
    return () => { active = false; };
  }, [provider, iconUrl, store.loadModIcon]);
  return <div className={`plugin-mark plugin-project-mark ${loaded ? 'is-loaded' : ''}`}>
    {!loaded && <span className="plugin-logo-skeleton" />}
    {source ? <img src={source} alt="" onLoad={() => setLoaded(true)} onError={() => { setSource(null); setLoaded(true); }} /> : loaded ? name.slice(0, 1).toUpperCase() : null}
  </div>;
}
