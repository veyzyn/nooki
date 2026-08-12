import { useEffect, useMemo, useRef, useState } from 'react';
import { useStore } from '../state/store';
import type { ModpackCatalog, ModpackProject, ModpackVersionOption, ModProvider, OperationEvent, Server } from '../types';
import { Callout, Field, FolderPicker, Modal, ProgressBar, Stepper, Toggle } from '../components/ui';
import { IconArrowLeft, IconBox, IconCheck, IconSearch } from '../components/Icons';
import { formatBytes, formatMegabytes } from '../format';
import modrinthLogo from '../assets/modrinth-logo.svg';
import curseforgeLogo from '../assets/curseforge-logo.svg';
import './ModpackWizard.css';

const steps = ['Modpack', 'Resources', 'Location', 'Review'];
const installStages = ['Pack', 'Files', 'Minecraft', 'Configure'];

function installStageForPhase(phase: string) {
  if (['resolve', 'manifest', 'verifyManifest'].includes(phase)) return 0;
  if (['files', 'serverPack', 'verifyPack', 'extract', 'overrides', 'prepared'].includes(phase)) return 1;
  if (['server', 'java', 'download', 'install'].includes(phase)) return 2;
  if (['modpack', 'configure', 'finalize', 'done'].includes(phase)) return 3;
  return 0;
}

function errorMessage(error: unknown) {
  if (typeof error === 'object' && error && 'message' in error) return String((error as { message: unknown }).message);
  return String(error);
}

function compactNumber(value: number) {
  return new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(value);
}

function PackLogo({ project }: { project: ModpackProject }) {
  const store = useStore();
  const [source, setSource] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    setSource(null);
    if (project.iconUrl) void store.loadModIcon(project.provider, project.iconUrl).then((value) => {
      if (active) setSource(value);
    });
    return () => { active = false; };
  }, [project.iconUrl, project.provider, store.loadModIcon]);
  return <span className="modpack-logo">{source ? <img src={source} alt="" /> : <IconBox size={18} />}</span>;
}

export default function ModpackWizard({ onClose, onBack, initialName = '' }: { onClose: () => void; onBack: () => void; initialName?: string }) {
  const store = useStore();
  const [step, setStep] = useState(0);
  const [provider, setProvider] = useState<ModProvider>('modrinth');
  const [query, setQuery] = useState('');
  const [catalog, setCatalog] = useState<ModpackCatalog | null>(null);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<ModpackProject | null>(null);
  const [versions, setVersions] = useState<ModpackVersionOption[]>([]);
  const [versionId, setVersionId] = useState('');
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [name, setName] = useState(initialName);
  const [maxMemory, setMaxMemory] = useState(6144);
  const [parentFolder, setParentFolder] = useState(store.settings.serverFolder);
  const [port, setPort] = useState(25565);
  const [eula, setEula] = useState(false);
  const [phase, setPhase] = useState<'form' | 'working' | 'done' | 'failed'>('form');
  const [progress, setProgress] = useState(0);
  const [message, setMessage] = useState('');
  const [operationPhase, setOperationPhase] = useState('resolve');
  const [operationId, setOperationId] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [wasCancelled, setWasCancelled] = useState(false);
  const [server, setServer] = useState<Server | null>(null);
  const searchSequence = useRef(0);

  useEffect(() => {
    if (selected) return;
    const sequence = ++searchSequence.current;
    const timer = window.setTimeout(() => {
      setLoading(true);
      void store.searchModpacks(provider, query, 0)
        .then((result) => { if (sequence === searchSequence.current) setCatalog(result); })
        .catch((error) => {
          if (sequence === searchSequence.current) {
            setCatalog(null);
            setMessage(errorMessage(error));
          }
        })
        .finally(() => { if (sequence === searchSequence.current) setLoading(false); });
    }, query ? 250 : 0);
    return () => window.clearTimeout(timer);
  }, [provider, query, selected, store.searchModpacks]);

  const chooseProject = (project: ModpackProject) => {
    setSelected(project);
    if (!name.trim()) setName(project.name.slice(0, 80));
    setVersions([]);
    setVersionId('');
    setVersionsLoading(true);
    setMessage('');
    void store.listModpackVersions(project.provider, project.projectId)
      .then((options) => {
        setVersions(options);
        const automatic = options.find((option) => option.automatic && option.releaseType === 'release')
          ?? options.find((option) => option.automatic)
          ?? options[0];
        setVersionId(automatic?.id ?? '');
      })
      .catch((error) => setMessage(errorMessage(error)))
      .finally(() => setVersionsLoading(false));
  };

  const chosenVersion = versions.find((version) => version.id === versionId) ?? null;
  const nameError = !name.trim() ? 'Give the server a name.'
    : store.servers.some((item) => item.name.toLowerCase() === name.trim().toLowerCase()) ? 'A server with this name already exists.' : '';
  const portError = port < 1024 || port > 65535 ? 'Pick a port between 1024 and 65535.' : '';
  const canContinue = useMemo(() => {
    if (step === 0) return Boolean(selected && chosenVersion?.automatic);
    if (step === 1) return maxMemory >= 2048 && maxMemory <= 12288;
    if (step === 2) return !nameError && !portError && Boolean(parentFolder.trim());
    return eula;
  }, [step, selected, chosenVersion, maxMemory, nameError, portError, parentFolder, eula]);

  const finish = async () => {
    if (!selected || !chosenVersion) return;
    setPhase('working');
    setProgress(0);
    setOperationPhase('resolve');
    setOperationId(null);
    setCancelling(false);
    setWasCancelled(false);
    setMessage('Preparing modpack setup');
    try {
      const created = await store.createModpackServer({
        provider: selected.provider,
        projectId: selected.projectId,
        versionId: chosenVersion.id,
        name: name.trim(),
        minMemory: 1024,
        maxMemory,
        port,
        parentFolder,
        eula,
        iconUrl: selected.iconUrl,
      }, (event: OperationEvent) => {
        setOperationId(event.data.operationId);
        if (event.event !== 'progress') return;
        setProgress(event.data.progress ?? 0);
        setOperationPhase(event.data.phase ?? 'resolve');
        setMessage(event.data.message);
      });
      setServer(created);
      setPhase('done');
      setProgress(100);
      store.pushToast({ tone: 'success', title: `${created.name} is ready`, detail: `${selected.name} was installed and configured.` });
    } catch (error) {
      const cancelled = typeof error === 'object' && error !== null && 'code' in error && (error as { code?: string }).code === 'cancelled';
      setWasCancelled(cancelled);
      setPhase('failed');
      setMessage(cancelled ? 'The partial download and temporary server files were discarded.' : errorMessage(error));
    }
  };

  const cancelInstall = async () => {
    if (!operationId || cancelling) return;
    setCancelling(true);
    setMessage('Cancelling and cleaning up temporary files…');
    try { await store.cancelOperation(operationId); } catch { setCancelling(false); }
  };

  if (phase !== 'form') {
    return (
      <Modal open onClose={phase === 'working' ? () => {} : onClose} dismissable={phase !== 'working'}
        title={phase === 'working' ? 'Installing your modpack' : phase === 'done' ? 'Modpack server ready' : wasCancelled ? 'Installation cancelled' : 'Setup did not finish'}
        description={phase === 'done' ? `${name} is now in your server list.` : undefined} width={480}
        footer={phase === 'working' ? <><span className="text-muted text-sm">Large packs can take several minutes</span><button className="btn btn-secondary" disabled={!operationId || cancelling} onClick={() => void cancelInstall()}>{cancelling ? 'Cancelling…' : 'Cancel install'}</button></> : phase === 'done' ? <>
          <button className="btn btn-secondary" onClick={onClose}>Close</button>
          <button className="btn btn-primary" onClick={() => { onClose(); if (server) store.openServer(server.id); }}>Open server</button>
        </> : <>
          <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary" onClick={() => setPhase('form')}>Back</button>
        </>}
      >
        {phase === 'working' && <div className="work modpack-install-progress">
          <div className="modpack-progress-heading"><span>Overall progress</span><strong>{Math.round(progress)}%</strong></div>
          <ProgressBar value={progress} />
          <div className="modpack-install-stages" aria-label="Installation stages">
            {installStages.map((label, index) => {
              const activeStage = installStageForPhase(operationPhase);
              const state = index < activeStage ? 'complete' : index === activeStage ? 'active' : 'pending';
              return <div className={`modpack-install-stage ${state}`} key={label}>
                <span>{state === 'complete' ? <IconCheck size={11} /> : index + 1}</span>
                <small>{label}</small>
              </div>;
            })}
          </div>
          <p className="work-msg">{message}</p>
        </div>}
        {phase === 'done' && <Callout tone="success" title="Everything is configured">Nooki installed {selected?.name}, {chosenVersion?.loader} and Minecraft {chosenVersion?.minecraftVersion}. Start it when you are ready.</Callout>}
        {phase === 'failed' && <Callout tone={wasCancelled ? 'info' : 'error'} title={wasCancelled ? 'Installation cancelled' : 'Modpack server was not created'}>{message}</Callout>}
      </Modal>
    );
  }

  return (
    <Modal open onClose={onClose} className={`modpack-modal ${step === 0 ? 'is-browser-step' : ''}`} title="Create from a modpack" description="Pick a server pack and Nooki configures the loader, Java, and files." width={760}
      footer={<>
        <button className="btn btn-ghost" onClick={() => { if (step === 0) onBack(); else setStep(step - 1); }}>Back</button>
        <div className="foot-spacer" />
        <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
        <button className="btn btn-primary" disabled={!canContinue} onClick={() => { if (step === steps.length - 1) void finish(); else setStep(step + 1); }}>
          {step === steps.length - 1 ? 'Install modpack' : 'Continue'}
        </button>
      </>}
    >
      <div className="wizard modpack-wizard">
        <Stepper steps={steps} current={step} />
        {step === 0 && <div className="modpack-browser">
          <aside className="modpack-provider-rail" aria-label="Modpack provider">
            <button className={provider === 'modrinth' ? 'active' : ''} onClick={() => { setProvider('modrinth'); setSelected(null); setQuery(''); }}>
              <span className="provider-symbol"><img src={modrinthLogo} alt="" /></span><span>Modrinth</span>
            </button>
            <button className={provider === 'curseforge' ? 'active' : ''} onClick={() => { setProvider('curseforge'); setSelected(null); setQuery(''); }}>
              <span className="provider-symbol"><img src={curseforgeLogo} alt="" /></span><span>CurseForge</span>
            </button>
          </aside>
          <section className="modpack-results">
            {selected ? <>
              <button className="modpack-back" onClick={() => { setSelected(null); setMessage(''); }}><IconArrowLeft size={14} /> All packs</button>
              <div className="modpack-selected-head"><PackLogo project={selected} /><div><strong>{selected.name}</strong><span>by {selected.author}</span></div></div>
              <div className="modpack-version-list">
                {versionsLoading ? Array.from({ length: 5 }, (_, index) => <div className="modpack-version-skeleton" key={index}><i /><i /></div>)
                  : message ? <Callout tone="error" title="Releases could not be loaded">{message}</Callout>
                  : versions.length === 0 ? <Callout tone="warning" title="No supported server releases">Nooki currently supports Forge, NeoForge, and Fabric server packs.</Callout>
                  : versions.map((version) => <button key={version.id} className={`modpack-version ${version.id === versionId ? 'active' : ''}`} onClick={() => setVersionId(version.id)}>
                    <span><strong>{version.name}</strong><small>Minecraft {version.minecraftVersion} · {version.loader} · {formatBytes(version.size)}</small></span>
                    <span className="modpack-version-tags">{version.releaseType !== 'release' && <em>{version.releaseType}</em>}{version.automatic ? <em className="auto">Auto setup</em> : <em className="manual">No server pack</em>}{version.id === versionId && <IconCheck size={14} />}</span>
                  </button>)}
              </div>
              {chosenVersion && !chosenVersion.automatic && <Callout tone="warning" title="Automatic setup unavailable">Choose a release marked Auto setup. This release has no downloadable CurseForge server pack.</Callout>}
            </> : <>
              <div className="modpack-search"><IconSearch size={14} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Search ${provider === 'modrinth' ? 'Modrinth' : 'CurseForge'} modpacks…`} /></div>
              <div className="modpack-project-list">
                {loading ? Array.from({ length: 6 }, (_, index) => <div className="modpack-project-skeleton" key={index}><i /><span><i /><i /></span></div>)
                  : !catalog ? <Callout tone="error" title="Catalog unavailable">{message || 'Try again in a moment.'}</Callout>
                  : catalog.projects.map((project) => <button className="modpack-project" key={`${project.provider}:${project.projectId}`} onClick={() => chooseProject(project)}>
                    <PackLogo project={project} /><span className="modpack-project-copy"><strong>{project.name}</strong><small>{project.description}</small><em>{project.author} · {compactNumber(project.downloads)} downloads</em></span>
                  </button>)}
              </div>
            </>}
          </section>
        </div>}

        {step === 1 && <div className="wizard-panel">
          <Field label="Memory for this modpack" hint="Modpacks commonly need more memory than a vanilla server.">
            <div className="mem-row"><input type="range" min={2048} max={12288} step={512} value={maxMemory} onChange={(event) => setMaxMemory(Number(event.target.value))} className="range" /><span className="mem-value">{formatMegabytes(maxMemory)}</span></div>
          </Field>
          <div className="mem-presets">{[4096, 6144, 8192, 10240].map((memory) => <button key={memory} className={`preset ${maxMemory === memory ? 'active' : ''}`} onClick={() => setMaxMemory(memory)}>{formatMegabytes(memory)}<span>{memory <= 4096 ? 'small packs' : memory <= 8192 ? 'most packs' : 'large packs'}</span></button>)}</div>
        </div>}

        {step === 2 && <div className="wizard-panel">
          <Field label="Server name" error={nameError || undefined}><input className="input" value={name} onChange={(event) => setName(event.target.value)} /></Field>
          <Field label="Server folder" hint="Nooki creates a separate folder for this pack."><FolderPicker value={parentFolder} onChange={setParentFolder} /></Field>
          <Field label="Port" error={portError || undefined} hint="Stopped servers can share a port."><input className="input mono" type="number" value={port} onChange={(event) => setPort(Number(event.target.value))} /></Field>
        </div>}

        {step === 3 && <div className="wizard-panel">
          <div className="review"><div className="review-icon">{selected ? <PackLogo project={selected} /> : <IconBox size={34} />}</div><div className="review-rows">
            <div className="summary-row"><span>Pack</span><strong>{selected?.name}</strong></div>
            <div className="summary-row"><span>Release</span><strong>{chosenVersion?.name}</strong></div>
            <div className="summary-row"><span>Software</span><strong>{chosenVersion?.loader} · Minecraft {chosenVersion?.minecraftVersion}</strong></div>
            <div className="summary-row"><span>Memory</span><strong>{formatMegabytes(maxMemory)}</strong></div>
            <div className="summary-row"><span>Port</span><strong className="mono">{port}</strong></div>
          </div></div>
          <Callout tone="info" title="Nooki handles the full setup">The pack files, exact loader, compatible Java runtime, EULA, port, and launch target are configured before the server is added.</Callout>
          <div className="eula"><Toggle checked={eula} onChange={setEula} label="I accept the Minecraft End User Licence Agreement" error={!eula ? 'Required before installation' : undefined} /></div>
        </div>}
      </div>
    </Modal>
  );
}
