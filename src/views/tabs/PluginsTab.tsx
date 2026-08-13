import { useEffect, useRef, useState } from 'react';
import { open as openFile } from '@tauri-apps/plugin-dialog';
import { useStore } from '../../state/store';
import type { AddonVersionOption, PluginCatalog, PluginFile, PluginProject, Server } from '../../types';
import { ConfirmDialog, EmptyState, Segmented, Spinner } from '../../components/ui';
import { IconCheck, IconDownload, IconPlug, IconPlus, IconRefresh, IconSearch, IconTrash, IconX } from '../../components/Icons';
import { formatBytes, formatRelative } from '../../format';
import AddonVersionDialog from './AddonVersionDialog';
import './PluginsTab.css';

type Mode = 'installed' | 'browse';

function messageFrom(error: unknown) {
  if (typeof error === 'object' && error && 'message' in error) return String((error as { message: unknown }).message);
  return String(error);
}

function compactNumber(value: number) {
  return new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(value);
}

export default function PluginsTab({ server }: { server: Server }) {
  const store = useStore();
  const [mode, setMode] = useState<Mode>('installed');
  const [plugins, setPlugins] = useState<PluginFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [adding, setAdding] = useState(false);
  const [changing, setChanging] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<PluginFile | null>(null);
  const [query, setQuery] = useState('');
  const [catalog, setCatalog] = useState<PluginCatalog | null>(null);
  const [searching, setSearching] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [installing, setInstalling] = useState<string | null>(null);
  const [installProgress, setInstallProgress] = useState(0);
  const [operationId, setOperationId] = useState<string | null>(null);
  const [versionTarget, setVersionTarget] = useState<PluginProject | null>(null);
  const [versions, setVersions] = useState<AddonVersionOption[]>([]);
  const [selectedVersionId, setSelectedVersionId] = useState('');
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [versionError, setVersionError] = useState('');
  const searchSequence = useRef(0);
  const editable = server.status === 'stopped' || server.status === 'crashed';
  const running = server.status === 'running';
  const installable = editable || running;

  const refresh = async (quiet = false) => {
    if (quiet) setRefreshing(true); else setLoading(true);
    try {
      setPlugins(await store.listPlugins(server.id));
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Could not read plugins', detail: messageFrom(error) });
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => { void refresh(); }, [server.id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (mode !== 'browse') return;
    const sequence = ++searchSequence.current;
    const timer = window.setTimeout(() => {
      setSearching(true);
      void store.searchPlugins(query, 0).then((result) => {
        if (searchSequence.current === sequence) setCatalog(result);
      }).catch((error) => {
        if (searchSequence.current === sequence) {
          setCatalog(null);
          store.pushToast({ tone: 'error', title: 'Plugin browser is unavailable', detail: messageFrom(error) });
        }
      }).finally(() => {
        if (searchSequence.current === sequence) setSearching(false);
      });
    }, query ? 280 : 0);
    return () => window.clearTimeout(timer);
  }, [mode, query, store.searchPlugins, store.pushToast]);

  const toggle = async (plugin: PluginFile) => {
    setChanging(plugin.fileName);
    try {
      setPlugins(await store.setPluginEnabled(server.id, plugin.fileName, !plugin.enabled));
      store.pushToast({
        tone: 'success',
        title: plugin.enabled ? `${plugin.name} disabled` : `${plugin.name} enabled`,
        detail: 'The change takes effect the next time the server starts.',
      });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Plugin was not changed', detail: messageFrom(error) });
    } finally {
      setChanging(null);
    }
  };

  const addFiles = async () => {
    const selected = await openFile({
      multiple: true,
      directory: false,
      title: 'Add plugin JARs',
      filters: [{ name: 'Plugin JARs', extensions: ['jar'] }],
    });
    const paths = typeof selected === 'string' ? [selected] : selected ?? [];
    if (paths.length === 0) return;
    setAdding(true);
    try {
      setPlugins(await store.addPluginFiles(server.id, paths));
      store.pushToast({
        tone: 'success',
        title: `${paths.length} plugin${paths.length === 1 ? '' : 's'} added`,
        detail: running ? 'The files were copied. Restart the server to load them.' : 'The files were copied into this server.',
      });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Plugins were not added', detail: messageFrom(error) });
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
      setPlugins(await store.deletePlugin(server.id, target.fileName));
      store.pushToast({ tone: 'success', title: `${target.name} moved to the Recycle Bin` });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Plugin was not deleted', detail: messageFrom(error) });
    } finally {
      setChanging(null);
    }
  };

  const chooseVersion = async (project: PluginProject) => {
    setVersionTarget(project);
    setVersions([]);
    setSelectedVersionId('');
    setVersionError('');
    setVersionsLoading(true);
    try {
      const options = await store.listPluginVersions(server.id, project.namespace, project.slug);
      setVersions(options);
      setSelectedVersionId(options.find((version) => version.releaseType.toLowerCase() === 'release')?.id ?? options[0]?.id ?? '');
    } catch (error) {
      setVersionError(messageFrom(error));
    } finally {
      setVersionsLoading(false);
    }
  };

  const install = async (project: PluginProject, versionId: string) => {
    setInstalling(`${project.namespace}/${project.slug}`);
    setInstallProgress(0);
    setOperationId(null);
    try {
      const next = await store.installPlugin(server.id, project.namespace, project.slug, versionId, (event) => {
        setOperationId(event.data.operationId);
        if (event.event === 'progress') setInstallProgress(event.data.progress ?? 0);
      });
      setPlugins(next);
      store.pushToast({
        tone: 'success',
        title: `${project.name} installed`,
        detail: running
          ? 'The download is complete, but the plugin will not load until you restart the server.'
          : 'It will load the next time the server starts.',
      });
    } catch (error) {
      if ((error as { code?: string })?.code !== 'cancelled') store.pushToast({ tone: 'error', title: `${project.name} was not installed`, detail: messageFrom(error) });
    } finally {
      setInstalling(null);
      setInstallProgress(0);
      setOperationId(null);
    }
  };

  const loadMore = async () => {
    if (!catalog || !catalog.hasMore) return;
    setLoadingMore(true);
    try {
      const next = await store.searchPlugins(query, catalog.offset + catalog.projects.length);
      setCatalog({ ...next, projects: [...catalog.projects, ...next.projects] });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Could not load more plugins', detail: messageFrom(error) });
    } finally {
      setLoadingMore(false);
    }
  };

  return (
    <div className="tab plugins-tab">
      <div className="plugins-toolbar">
        <Segmented
          value={mode}
          onChange={setMode}
          options={[
            { value: 'installed', label: `Installed${plugins.length ? ` (${plugins.length})` : ''}` },
            { value: 'browse', label: 'Browse Hangar' },
          ]}
        />
        {mode === 'installed' && (
          <div className="plugin-toolbar-actions">
            <button className="btn btn-secondary btn-sm" disabled={!installable || adding} onClick={() => void addFiles()}>
              {adding ? <Spinner size={12} /> : <IconPlus size={13} />}
              Add plugins
            </button>
            <button className="btn btn-secondary btn-sm" disabled={refreshing || adding} onClick={() => void refresh(true)}>
              {refreshing ? <Spinner size={12} /> : <IconRefresh size={13} />}
              Refresh
            </button>
          </div>
        )}
        {mode === 'browse' && (
          <div className="plugin-search">
            <IconSearch size={14} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search Paper plugins" aria-label="Search Paper plugins" />
            {query && <button className="icon-btn" onClick={() => setQuery('')} aria-label="Clear search"><IconX size={12} /></button>}
          </div>
        )}
      </div>

      {!editable && (
        <div className="plugin-running-note">
          {running
            ? 'You can install plugins while the server is running, but they will not load until you restart it. Stop the server to enable, disable, or delete plugins.'
            : 'Wait for the current server operation to finish before changing plugins.'}
        </div>
      )}

      {mode === 'installed' ? (
        <div className="plugins-surface">
          {loading ? <PluginSkeleton count={4} /> : plugins.length === 0 ? (
            <EmptyState
              icon={<IconPlug size={40} />}
              title="No plugins installed"
              description="Browse trusted Paper plugins from Hangar and install a compatible release without leaving Nooki."
              action={<button className="btn btn-primary" onClick={() => setMode('browse')}>Browse plugins</button>}
            />
          ) : plugins.map((plugin) => {
            const metadata = plugin.hangar;
            return <div className={`plugin-row ${plugin.enabled ? '' : 'is-disabled'}`} key={plugin.fileName}>
              {metadata ? <PluginLogo project={metadata} /> : <div className="plugin-mark"><IconPlug size={18} /></div>}
              <div className="plugin-main">
                <div className="plugin-name-line">
                  <span className="plugin-name">{metadata?.name || plugin.name}</span>
                  {(metadata?.version || plugin.version) && <span className="plugin-version">v{metadata?.version || plugin.version}</span>}
                  {metadata && <span className="plugin-source-tag">Hangar</span>}
                  {!plugin.enabled && <span className="plugin-disabled-tag">disabled</span>}
                </div>
                <span className="plugin-description">{metadata?.description || plugin.description || plugin.fileName}</span>
                <span className="plugin-meta">
                  {metadata ? `By ${metadata.author} · ` : plugin.authors.length ? `By ${plugin.authors.join(', ')} · ` : ''}{formatBytes(plugin.size)} · changed {formatRelative(plugin.modifiedAt)}
                </span>
              </div>
              <div className="plugin-actions">
                {changing === plugin.fileName && <Spinner size={13} />}
                <button
                  type="button"
                  role="switch"
                  aria-checked={plugin.enabled}
                  aria-label={`${plugin.enabled ? 'Disable' : 'Enable'} ${plugin.name}`}
                  className={`switch ${plugin.enabled ? 'on' : ''}`}
                  disabled={!editable || changing !== null}
                  onClick={() => void toggle(plugin)}
                >
                  <span className="switch-knob" />
                </button>
                <button className="icon-btn danger-text" disabled={!editable || changing !== null} onClick={() => setDeleteTarget(plugin)} aria-label={`Delete ${plugin.name}`} title="Move to Recycle Bin">
                  <IconTrash size={14} />
                </button>
              </div>
            </div>;
          })}
        </div>
      ) : (
        <div className="plugins-surface plugin-browser" aria-busy={searching}>
          {searching ? <PluginSkeleton count={6} /> : catalog?.projects.length === 0 ? (
            <EmptyState icon={<IconSearch size={40} />} title="No matching plugins" description="Try a broader name or keyword." />
          ) : catalog?.projects.map((project) => {
            const projectId = `${project.namespace}/${project.slug}`;
            const busy = installing === projectId;
            const installed = plugins.some((plugin) => plugin.hangar?.projectId === project.projectId);
            return (
              <div className="plugin-row plugin-project-row" key={projectId}>
                <PluginLogo project={project} />
                <div className="plugin-main">
                  <div className="plugin-name-line">
                    <span className="plugin-name">{project.name}</span>
                    <span className="plugin-author">by {project.author}</span>
                  </div>
                  <span className="plugin-description">{project.description}</span>
                  <span className="plugin-meta">{compactNumber(project.downloads)} downloads · {compactNumber(project.stars)} stars · updated {formatRelative(project.lastUpdated)}</span>
                  {busy && <div className="plugin-install-progress"><span style={{ width: `${installProgress}%` }} /></div>}
                </div>
                <button className={`btn btn-sm ${busy ? 'btn-secondary' : installed ? 'btn-secondary plugin-installed-btn' : 'btn-primary'}`} disabled={installed || !installable || (installing !== null && !busy) || (busy && !operationId)} onClick={() => busy && operationId ? void store.cancelOperation(operationId) : void chooseVersion(project)}>
                  {busy ? <IconX size={12} /> : installed ? <IconCheck size={13} /> : <IconDownload size={13} />}
                  {busy ? 'Cancel' : installed ? 'Installed' : 'Install'}
                </button>
              </div>
            );
          })}
          {loadingMore && <PluginSkeleton count={3} />}
          {catalog?.hasMore && !loadingMore && (
            <div className="plugin-load-more">
              <button className="btn btn-secondary" disabled={loadingMore} onClick={() => void loadMore()}>
                {loadingMore && <Spinner size={12} />} Load more
              </button>
            </div>
          )}
        </div>
      )}

      <ConfirmDialog
        open={deleteTarget !== null}
        title={`Delete ${deleteTarget?.name ?? 'plugin'}?`}
        description="The plugin JAR will be moved to the Windows Recycle Bin. Its configuration folder will be kept."
        confirmLabel="Delete plugin"
        tone="danger"
        notes={deleteTarget ? [deleteTarget.fileName, 'The change takes effect the next time the server starts.'] : undefined}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => void remove()}
      />
      <AddonVersionDialog
        open={versionTarget !== null}
        projectName={versionTarget?.name ?? 'plugin'}
        kind="plugin"
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
    </div>
  );
}

function PluginSkeleton({ count }: { count: number }) {
  return <>{Array.from({ length: count }, (_, index) => (
    <div className="plugin-row plugin-skeleton" key={index}>
      <span className="skeleton-block skeleton-icon" />
      <span className="skeleton-copy"><i /><i /></span>
      <span className="skeleton-block skeleton-button" />
    </div>
  ))}</>;
}

function PluginLogo({ project }: { project: Pick<PluginProject, 'projectId' | 'name'> }) {
  const store = useStore();
  const [source, setSource] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let active = true;
    setSource(null);
    setLoaded(false);
    void store.loadPluginIcon(project.projectId)
      .then((icon) => {
        if (!active) return;
        setSource(icon);
        if (!icon) setLoaded(true);
      })
      .catch(() => { if (active) setLoaded(true); });
    return () => { active = false; };
  }, [project.projectId, store.loadPluginIcon]);

  return (
    <div className={`plugin-mark plugin-project-mark ${loaded ? 'is-loaded' : ''}`}>
      {!loaded && <span className="plugin-logo-skeleton" />}
      {source ? (
        <img
          src={source}
          alt=""
          onLoad={() => setLoaded(true)}
          onError={() => { setSource(null); setLoaded(true); }}
        />
      ) : loaded ? project.name.slice(0, 1).toUpperCase() : null}
    </div>
  );
}
