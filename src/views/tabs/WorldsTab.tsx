import { useEffect, useMemo, useState } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useStore } from '../../state/store';
import type { Server, WorldEntry, WorldSettingsInput } from '../../types';
import { Callout, EmptyState, Field, Modal, Select, Spinner } from '../../components/ui';
import { IconCopy, IconFolder, IconGlobe, IconRefresh, IconTrash, IconWarning } from '../../components/Icons';
import { formatBytes, formatRelative } from '../../format';
import './WorldsTab.css';

const kindLabels = { overworld: 'Overworld', nether: 'Nether', end: 'The End', custom: 'Custom dimension' } as const;

function messageFrom(error: unknown) {
  if (typeof error === 'object' && error && 'message' in error) return String((error as { message: unknown }).message);
  return String(error);
}

function draftFor(world: WorldEntry): WorldSettingsInput {
  return {
    seed: world.seed ?? '0',
    spawnX: world.spawnX ?? 0,
    spawnY: world.spawnY ?? 64,
    spawnZ: world.spawnZ ?? 0,
    borderSize: world.borderSize ?? 59999968,
    dayTime: Math.max(0, Math.min(23999, world.dayTime == null ? 0 : ((world.dayTime % 24000) + 24000) % 24000)),
    weather: world.weather === 'rain' || world.weather === 'thunder' ? world.weather : 'clear',
  };
}

function timeLabel(ticks: number) {
  const hour = Math.floor(((ticks / 1000) + 6) % 24);
  const minutes = Math.floor(((ticks % 1000) / 1000) * 60);
  return `${String(hour).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`;
}

export default function WorldsTab({ server }: { server: Server }) {
  const store = useStore();
  const [worlds, setWorlds] = useState<WorldEntry[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [draft, setDraft] = useState<WorldSettingsInput | null>(null);
  const [copied, setCopied] = useState(false);
  const [resetTarget, setResetTarget] = useState<WorldEntry | null>(null);
  const [resetPlayers, setResetPlayers] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<WorldEntry | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState('');
  const [destructiveBusy, setDestructiveBusy] = useState(false);
  const editable = server.status === 'stopped' || server.status === 'crashed';

  const selected = useMemo(
    () => worlds.find((world) => world.id === selectedId) ?? worlds[0] ?? null,
    [selectedId, worlds],
  );

  const refresh = async (quiet = false) => {
    if (quiet) setRefreshing(true); else setLoading(true);
    try {
      const next = await store.listWorlds(server.id);
      setWorlds(next);
      setSelectedId((current) => next.some((world) => world.id === current) ? current : next[0]?.id ?? null);
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'Could not scan worlds', detail: messageFrom(error) });
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => { void refresh(); }, [server.id]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { if (selected) setDraft(draftFor(selected)); }, [selected]);

  const copySeed = () => {
    if (!selected?.seed) return;
    void writeText(selected.seed);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };

  const save = async () => {
    if (!selected || !draft) return;
    setSaving(true);
    try {
      const next = await store.saveWorldSettings(server.id, selected.id, draft);
      setWorlds(next);
      store.pushToast({ tone: 'success', title: `${selected.name} updated`, detail: 'The new metadata will apply the next time the server starts.' });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'World settings were not saved', detail: messageFrom(error) });
    } finally {
      setSaving(false);
    }
  };

  const regenerate = async () => {
    if (!resetTarget) return;
    const target = resetTarget;
    setDestructiveBusy(true);
    try {
      const next = await store.regenerateWorld(server.id, target.id, resetPlayers);
      setWorlds(next);
      setResetTarget(null);
      store.pushToast({ tone: 'success', title: `${target.name} queued for regeneration`, detail: 'Existing terrain was moved to the Recycle Bin. New terrain generates on the next start.' });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'World was not regenerated', detail: messageFrom(error) });
    } finally {
      setDestructiveBusy(false);
    }
  };

  const remove = async () => {
    if (!deleteTarget) return;
    const target = deleteTarget;
    setDestructiveBusy(true);
    try {
      const next = await store.deleteWorld(server.id, target.id, deleteConfirmation);
      setWorlds(next);
      setDeleteTarget(null);
      setDeleteConfirmation('');
      store.pushToast({ tone: 'success', title: `${target.name} moved to the Recycle Bin` });
    } catch (error) {
      store.pushToast({ tone: 'error', title: 'World was not deleted', detail: messageFrom(error) });
    } finally {
      setDestructiveBusy(false);
    }
  };

  const totalSize = worlds.reduce((total, world) => total + world.size, 0);
  const totalRegions = worlds.reduce((total, world) => total + world.regionFiles, 0);

  return (
    <div className="tab worlds-tab">
      <div className="worlds-toolbar">
        <div>
          <h2>Worlds</h2>
          <p>{worlds.length} dimension{worlds.length === 1 ? '' : 's'} · {formatBytes(totalSize)} terrain · {totalRegions} region files</p>
        </div>
        <button className="btn btn-secondary btn-sm" disabled={refreshing} onClick={() => void refresh(true)}>{refreshing ? <Spinner size={12} /> : <IconRefresh size={13} />} Rescan</button>
      </div>

      {!editable && <Callout tone="warning" title="World files are locked while the server is active">You can inspect metadata now, but stop the server before editing, regenerating, or deleting a world.</Callout>}

      {loading ? (
        <div className="world-layout"><div className="world-list">{[0, 1, 2].map((item) => <div className="world-card world-skeleton" key={item}><i /><div><i /><i /></div></div>)}</div><div className="world-detail world-detail-skeleton"><i /><i /><i /></div></div>
      ) : worlds.length === 0 ? (
        <div className="worlds-empty"><EmptyState icon={<IconGlobe size={38} />} title="No worlds found" description="Start the server once to generate its first world, then rescan." /></div>
      ) : (
        <div className="world-layout">
          <div className="world-list" role="list" aria-label="Server worlds">
            {worlds.map((world) => <button type="button" role="listitem" key={world.id} className={`world-card ${selected?.id === world.id ? 'is-selected' : ''}`} onClick={() => setSelectedId(world.id)}><WorldMark kind={world.kind} generated={world.generated} /><div className="world-card-copy"><span className="world-card-name">{world.name}</span><span>{kindLabels[world.kind]} · {world.generated ? formatBytes(world.size) : 'Not generated'}</span><small>{world.folderName}{world.version ? ` · ${world.version}` : ''}</small></div>{world.custom && <span className="world-custom-tag">Extra</span>}</button>)}
          </div>

          {selected && draft && (
            <section className="world-detail">
              <header className="world-detail-head">
                <WorldMark kind={selected.kind} generated={selected.generated} large />
                <div><span>{kindLabels[selected.kind]}</span><h3>{selected.name}</h3><p className="mono">{selected.folderName}</p></div>
                <div className="world-detail-actions"><button className="btn btn-secondary btn-sm" onClick={() => store.revealPath(selected.path)}><IconFolder size={13} /> Open folder</button></div>
              </header>

              {selected.metadataError && <Callout tone="error" title="Some metadata could not be read">{selected.metadataError}</Callout>}
              {!selected.generated && <Callout tone="info" title="This dimension has not generated yet">Its folder and terrain will be created automatically when Minecraft first loads it.</Callout>}

              <div className="world-facts">
                <WorldFact label="Terrain" value={formatBytes(selected.size)} />
                <WorldFact label="Regions" value={String(selected.regionFiles)} />
                <WorldFact label="Player files" value={String(selected.playerFiles)} />
                <WorldFact label="Last played" value={selected.lastPlayed ? formatRelative(selected.lastPlayed) : 'Unknown'} />
                <WorldFact label="Game mode" value={selected.gameMode ?? 'Inherited'} />
                <WorldFact label="Difficulty" value={selected.difficulty ?? 'Inherited'} />
                <WorldFact label="Time" value={`${timeLabel(draft.dayTime)} · ${draft.dayTime} ticks`} />
                <WorldFact label="Weather" value={selected.weather === 'thunder' ? 'Thunderstorm' : selected.weather === 'rain' ? 'Raining' : 'Clear'} />
              </div>

              <div className="world-settings-head"><div><h4>World metadata</h4><p>Seed, spawn, border, time, and weather are stored directly in level.dat.</p></div>{selected.hardcore && <span className="world-flag danger">Hardcore</span>}{selected.allowCommands && <span className="world-flag">Commands</span>}</div>
              <div className="world-settings-grid">
                <Field label="Seed" hint="Shared by dimensions that use the same world save."><div className="world-seed-input"><input className="input mono" value={draft.seed} disabled={!editable || !selected.seed} onChange={(event) => setDraft({ ...draft, seed: event.target.value })} /><button className="btn btn-secondary btn-sm" disabled={!selected.seed} onClick={copySeed}><IconCopy size={12} /> {copied ? 'Copied' : 'Copy'}</button></div></Field>
                <Field label="World border diameter"><input className="input" type="number" min={1} max={59999968} value={draft.borderSize} disabled={!editable || !selected.seed} onChange={(event) => setDraft({ ...draft, borderSize: Number(event.target.value) })} /></Field>
                <div className="world-coordinate-field"><span>Spawn position</span><div><input className="input" type="number" aria-label="Spawn X" value={draft.spawnX} disabled={!editable || !selected.seed} onChange={(event) => setDraft({ ...draft, spawnX: Number(event.target.value) })} /><input className="input" type="number" aria-label="Spawn Y" value={draft.spawnY} disabled={!editable || !selected.seed} onChange={(event) => setDraft({ ...draft, spawnY: Number(event.target.value) })} /><input className="input" type="number" aria-label="Spawn Z" value={draft.spawnZ} disabled={!editable || !selected.seed} onChange={(event) => setDraft({ ...draft, spawnZ: Number(event.target.value) })} /></div><small>X / Y / Z</small></div>
                <Field label="Time of day"><Select value={String(draft.dayTime)} disabled={!editable || !selected.seed} onChange={(value) => setDraft({ ...draft, dayTime: Number(value) })} options={[{ value: '0', label: 'Sunrise · 06:00' }, { value: '6000', label: 'Noon · 12:00' }, { value: '12000', label: 'Sunset · 18:00' }, { value: '18000', label: 'Midnight · 00:00' }]} /></Field>
                <Field label="Weather"><Select value={draft.weather} disabled={!editable || !selected.seed} onChange={(value) => setDraft({ ...draft, weather: value as WorldSettingsInput['weather'] })} options={[{ value: 'clear', label: 'Clear' }, { value: 'rain', label: 'Rain' }, { value: 'thunder', label: 'Thunderstorm' }]} /></Field>
              </div>
              <div className="world-save-row"><span>{selected.seed ? `Seed ${selected.seed}` : 'Metadata becomes editable after the world is generated.'}</span><button className="btn btn-primary" disabled={!editable || !selected.seed || saving} onClick={() => void save()}>{saving ? <><Spinner size={12} /> Saving</> : 'Save metadata'}</button></div>

              <div className="world-danger-zone">
                <div><IconWarning size={16} /><div><strong>Terrain operations</strong><span>Terrain goes to the Windows Recycle Bin before Minecraft generates a replacement.</span></div></div>
                <div><button className="btn btn-secondary" disabled={!editable || !selected.generated} onClick={() => { setResetPlayers(false); setResetTarget(selected); }}>Regenerate terrain</button>{selected.custom && !selected.primary && <button className="btn btn-danger" disabled={!editable} onClick={() => { setDeleteConfirmation(''); setDeleteTarget(selected); }}><IconTrash size={13} /> Delete world</button>}</div>
              </div>
            </section>
          )}
        </div>
      )}

      <Modal open={resetTarget !== null} onClose={destructiveBusy ? () => {} : () => setResetTarget(null)} dismissable={!destructiveBusy} title={`Regenerate ${resetTarget?.name ?? 'world'}?`} description="Existing region, entity, and point-of-interest files will be moved to the Recycle Bin. Minecraft creates fresh terrain on the next start." width={470} tone="danger" footer={<><button className="btn btn-secondary" disabled={destructiveBusy} onClick={() => setResetTarget(null)}>Cancel</button><button className="btn btn-danger" disabled={destructiveBusy} onClick={() => void regenerate()}>{destructiveBusy ? <><Spinner size={12} /> Moving terrain</> : 'Regenerate terrain'}</button></>}><label className="world-reset-players"><input type="checkbox" checked={resetPlayers} onChange={(event) => setResetPlayers(event.target.checked)} /><span><strong>Also reset player progress</strong><small>Moves playerdata, advancements, and statistics to the Recycle Bin for this world save.</small></span></label></Modal>

      <Modal open={deleteTarget !== null} onClose={destructiveBusy ? () => {} : () => setDeleteTarget(null)} dismissable={!destructiveBusy} title={`Delete ${deleteTarget?.name ?? 'world'}?`} description="The complete custom world folder will be moved to the Windows Recycle Bin." width={450} tone="danger" footer={<><button className="btn btn-secondary" disabled={destructiveBusy} onClick={() => setDeleteTarget(null)}>Cancel</button><button className="btn btn-danger" disabled={destructiveBusy || deleteConfirmation !== deleteTarget?.name} onClick={() => void remove()}>{destructiveBusy ? <><Spinner size={12} /> Deleting</> : 'Delete world'}</button></>}><Field label={`Type “${deleteTarget?.name ?? ''}” to confirm`}><input className="input" value={deleteConfirmation} onChange={(event) => setDeleteConfirmation(event.target.value)} /></Field></Modal>
    </div>
  );
}

function WorldMark({ kind, generated, large = false }: { kind: WorldEntry['kind']; generated: boolean; large?: boolean }) {
  return <span className={`world-mark is-${kind} ${large ? 'is-large' : ''} ${generated ? '' : 'is-empty'}`}><span /><i /></span>;
}

function WorldFact({ label, value }: { label: string; value: string }) {
  return <div><span>{label}</span><strong>{value}</strong></div>;
}
