import { useEffect, useMemo, useState } from 'react';
import { open as openFile } from '@tauri-apps/plugin-dialog';
import { useStore } from '../state/store';
import { Field, FolderPicker, Modal, ProgressBar, Select, Stepper, Callout, Toggle } from '../components/ui';
import { IconBox, IconCheck, IconFolder, IconPlus, IconSearch } from '../components/Icons';
import { SoftwareIcon } from '../components/ServerIcon';
import ModpackWizard from './ModpackWizard';
import { formatMegabytes, softwareLabel } from '../format';
import type { ImportScan, OperationEvent, Server, ServerType, VersionOption } from '../types';
import './AddServerWizard.css';

type Mode = 'choose' | 'create' | 'modpack' | 'import';

const createSteps = ['Basics', 'Software', 'Resources', 'Location', 'Review'];
const importSteps = ['Folder', 'Detected', 'Review'];

const softwareTypes: ServerType[] = ['vanilla', 'paper', 'forge', 'neoforge', 'fabric'];

const softwareDescriptions: Record<ServerType, string> = {
  vanilla: 'The official Mojang server',
  paper: 'Optimized server with plugin support',
  forge: 'The established mod loader and ecosystem',
  neoforge: 'A modern continuation of the Forge ecosystem',
  fabric: 'A lightweight, modern mod loader',
};

function versionDetail(type: ServerType, version: VersionOption) {
  if (type === 'paper' && version.build && version.build !== 'release') return `Paper build ${version.build}`;
  if (type === 'forge') return `Forge ${version.build} · ${version.releaseType}`;
  if (type === 'neoforge') return `NeoForge ${version.build} · ${version.releaseType}`;
  if (type === 'fabric') return `Fabric Loader ${version.build}`;
  return version.releaseType === 'release' ? 'Stable release' : version.releaseType;
}

interface Draft {
  name: string;
  type: ServerType;
  version: string;
  maxMemory: number;
  minMemory: number;
  port: number;
  folder: string;
  eula: boolean;
  build: string;
  jarPath: string;
  experimental: boolean;
  iconData: string | null;
}

const emptyDraft: Draft = {
  name: '',
  type: 'paper',
  version: '',
  maxMemory: 4096,
  minMemory: 1024,
  port: 25565,
  folder: 'D:\\Minecraft',
  eula: false,
  build: '',
  jarPath: '',
  experimental: false,
  iconData: null,
};

function ServerIconPicker({ type, value, onChange }: { type: ServerType; value: string | null; onChange: (value: string | null) => void }) {
  const store = useStore();
  const [loading, setLoading] = useState(false);

  const chooseIcon = async () => {
    const selected = await openFile({
      multiple: false,
      directory: false,
      filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
    });
    if (typeof selected !== 'string') return;
    setLoading(true);
    try {
      onChange(await store.loadServerIcon(selected));
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Icon was not selected', detail: String((error as { message?: string })?.message ?? error) });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="server-icon-choice">
      <span className={`server-icon-choice-preview ${value ? 'is-custom' : ''}`}>
        {value ? <img src={value} alt="Custom server icon preview" /> : <SoftwareIcon type={type} size={30} />}
      </span>
      <div className="server-icon-choice-copy">
        <strong>Server icon</strong>
        <span>{value ? 'Using your custom image' : `Using the ${softwareLabel(type)} icon`}</span>
      </div>
      {value && <button type="button" className="btn btn-sm btn-ghost" disabled={loading} onClick={() => onChange(null)}>Use default</button>}
      <button type="button" className="btn btn-sm btn-secondary" disabled={loading} onClick={() => void chooseIcon()}>
        {loading ? 'Loading…' : value ? 'Change' : 'Choose icon'}
      </button>
    </div>
  );
}

function SoftwareVersionPicker({
  serverType,
  value,
  versions,
  loading,
  error,
  onServerTypeChange,
  onChange,
  onRetry,
  onChooseModpack,
}: {
  serverType: ServerType;
  value: string;
  versions: VersionOption[];
  loading: boolean;
  error: string;
  onServerTypeChange: (type: ServerType) => void;
  onChange: (version: VersionOption) => void;
  onRetry: () => void;
  onChooseModpack: () => void;
}) {
  const [query, setQuery] = useState('');
  const selected = versions.find((version) => `${version.version}:${version.build}` === value);
  const visible = versions.filter((version) => {
    const search = query.trim().toLowerCase();
    return !search || version.version.toLowerCase().includes(search) || version.build.toLowerCase().includes(search);
  });

  useEffect(() => setQuery(''), [serverType]);

  return (
    <div className="version-picker">
      <div className="version-picker-toolbar">
        <div className="version-picker-selection">
          <span className="version-selection-swap" key={serverType}>
            <span className={`version-product-icon is-${serverType}`}>
              <SoftwareIcon type={serverType} size={21} />
            </span>
            <span className="version-trigger-copy">
              <span className="version-trigger-title">{softwareLabel(serverType)}</span>
              <span className="version-trigger-value" key={`${serverType}-${loading ? 'loading' : value || 'empty'}`}>{loading ? 'Loading versions…' : selected ? `${selected.version}${selected.build && selected.build !== 'release' ? ` build ${selected.build}` : ''}` : 'Choose a version'}</span>
            </span>
          </span>
          {loading && <span className="spinner version-trigger-spinner" aria-hidden="true" />}
        </div>
        <div className="version-picker-search">
            <IconSearch size={14} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search versions…" aria-label="Search Minecraft versions" />
        </div>
      </div>
      <div className="version-picker-content">
        <div className="version-product-rail" role="tablist" aria-label="Server software">
          {softwareTypes.map((type) => (
            <button type="button" role="tab" key={type} aria-selected={serverType === type} aria-label={softwareLabel(type)} title={softwareLabel(type)} className={serverType === type ? 'active' : ''} onClick={() => onServerTypeChange(type)}>
              <SoftwareIcon type={type} />
            </button>
          ))}
          <span className="version-product-rail-separator" />
          <button type="button" role="tab" aria-selected="false" aria-label="Modpacks" title="Modpacks" onClick={onChooseModpack}>
            <IconBox size={20} />
          </button>
        </div>
        <div className="version-results" role="listbox" aria-label={`${softwareLabel(serverType)} versions`}>
          <div className="version-results-head version-switch-copy" key={`head-${serverType}`}>
            <div><strong>{softwareLabel(serverType)}</strong><span>{softwareDescriptions[serverType]}</span></div>
            {!loading && !error && <span>{visible.length} version{visible.length === 1 ? '' : 's'}</span>}
          </div>
          <div className="version-results-body" key={`${serverType}-${loading ? 'loading' : error ? 'error' : 'ready'}`}>
          {loading ? (
            <div className="version-loading" aria-label="Loading versions">
              {[0, 1, 2, 3, 4].map((row) => <div className="version-skeleton" key={row}><span /><span /></div>)}
            </div>
          ) : error ? (
            <div className="version-message"><strong>Versions could not be loaded</strong><span>{error}</span><button type="button" className="btn btn-sm btn-secondary" onClick={onRetry}>Retry</button></div>
          ) : visible.length === 0 ? (
            <div className="version-message"><strong>No versions found</strong><span>Try a different search.</span></div>
          ) : visible.map((version, index) => {
            const active = `${version.version}:${version.build}` === value;
            return (
              <button key={version.id} type="button" role="option" aria-selected={active} className={`version-result ${active ? 'active' : ''}`} onClick={() => onChange(version)}>
                <span className="version-result-main"><strong>{version.version}</strong><span>{versionDetail(serverType, version)}</span></span>
                <span className="version-result-tags">
                  {index === 0 && !query && <span className="version-tag is-latest">Latest</span>}
                  {version.experimental && <span className="version-tag is-experimental">Experimental</span>}
                  {active && <IconCheck size={13} className="version-result-check" />}
                </span>
              </button>
            );
          })}
          </div>
        </div>
      </div>
    </div>
  );
}

export default function AddServerWizard({ onClose }: { onClose: () => void }) {
  const store = useStore();
  const [mode, setMode] = useState<Mode>('choose');
  const [modpackBackMode, setModpackBackMode] = useState<'choose' | 'create'>('choose');
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState<Draft>(() => ({ ...emptyDraft, folder: store.settings.serverFolder }));
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const [phase, setPhase] = useState<'form' | 'working' | 'done' | 'failed'>('form');
  const [progress, setProgress] = useState(0);
  const [operationId, setOperationId] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [wasCancelled, setWasCancelled] = useState(false);
  const [workMessage, setWorkMessage] = useState('');
  const [createdServer, setCreatedServer] = useState<Server | null>(null);
  const [versions, setVersions] = useState<VersionOption[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState('');
  const [includeExperimental, setIncludeExperimental] = useState(false);
  const [catalogRetry, setCatalogRetry] = useState(0);

  /* import-specific state */
  const [importFolder, setImportFolder] = useState('');
  const [scanState, setScanState] = useState<'idle' | 'scanning' | 'found' | 'unclear' | 'invalid'>('idle');
  const [scanResult, setScanResult] = useState<ImportScan | null>(null);

  useEffect(() => {
    if (mode !== 'create') return;
    let cancelled = false;
    setCatalogLoading(true);
    setCatalogError('');
    store.listVersions(draft.type, includeExperimental).then((catalog) => {
      if (cancelled) return;
      setVersions(catalog.versions);
      const selected = catalog.versions.find((version) => version.version === draft.version) ?? catalog.versions[0];
      if (selected) patch({ version: selected.version, build: selected.build, experimental: selected.experimental });
    }).catch((error) => {
      if (!cancelled) setCatalogError(error instanceof Error ? error.message : String((error as { message?: string })?.message ?? error));
    }).finally(() => { if (!cancelled) setCatalogLoading(false); });
    return () => { cancelled = true; };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, draft.type, includeExperimental, catalogRetry]);

  const errors = useMemo(() => {
    const e: Record<string, string> = {};
    const name = draft.name.trim();
    if (!name) e.name = 'Give your server a name so you can tell them apart.';
    else if (name.length < 3) e.name = 'Use at least 3 characters.';
    else if (store.servers.some((s) => s.name.toLowerCase() === name.toLowerCase()))
      e.name = 'You already have a server with this name.';

    if (!Number.isFinite(draft.port)) e.port = 'Enter a port number.';
    else if (draft.port < 1024 || draft.port > 65535) e.port = 'Pick a port between 1024 and 65535.';

    if (draft.minMemory >= draft.maxMemory) e.memory = 'Minimum memory must be lower than the maximum.';
    if (draft.maxMemory > 12288) e.memory = 'That is more memory than this computer can spare.';

    if (!draft.folder.trim()) e.folder = 'Choose where the server files should live.';
    if (!draft.eula) e.eula = 'Minecraft requires accepting the EULA before a server can run.';
    return e;
  }, [draft, store.servers]);

  const stepValid = (index: number): boolean => {
    if (mode === 'create') {
      if (index === 0) return !errors.name;
      if (index === 1) return Boolean(draft.version) && !catalogLoading;
      if (index === 2) return !errors.memory;
      if (index === 3) return !errors.folder && !errors.port;
      if (index === 4) return !errors.eula;
    }
    if (mode === 'import') {
      if (index === 0) return scanState === 'found' || scanState === 'unclear';
      if (index === 1) return !errors.name && !errors.port && Boolean(draft.version && draft.jarPath);
      if (index === 2) return !errors.eula;
    }
    return true;
  };

  const patch = (p: Partial<Draft>) => setDraft((prev) => ({ ...prev, ...p }));
  const markTouched = (key: string) => setTouched((prev) => ({ ...prev, [key]: true }));
  const show = (key: string) => (touched[key] ? errors[key] : undefined);

  const scanFolder = (folder: string) => {
    setImportFolder(folder);
    setScanState('scanning');
    setScanResult(null);
    void store.scanServerFolder(folder).then((scan) => {
      setScanResult(scan);
      if (!scan.valid) { setScanState('invalid'); return; }
      const candidate = scan.candidates[0];
      setScanState(scan.warnings.length > 0 || scan.candidates.length !== 1 ? 'unclear' : 'found');
      patch({
        name: scan.detectedName || 'Imported Server',
        type: scan.detectedType ?? 'vanilla',
        version: scan.detectedVersion ?? '',
        build: candidate?.build ?? '',
        jarPath: candidate?.path ?? '',
        port: scan.port ?? 25565,
        folder,
        eula: scan.eulaAccepted,
      });
    }).catch((error) => {
      setScanState('invalid');
      setWorkMessage(String((error as { message?: string })?.message ?? error));
    });
  };

  const finish = async () => {
    setPhase('working');
    setProgress(0);
    setOperationId(null);
    setCancelling(false);
    setWasCancelled(false);
    setWorkMessage(mode === 'create' ? 'Preparing the server' : 'Reading the server folder');
    const onProgress = (event: OperationEvent) => {
      setOperationId(event.data.operationId);
      setProgress(event.data.progress ?? 0);
      setWorkMessage(event.data.message);
    };
    try {
      const created = mode === 'create'
        ? await store.createServer({
            name: draft.name.trim(), type: draft.type, version: draft.version, build: draft.build || null,
            minMemory: draft.minMemory, maxMemory: draft.maxMemory, port: draft.port,
            parentFolder: draft.folder, eula: draft.eula, experimental: draft.experimental, iconData: draft.iconData,
          }, onProgress)
        : await store.importServer({
            name: draft.name.trim(), folder: draft.folder, jarPath: draft.jarPath, type: draft.type,
            version: draft.version, build: draft.build, minMemory: draft.minMemory,
            maxMemory: draft.maxMemory, port: draft.port, eula: draft.eula, iconData: draft.iconData,
          }, onProgress);
      const server: Server = created;
      setCreatedServer(server);
      setPhase('done');
      setProgress(100);
      store.pushToast({
        tone: 'success',
        title: mode === 'create' ? `${server.name} is ready` : `${server.name} imported`,
        detail: 'Start it whenever you are ready.',
      });
    } catch (error) {
      const cancelled = typeof error === 'object' && error !== null && 'code' in error && (error as { code?: string }).code === 'cancelled';
      setWasCancelled(cancelled);
      setPhase('failed');
      setWorkMessage(cancelled ? 'No server was added. Any temporary download files were discarded.' : String((error as { message?: string })?.message ?? error));
    }
  };

  const cancelSetup = async () => {
    if (!operationId || cancelling) return;
    setCancelling(true);
    setWorkMessage('Cancelling and cleaning up temporary files…');
    try { await store.cancelOperation(operationId); } catch { setCancelling(false); }
  };

  const steps = mode === 'create' ? createSteps : importSteps;
  const isLast = step === steps.length - 1;

  /* ----------------------------- choose ----------------------------- */
  if (mode === 'choose') {
    return (
      <Modal open onClose={onClose} title="Add a server" description="Start fresh, install a modpack, or bring in a server you already have." width={700}>
        <div className="choice-grid">
          <button className="choice" onClick={() => setMode('create')}>
            <span className="choice-icon">
              <IconPlus size={18} />
            </span>
            <span className="choice-title">Create a new server</span>
            <span className="choice-desc">
              Nooki downloads the server software, sets sensible defaults, and gets you a world in a few clicks.
            </span>
          </button>
          <button className="choice" onClick={() => { setModpackBackMode('choose'); setMode('modpack'); }}>
            <span className="choice-icon">
              <IconBox size={18} />
            </span>
            <span className="choice-title">Install a modpack</span>
            <span className="choice-desc">
              Browse Modrinth or CurseForge. Nooki installs the pack, matching loader, Java, and server files.
            </span>
          </button>
          <button className="choice" onClick={() => setMode('import')}>
            <span className="choice-icon">
              <IconFolder size={18} />
            </span>
            <span className="choice-title">Import an existing server</span>
            <span className="choice-desc">
              Point Nooki at a folder you already run a server from. Your world and settings stay exactly as they are.
            </span>
          </button>
        </div>
      </Modal>
    );
  }

  if (mode === 'modpack') {
    return <ModpackWizard onClose={onClose} onBack={() => setMode(modpackBackMode)} initialName={draft.name} />;
  }

  /* ---------------------------- progress ---------------------------- */
  if (phase !== 'form') {
    return (
      <Modal
        open
        onClose={phase === 'working' ? () => {} : onClose}
        dismissable={phase !== 'working'}
        title={
          phase === 'working'
            ? mode === 'create'
              ? 'Setting up your server'
              : 'Importing your server'
            : phase === 'done'
              ? 'All set'
              : wasCancelled ? 'Setup cancelled' : 'Something went wrong'
        }
        description={phase === 'done' ? `${draft.name.trim()} is in your server list.` : undefined}
        width={460}
        footer={
          phase === 'working' ? (
            <>
              <span className="text-muted text-sm">Temporary files are removed when cancelled</span>
              <button className="btn btn-secondary" disabled={!operationId || cancelling} onClick={() => void cancelSetup()}>
                {cancelling ? 'Cancelling…' : 'Cancel setup'}
              </button>
            </>
          ) : phase === 'done' ? (
            <>
              <button className="btn btn-secondary" onClick={onClose}>
                Close
              </button>
              <button
                className="btn btn-primary"
                data-autofocus
                onClick={() => {
                  onClose();
                  if (createdServer) store.openServer(createdServer.id);
                }}
              >
                Open server
              </button>
            </>
          ) : (
            <>
              <button className="btn btn-secondary" onClick={onClose}>
                Cancel
              </button>
              <button className="btn btn-primary" data-autofocus onClick={() => setPhase('form')}>
                Back to the form
              </button>
            </>
          )
        }
      >
        {phase === 'working' && (
          <div className="work">
            <ProgressBar value={progress} tone="accent" />
            <p className="work-msg">{workMessage || 'Getting started'}</p>
          </div>
        )}
        {phase === 'done' && (
          <div className="done-panel">
            <span className="done-check">
              <IconCheck size={18} />
            </span>
            <div className="done-summary">
              <SummaryRow label="Name" value={draft.name.trim()} />
              <SummaryRow label="Software" value={`${softwareLabel(draft.type)} ${draft.version}`} />
              <SummaryRow label="Address" value={`localhost:${draft.port}`} mono />
            </div>
          </div>
        )}
        {phase === 'failed' && (
          <Callout tone={wasCancelled ? 'info' : 'error'} title={wasCancelled ? 'Setup cancelled' : 'Setup did not finish'}>
            {workMessage}
          </Callout>
        )}
      </Modal>
    );
  }

  /* ------------------------------ form ------------------------------ */
  return (
    <Modal
      open
      onClose={onClose}
      title={mode === 'create' ? 'Create a new server' : 'Import an existing server'}
      width={560}
      footer={
        <>
          <button
            className="btn btn-ghost"
            onClick={() => {
              if (step === 0) {
                setMode('choose');
                setDraft(emptyDraft);
                setScanState('idle');
                setStep(0);
              } else setStep(step - 1);
            }}
          >
            Back
          </button>
          <div className="foot-spacer" />
          <button className="btn btn-secondary" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn btn-primary"
            disabled={!stepValid(step)}
            onClick={() => {
              if (isLast) finish();
              else setStep(step + 1);
            }}
          >
            {isLast ? (mode === 'create' ? 'Create server' : 'Import server') : 'Continue'}
          </button>
        </>
      }
    >
      <div className="wizard">
        <Stepper steps={steps} current={step} />

        {mode === 'create' && (
          <>
            {step === 0 && (
              <div className="wizard-panel">
                <Field label="Server name" error={show('name')} hint="This is only shown inside Nooki.">
                  <input
                    className="input"
                    value={draft.name}
                    placeholder="Sunday Survival"
                    onChange={(e) => patch({ name: e.target.value })}
                    onBlur={() => markTouched('name')}
                  />
                </Field>
                <ServerIconPicker type={draft.type} value={draft.iconData} onChange={(iconData) => patch({ iconData })} />
                <Callout tone="info" title="Nooki keeps each server in its own folder">
                  Worlds, settings, and backups stay separate, so nothing you do to one server affects the others.
                </Callout>
              </div>
            )}

            {step === 1 && (
              <div className="wizard-panel">
                <Field label="Software and Minecraft version" hint="Choose server software from the icon rail, then pick the version your players use.">
                  <SoftwareVersionPicker
                    serverType={draft.type}
                    value={`${draft.version}:${draft.build}`}
                    versions={versions}
                    loading={catalogLoading}
                    error={catalogError}
                    onServerTypeChange={(type) => {
                      if (type === draft.type) return;
                      setCatalogLoading(true);
                      setCatalogError('');
                      setVersions([]);
                      patch({ type, version: '', build: '', experimental: false });
                    }}
                    onChange={(version) => patch({ version: version.version, build: version.build, experimental: version.experimental })}
                    onRetry={() => setCatalogRetry((value) => value + 1)}
                    onChooseModpack={() => { setModpackBackMode('create'); setMode('modpack'); }}
                  />
                </Field>
                <Toggle
                  checked={includeExperimental}
                  onChange={(checked) => { setIncludeExperimental(checked); patch({ experimental: checked ? draft.experimental : false }); }}
                  label="Show snapshots and experimental builds"
                  hint="Snapshots and unstable builds are hidden by default."
                />
              </div>
            )}

            {step === 2 && (
              <div className="wizard-panel">
                <Field
                  label="Memory for this server"
                  error={show('memory')}
                  hint={`This computer has ${formatMegabytes(store.host.totalMemory)} in total. Leave some for Windows.`}
                >
                  <div className="mem-row">
                    <input
                      type="range"
                      min={1024}
                      max={12288}
                      step={512}
                      value={draft.maxMemory}
                      onChange={(e) => patch({ maxMemory: Number(e.target.value) })}
                      onBlur={() => markTouched('memory')}
                      className="range"
                    />
                    <span className="mem-value">{formatMegabytes(draft.maxMemory)}</span>
                  </div>
                </Field>
                <div className="mem-presets">
                  {[2048, 4096, 6144, 8192].map((mb) => (
                    <button
                      key={mb}
                      className={`preset ${draft.maxMemory === mb ? 'active' : ''}`}
                      onClick={() => patch({ maxMemory: mb, minMemory: Math.min(draft.minMemory, mb / 2) })}
                    >
                      {formatMegabytes(mb)}
                      <span>{mb <= 2048 ? '1-4 players' : mb <= 4096 ? '5-10 players' : 'mods, plugins, or more players'}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}

            {step === 3 && (
              <div className="wizard-panel">
                <Field label="Server folder" error={show('folder')} hint="Nooki creates a subfolder with the server name.">
                  <FolderPicker value={draft.folder} onChange={(folder) => patch({ folder })} />
                </Field>
                <Field
                  label="Port"
                  error={show('port')}
                  hint="Stopped servers can share a port; only one of them can run at a time."
                >
                  <input
                    className="input mono"
                    type="number"
                    value={draft.port}
                    onChange={(e) => patch({ port: Number(e.target.value) })}
                    onBlur={() => markTouched('port')}
                  />
                </Field>
              </div>
            )}

            {step === 4 && (
              <div className="wizard-panel">
                <div className="review">
                  <div className="review-icon">
                    {draft.iconData ? <img className="review-server-icon" src={draft.iconData} alt="" /> : <SoftwareIcon type={draft.type} size={40} />}
                  </div>
                  <div className="review-rows">
                    <SummaryRow label="Name" value={draft.name.trim() || '—'} />
                    <SummaryRow label="Software" value={`${softwareLabel(draft.type)} ${draft.version}`} />
                    <SummaryRow label="Memory" value={formatMegabytes(draft.maxMemory)} />
                    <SummaryRow label="Port" value={String(draft.port)} mono />
                    <SummaryRow label="Folder" value={`${draft.folder}\\${draft.name.trim() || 'New Server'}`} mono />
                  </div>
                </div>
                <div className="eula">
                  <Toggle
                    checked={draft.eula}
                    onChange={(eula) => {
                      patch({ eula });
                      markTouched('eula');
                    }}
                    label="I accept the Minecraft End User Licence Agreement"
                    hint="Mojang requires this before any server can start."
                    error={show('eula') ? errors.eula : undefined}
                  />
                </div>
              </div>
            )}
          </>
        )}

        {mode === 'import' && (
          <>
            {step === 0 && (
              <div className="wizard-panel">
                <Field label="Where is the server?" hint="Pick the folder that contains server.properties.">
                  <FolderPicker
                    value={importFolder}
                    onChange={scanFolder}
                  />
                </Field>

                {scanState === 'scanning' && (
                  <div className="scan">
                    <span className="spinner" style={{ width: 14, height: 14 }} />
                    <span>Looking for server files</span>
                  </div>
                )}
                {scanState === 'found' && (
                  <Callout tone="success" title="Minecraft server found">
                    Nooki found the configuration and a server launch file in this folder.
                  </Callout>
                )}
                {scanState === 'unclear' && (
                  <Callout tone="warning" title="Found a server, but some details are unclear">
                    {scanResult?.warnings.join(' ') || 'Choose the correct jar and version on the next step.'}
                  </Callout>
                )}
                {scanState === 'invalid' && (
                  <Callout tone="error" title="No Minecraft server here">
                    This folder has no server.properties or supported launch file. Pick the folder you normally start the server from.
                  </Callout>
                )}
              </div>
            )}

            {step === 1 && (
              <div className="wizard-panel">
                {scanState === 'unclear' && (
                  <Callout tone="warning" title="Please double-check these">
                    The highlighted fields were guessed from the folder contents.
                  </Callout>
                )}
                <Field label="Server name" error={show('name')}>
                  <input
                    className="input"
                    value={draft.name}
                    onChange={(e) => patch({ name: e.target.value })}
                    onBlur={() => markTouched('name')}
                  />
                </Field>
                <ServerIconPicker type={draft.type} value={draft.iconData} onChange={(iconData) => patch({ iconData })} />
                <div className="two-col">
                  <Field label="Software">
                    <Select
                      value={draft.type}
                      onChange={(type) => patch({ type: type as ServerType })}
                      options={[
                        { value: 'vanilla', label: 'Vanilla' },
                        { value: 'paper', label: 'Paper' },
                        { value: 'forge', label: 'Forge' },
                        { value: 'neoforge', label: 'NeoForge' },
                        { value: 'fabric', label: 'Fabric' },
                      ]}
                    />
                  </Field>
                  <Field label="Minecraft version" hint={scanState === 'unclear' ? 'Detected loosely, please confirm' : undefined}>
                    <input className="input" value={draft.version} placeholder="1.21.8" onChange={(e) => patch({ version: e.target.value })} />
                  </Field>
                </div>
                {(scanResult?.candidates.length ?? 0) > 1 && (
                  <Field label="Server launch file" hint="Choose the JAR or generated Forge argument file Nooki should launch.">
                    <Select value={draft.jarPath} options={(scanResult?.candidates ?? []).map((candidate) => ({ value: candidate.path, label: candidate.fileName }))} onChange={(value) => {
                      const candidate = scanResult?.candidates.find((item) => item.path === value);
                      patch({ jarPath: value, type: candidate?.serverType ?? draft.type, version: candidate?.version ?? draft.version, build: candidate?.build ?? draft.build });
                    }} />
                  </Field>
                )}
                <div className="two-col">
                  <Field label="Port" error={show('port')} hint="Stopped servers can share a port; only one can run at a time.">
                    <input
                      className="input mono"
                      type="number"
                      value={draft.port}
                      onChange={(e) => patch({ port: Number(e.target.value) })}
                      onBlur={() => markTouched('port')}
                    />
                  </Field>
                  <Field label="Memory" error={show('memory')}>
                    <Select
                      value={String(draft.maxMemory)}
                      options={[2048, 4096, 6144, 8192].map((mb) => ({ value: String(mb), label: formatMegabytes(mb) }))}
                      onChange={(value) => patch({ maxMemory: Number(value) })}
                    />
                  </Field>
                </div>
              </div>
            )}

            {step === 2 && (
              <div className="wizard-panel">
                <div className="review">
                  <div className="review-icon">
                    <IconFolder size={30} />
                  </div>
                  <div className="review-rows">
                    <SummaryRow label="Name" value={draft.name.trim() || '—'} />
                    <SummaryRow label="Software" value={`${softwareLabel(draft.type)} ${draft.version}`} />
                    <SummaryRow label="Port" value={String(draft.port)} mono />
                    <SummaryRow label="Folder" value={draft.folder} mono />
                  </div>
                </div>
                <Callout tone="info" title="Nothing is moved or overwritten">
                  Your world stays where it is. Nooki only reads the folder and manages the server process for you.
                </Callout>
                <div className="eula">
                  <Toggle
                    checked={draft.eula}
                    onChange={(eula) => {
                      patch({ eula });
                      markTouched('eula');
                    }}
                    label="I accept the Minecraft End User Licence Agreement"
                    error={show('eula') ? errors.eula : undefined}
                  />
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </Modal>
  );
}

function SummaryRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="summary-row">
      <span className="summary-label">{label}</span>
      <span className={`summary-value ${mono ? 'mono' : ''}`}>{value}</span>
    </div>
  );
}
