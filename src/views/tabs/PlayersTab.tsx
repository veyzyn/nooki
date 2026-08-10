import { useState } from 'react';
import { useStore } from '../../state/store';
import type { Server } from '../../types';
import { Avatar, ConfirmDialog, EmptyState, Field, Modal, Segmented } from '../../components/ui';
import { IconUsers, IconShield, IconPlus } from '../../components/Icons';
import { formatRelative } from '../../format';
import './PlayersTab.css';

type PlayerTab = 'online' | 'whitelist' | 'operators' | 'banned';

export default function PlayersTab({ server }: { server: Server }) {
  const store = useStore();
  const [tab, setTab] = useState<PlayerTab>('online');
  const roster = store.rosters[server.id] ?? { whitelist: [], operators: [], banned: [] };
  const online = store.players.filter((p) => p.serverId === server.id);
  const running = server.status === 'running';

  const [confirmKick, setConfirmKick] = useState<string | null>(null);
  const [banTarget, setBanTarget] = useState<string | null>(null);
  const [banReason, setBanReason] = useState('');
  const [confirmRevokeOp, setConfirmRevokeOp] = useState<string | null>(null);
  const [confirmUnwhitelist, setConfirmUnwhitelist] = useState<string | null>(null);
  const [addOpen, setAddOpen] = useState<null | 'whitelist' | 'operators'>(null);
  const [addName, setAddName] = useState('');

  const kickTarget = online.find((p) => p.id === confirmKick);
  const banPlayer = online.find((p) => p.id === banTarget);
  const revokeEntry = roster.operators.find((o) => o.id === confirmRevokeOp);
  const unwhitelistEntry = roster.whitelist.find((w) => w.id === confirmUnwhitelist);

  const addNameError =
    addName.trim().length === 0
      ? undefined
      : addName.trim().length < 3
        ? 'Minecraft usernames are at least 3 characters.'
        : /[^A-Za-z0-9_]/.test(addName.trim())
          ? 'Usernames can only use letters, numbers, and underscores.'
          : undefined;

  return (
    <div className="tab players-tab">
      <div className="players-head">
        <Segmented
          value={tab}
          onChange={setTab}
          options={[
            { value: 'online', label: `Online${online.length ? ` (${online.length})` : ''}` },
            { value: 'whitelist', label: `Whitelist (${roster.whitelist.length})` },
            { value: 'operators', label: `Operators (${roster.operators.length})` },
            { value: 'banned', label: `Banned (${roster.banned.length})` },
          ]}
        />
        {(tab === 'whitelist' || tab === 'operators') && (
          <button
            className="btn btn-sm btn-secondary"
            disabled={!running}
            onClick={() => {
              setAddOpen(tab);
              setAddName('');
            }}
          >
            <IconPlus size={13} />
            Add player
          </button>
        )}
      </div>

      {!running && tab !== 'online' && <div className="panel-note">Start the server to change player access or permissions.</div>}

      {tab === 'online' && (
        <div className="players-panel">
          {online.length === 0 ? (
            <EmptyState
              icon={<IconUsers size={40} />}
              title={running ? 'Nobody is playing right now' : 'Server is not running'}
              description={
                running
                  ? `Players can join at ${server.sharing.address ?? `localhost:${server.port}`}. They will appear here as soon as they connect.`
                  : 'Start the server so players can connect.'
              }
            />
          ) : (
            <ul className="player-list">
              {online.map((p) => (
                <li key={p.id} className="player-row">
                  <Avatar name={p.username} color={p.avatar} size={34} />
                  <div className="player-main">
                    <span className="player-name">
                      {p.username}
                      {p.isOp && <span className="op-tag">operator</span>}
                    </span>
                    <span className="player-sub">
                      Playing for {formatRelative(p.connectedAt).replace(' ago', '')}
                    </span>
                  </div>
                  <div className="player-actions">
                    {!p.isOp ? (
                      <button className="btn btn-sm btn-ghost" onClick={() => store.grantOperator(server.id, p.username)}>
                        Make operator
                      </button>
                    ) : (
                      <button
                        className="btn btn-sm btn-ghost"
                        onClick={() => {
                          const entry = roster.operators.find((o) => o.username === p.username);
                          if (entry) setConfirmRevokeOp(entry.id);
                        }}
                      >
                        Remove operator
                      </button>
                    )}
                    <button className="btn btn-sm btn-ghost" onClick={() => setConfirmKick(p.id)}>
                      Kick
                    </button>
                    <button
                      className="btn btn-sm btn-ghost danger-text"
                      onClick={() => {
                        setBanTarget(p.id);
                        setBanReason('');
                      }}
                    >
                      Ban
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {tab === 'whitelist' && (
        <div className="players-panel">
          {!server.whitelistEnabled && (
            <div className="panel-note">
              The whitelist is currently turned off, so anyone can join. Turn it on in Settings to use this list.
            </div>
          )}
          {roster.whitelist.length === 0 ? (
            <EmptyState
              icon={<IconShield size={40} />}
              title="Nobody is whitelisted"
              description="Add the players you want to let in. With the whitelist on, only these players can join."
              action={
                <button className="btn btn-primary" disabled={!running} onClick={() => setAddOpen('whitelist')}>
                  Add player
                </button>
              }
            />
          ) : (
            <ul className="player-list">
              {roster.whitelist.map((entry) => (
                <li key={entry.id} className="player-row">
                  <Avatar name={entry.username} color={entry.avatar} size={30} />
                  <div className="player-main">
                    <span className="player-name">{entry.username}</span>
                    <span className="player-sub">Added {formatRelative(entry.addedAt)}</span>
                  </div>
                  <div className="player-actions">
                    <button className="btn btn-sm btn-ghost" disabled={!running} onClick={() => setConfirmUnwhitelist(entry.id)}>
                      Remove
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {tab === 'operators' && (
        <div className="players-panel">
          <div className="panel-note">
            Operators can use commands, change the weather and time, and manage other players. Only add people you trust.
          </div>
          {roster.operators.length === 0 ? (
            <EmptyState
              icon={<IconShield size={40} />}
              title="No operators"
              description="Operators have full control of the server from inside the game."
              action={
                <button className="btn btn-primary" disabled={!running} onClick={() => setAddOpen('operators')}>
                  Add operator
                </button>
              }
            />
          ) : (
            <ul className="player-list">
              {roster.operators.map((entry) => (
                <li key={entry.id} className="player-row">
                  <Avatar name={entry.username} color={entry.avatar} size={30} />
                  <div className="player-main">
                    <span className="player-name">{entry.username}</span>
                    <span className="player-sub">Operator since {formatRelative(entry.addedAt)}</span>
                  </div>
                  <div className="player-actions">
                    <button className="btn btn-sm btn-ghost" disabled={!running} onClick={() => setConfirmRevokeOp(entry.id)}>
                      Remove operator
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {tab === 'banned' && (
        <div className="players-panel">
          {roster.banned.length === 0 ? (
            <EmptyState
              icon={<IconShield size={40} />}
              title="Nobody is banned"
              description="Players you ban will be listed here, along with the reason you gave."
            />
          ) : (
            <ul className="player-list">
              {roster.banned.map((entry) => (
                <li key={entry.id} className="player-row">
                  <Avatar name={entry.username} color={entry.avatar} size={30} />
                  <div className="player-main">
                    <span className="player-name">{entry.username}</span>
                    <span className="player-sub">
                      Banned {formatRelative(entry.addedAt)}
                      {entry.reason ? ` · ${entry.reason}` : ''}
                    </span>
                  </div>
                  <div className="player-actions">
                    <button className="btn btn-sm btn-secondary" disabled={!running} onClick={() => store.unbanPlayer(server.id, entry.id)}>
                      Unban
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {/* ---------------------------- Dialogs ---------------------------- */}

      <ConfirmDialog
        open={Boolean(kickTarget)}
        title={`Kick ${kickTarget?.username}?`}
        description="They will be disconnected right away but can join again immediately."
        confirmLabel="Kick player"
        onCancel={() => setConfirmKick(null)}
        onConfirm={() => {
          if (kickTarget) store.kickPlayer(server.id, kickTarget.id);
          setConfirmKick(null);
        }}
      />

      <Modal
        open={Boolean(banPlayer)}
        onClose={() => setBanTarget(null)}
        title={`Ban ${banPlayer?.username}?`}
        description="They will be disconnected and blocked from joining again until you unban them."
        width={440}
        tone="danger"
        footer={
          <>
            <button className="btn btn-secondary" onClick={() => setBanTarget(null)}>
              Cancel
            </button>
            <button
              className="btn btn-danger"
              onClick={() => {
                if (banPlayer) store.banPlayer(server.id, banPlayer.id, banReason.trim() || 'No reason given');
                setBanTarget(null);
              }}
            >
              Ban player
            </button>
          </>
        }
      >
        <Field label="Reason" hint="Players see this message when they try to reconnect.">
          <input
            className="input"
            value={banReason}
            placeholder="Griefing the spawn area"
            onChange={(e) => setBanReason(e.target.value)}
          />
        </Field>
      </Modal>

      <ConfirmDialog
        open={Boolean(revokeEntry)}
        title={`Remove operator from ${revokeEntry?.username}?`}
        description="They keep their whitelist access but lose command permissions."
        confirmLabel="Remove operator"
        onCancel={() => setConfirmRevokeOp(null)}
        onConfirm={() => {
          if (revokeEntry) store.revokeOperator(server.id, revokeEntry.id);
          setConfirmRevokeOp(null);
        }}
      />

      <ConfirmDialog
        open={Boolean(unwhitelistEntry)}
        title={`Remove ${unwhitelistEntry?.username} from the whitelist?`}
        description={
          server.whitelistEnabled
            ? 'They will not be able to join while the whitelist is on.'
            : 'The whitelist is off right now, so this will not disconnect them.'
        }
        confirmLabel="Remove"
        tone="danger"
        onCancel={() => setConfirmUnwhitelist(null)}
        onConfirm={() => {
          if (unwhitelistEntry) store.removeFromWhitelist(server.id, unwhitelistEntry.id);
          setConfirmUnwhitelist(null);
        }}
      />

      <Modal
        open={addOpen !== null}
        onClose={() => setAddOpen(null)}
        title={addOpen === 'operators' ? 'Add an operator' : 'Add to the whitelist'}
        description={
          addOpen === 'operators'
            ? 'Operators can run commands and manage the server from in game.'
            : 'Only whitelisted players can join while the whitelist is on.'
        }
        width={420}
        footer={
          <>
            <button className="btn btn-secondary" onClick={() => setAddOpen(null)}>
              Cancel
            </button>
            <button
              className="btn btn-primary"
              disabled={!running || addName.trim().length < 3 || Boolean(addNameError)}
              onClick={() => {
                if (addOpen === 'operators') store.grantOperator(server.id, addName.trim());
                else store.addToWhitelist(server.id, addName.trim());
                setAddOpen(null);
              }}
            >
              Add
            </button>
          </>
        }
      >
        <Field label="Minecraft username" error={addNameError} hint="This must match their username exactly.">
          <input className="input" value={addName} placeholder="birchbark" onChange={(e) => setAddName(e.target.value)} />
        </Field>
      </Modal>
    </div>
  );
}
