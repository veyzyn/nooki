import { useMemo, useState } from 'react';
import { useStore } from '../../state/store';
import type { Server } from '../../types';
import { Callout, Field, Select, Toggle } from '../../components/ui';
import { formatMegabytes } from '../../format';
import './ServerSettingsTab.css';

interface Draft {
  name: string;
  motd: string;
  gameMode: Server['gameMode'];
  difficulty: Server['difficulty'];
  maxPlayers: number;
  pvp: boolean;
  whitelistEnabled: boolean;
  onlineMode: boolean;
  port: number;
  minMemory: number;
  maxMemory: number;
  javaRuntimeId: string;
  jvmArgs: string;
  vanity: string;
}

function draftFrom(server: Server): Draft {
  return {
    name: server.name,
    motd: server.motd,
    gameMode: server.gameMode,
    difficulty: server.difficulty,
    maxPlayers: server.maxPlayers,
    pvp: server.pvp,
    whitelistEnabled: server.whitelistEnabled,
    onlineMode: server.onlineMode,
    port: server.port,
    minMemory: server.minMemory,
    maxMemory: server.maxMemory,
    javaRuntimeId: server.javaRuntimeId,
    jvmArgs: server.jvmArgs,
    vanity: server.sharing.vanity ?? '',
  };
}

/** Fields that only take effect after a restart. */
const restartFields: (keyof Draft)[] = [
  'motd',
  'gameMode',
  'difficulty',
  'maxPlayers',
  'pvp',
  'whitelistEnabled',
  'onlineMode',
  'port',
  'minMemory',
  'maxMemory',
  'javaRuntimeId',
  'jvmArgs',
];

export default function ServerSettingsTab({ server }: { server: Server }) {
  const store = useStore();
  const [draft, setDraft] = useState<Draft>(() => draftFrom(server));
  const [saved, setSaved] = useState(false);

  const patch = (p: Partial<Draft>) => {
    setDraft((prev) => ({ ...prev, ...p }));
    setSaved(false);
  };

  const errors = useMemo(() => {
    const e: Partial<Record<keyof Draft, string>> = {};
    if (!draft.name.trim()) e.name = 'A server needs a name.';
    else if (
      store.servers.some((s) => s.id !== server.id && s.name.toLowerCase() === draft.name.trim().toLowerCase())
    )
      e.name = 'Another server already uses this name.';

    if (!Number.isFinite(draft.port)) e.port = 'Enter a port number.';
    else if (draft.port < 1024 || draft.port > 65535) e.port = 'Pick a port between 1024 and 65535.';

    if (draft.maxPlayers < 1 || draft.maxPlayers > 200) e.maxPlayers = 'Choose between 1 and 200 players.';

    if (draft.minMemory >= draft.maxMemory) e.maxMemory = 'Maximum memory must be higher than the minimum.';
    else if (draft.maxMemory > 12288) e.maxMemory = 'That is more than this computer can spare.';

    if (!draft.motd.trim()) e.motd = 'The message shown in the server list cannot be empty.';
    if (draft.vanity && !/^[a-z0-9][a-z0-9-]{1,30}[a-z0-9]$/.test(draft.vanity)) {
      e.vanity = 'Use 3–32 lowercase letters, numbers, or hyphens.';
    }
    return e;
  }, [draft, server.id, store.servers]);

  const changedKeys = useMemo(() => {
    const base = draftFrom(server);
    return (Object.keys(base) as (keyof Draft)[]).filter((k) => base[k] !== draft[k]);
  }, [draft, server]);

  const dirty = changedKeys.length > 0;
  const valid = Object.keys(errors).length === 0;
  const needsRestart = changedKeys.some((k) => restartFields.includes(k)) && server.status === 'running';

  const save = () => {
    if (!valid) return;
    store.patchServer(server.id, {
      name: draft.name.trim(),
      motd: draft.motd,
      gameMode: draft.gameMode,
      difficulty: draft.difficulty,
      maxPlayers: draft.maxPlayers,
      pvp: draft.pvp,
      whitelistEnabled: draft.whitelistEnabled,
      onlineMode: draft.onlineMode,
      port: draft.port,
      minMemory: draft.minMemory,
      maxMemory: draft.maxMemory,
      javaRuntimeId: draft.javaRuntimeId,
      jvmArgs: draft.jvmArgs,
      sharing: { ...server.sharing, vanity: draft.vanity || null },
    });
    store.logActivity({
      kind: 'settings',
      serverId: server.id,
      serverName: draft.name.trim(),
      message: 'Settings updated',
    });
    store.pushToast({
      tone: 'success',
      title: 'Settings saved',
      detail: needsRestart ? 'Restart the server to apply everything.' : undefined,
    });
    if (needsRestart) store.setRestartRequired(true);
    setSaved(true);
  };

  const discard = () => {
    setDraft(draftFrom(server));
    setSaved(false);
  };

  return (
    <div className="tab settings-tab">
      {dirty && (
        <div className="save-bar">
          <div className="save-bar-text">
            <span className="save-bar-title">You have unsaved changes</span>
            <span className="save-bar-detail">
              {changedKeys.length} setting{changedKeys.length !== 1 ? 's' : ''} changed
              {needsRestart ? ' · a restart is needed to apply some of them' : ''}
            </span>
          </div>
          <button className="btn btn-sm btn-ghost" onClick={discard}>
            Discard
          </button>
          <button className="btn btn-sm btn-primary" disabled={!valid} onClick={save}>
            Save changes
          </button>
        </div>
      )}

      {saved && !dirty && (
        <Callout
          tone="success"
          title="Settings saved"
          action={
            store.restartRequired && server.status === 'running' ? (
              <button className="btn btn-sm btn-secondary" onClick={() => store.restartServer(server.id)}>
                Restart now
              </button>
            ) : undefined
          }
        >
          {store.restartRequired && server.status === 'running'
            ? 'Some changes only take effect after the server restarts.'
            : 'Everything is up to date.'}
        </Callout>
      )}

      <Group title="General">
        <Field label="Server name" error={errors.name} hint="Only used inside Nooki.">
          <input className="input" value={draft.name} onChange={(e) => patch({ name: e.target.value })} />
        </Field>
        <Field label="Server folder" hint="Remove and re-import the server if you move this folder.">
          <div className="picker">
            <input className="input mono" value={server.folder} readOnly />
            <button className="btn btn-secondary" onClick={() => store.revealPath(server.folder)}>Open folder</button>
          </div>
        </Field>
      </Group>

      <Group title="Gameplay">
        <Field
          label="Message in the server list"
          error={errors.motd}
          restartHint
          hint="Players see this under the server name in their Minecraft client."
        >
          <input className="input" value={draft.motd} onChange={(e) => patch({ motd: e.target.value })} />
        </Field>

        <div className="two-col">
          <Field label="Game mode" restartHint>
            <Select
              value={draft.gameMode}
              options={[
                { value: 'survival', label: 'Survival' },
                { value: 'creative', label: 'Creative' },
                { value: 'adventure', label: 'Adventure' },
                { value: 'spectator', label: 'Spectator' },
              ]}
              onChange={(value) => patch({ gameMode: value as Draft['gameMode'] })}
            />
          </Field>
          <Field label="Difficulty" restartHint>
            <Select
              value={draft.difficulty}
              options={[
                { value: 'peaceful', label: 'Peaceful' },
                { value: 'easy', label: 'Easy' },
                { value: 'normal', label: 'Normal' },
                { value: 'hard', label: 'Hard' },
              ]}
              onChange={(value) => patch({ difficulty: value as Draft['difficulty'] })}
            />
          </Field>
        </div>

        <Field
          label="Maximum players"
          error={errors.maxPlayers}
          restartHint
          hint="More players need more memory. Around 100 MB each is a safe estimate."
        >
          <input
            className="input"
            type="number"
            min={1}
            max={200}
            value={draft.maxPlayers}
            onChange={(e) => patch({ maxPlayers: Number(e.target.value) })}
          />
        </Field>

        <div className="toggle-stack">
          <Toggle
            checked={draft.pvp}
            onChange={(pvp) => patch({ pvp })}
            label="Allow players to fight each other"
            hint="Turn this off for a friendlier, build-focused server."
            restartHint
          />
          <Toggle
            checked={draft.whitelistEnabled}
            onChange={(whitelistEnabled) => patch({ whitelistEnabled })}
            label="Only let whitelisted players join"
            hint="Recommended if your server is reachable from the internet."
            restartHint
          />
          <Toggle
            checked={draft.onlineMode}
            onChange={(onlineMode) => patch({ onlineMode })}
            label="Verify players with Mojang"
            hint="Keep this on unless you know exactly why you need it off."
            restartHint
          />
        </div>
      </Group>

      <Group title="Network">
        <Field
          label="Port"
          error={errors.port}
          restartHint
          hint={`Players connect to localhost:${draft.port}. Stopped servers may share this port.`}
        >
          <input
            className="input mono"
            type="number"
            value={draft.port}
            onChange={(e) => patch({ port: Number(e.target.value) })}
          />
        </Field>
        <Field
          label="Vanity address"
          error={errors.vanity}
          hint={store.relayAccess.activated
            ? 'Optional. Without one, the public address changes each time this server starts.'
            : 'Activate relay access in App Settings before reserving a public address.'}
        >
          <div className="vanity-input">
            <input
              className="input mono"
              value={draft.vanity}
              disabled={!store.relayAccess.activated}
              maxLength={32}
              placeholder="mycoolserver"
              onChange={(event) => patch({ vanity: event.target.value.toLowerCase().replace(/[^a-z0-9-]/g, '') })}
            />
            <span>.nooki-64f85d08d9.mints.wtf</span>
          </div>
        </Field>
        {server.sharing.lastError && (
          <Callout tone="warning" title="Public address unavailable">{server.sharing.lastError}</Callout>
        )}
        {server.alerts.some((a) => a.kind === 'port-conflict') && (
          <Callout tone="warning" title="This port is in use by another program">
            Pick a different port, or close whatever else is listening on {server.port}.
          </Callout>
        )}
      </Group>

      <Group title="Performance">
        <div className="two-col">
          <Field label="Minimum memory" restartHint hint="Reserved as soon as the server starts.">
            <Select
              value={String(draft.minMemory)}
              options={[512, 1024, 2048, 3072, 4096].map((mb) => ({ value: String(mb), label: formatMegabytes(mb) }))}
              onChange={(value) => patch({ minMemory: Number(value) })}
            />
          </Field>
          <Field label="Maximum memory" error={errors.maxMemory} restartHint hint="The most this server may use.">
            <Select
              value={String(draft.maxMemory)}
              options={[1024, 2048, 3072, 4096, 6144, 8192, 10240, 12288].map((mb) => ({ value: String(mb), label: formatMegabytes(mb) }))}
              onChange={(value) => patch({ maxMemory: Number(value) })}
            />
          </Field>
        </div>
        <Callout tone="info" title="Leave room for Windows">
          This computer has {formatMegabytes(store.host.totalMemory)}. Keeping at least 4 GB free for the rest of your
          system keeps everything responsive.
        </Callout>
      </Group>

      <Group title="Advanced" subtitle="Most people never need to change these.">
        <Field label="Java runtime" restartHint hint="Newer Minecraft versions need Java 17 or later.">
          <Select
            value={draft.javaRuntimeId}
            options={store.javaRuntimes.map((jr) => ({ value: jr.id, label: `${jr.label} · ${jr.version} ${jr.bundled ? '(bundled)' : '(system)'}` }))}
            onChange={(javaRuntimeId) => patch({ javaRuntimeId })}
          />
        </Field>
        <Field
          label="Java startup options"
          restartHint
          hint="Extra flags passed to Java. Leave as they are unless you are following a specific guide."
        >
          <textarea
            className="textarea"
            rows={3}
            value={draft.jvmArgs}
            onChange={(e) => patch({ jvmArgs: e.target.value })}
            spellCheck={false}
          />
        </Field>
      </Group>
    </div>
  );
}

function Group({ title, subtitle, children }: { title: string; subtitle?: string; children: React.ReactNode }) {
  return (
    <section className="settings-group">
      <header className="settings-group-head">
        <h3 className="settings-group-title">{title}</h3>
        {subtitle && <p className="settings-group-sub">{subtitle}</p>}
      </header>
      <div className="settings-group-body">{children}</div>
    </section>
  );
}
