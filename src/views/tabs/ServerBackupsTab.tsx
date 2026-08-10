import { useState } from 'react';
import { useStore } from '../../state/store';
import type { Backup, Server } from '../../types';
import { Callout, ConfirmDialog, EmptyState, Field, Modal, ProgressBar, Select, Toggle } from '../../components/ui';
import { IconBox, IconPlus, IconFolder } from '../../components/Icons';
import { formatBytes, formatDateTime, formatRelative } from '../../format';
import './ServerBackupsTab.css';

const typeLabels: Record<Backup['type'], string> = {
  manual: 'Manual',
  scheduled: 'Scheduled',
  safety: 'Safety',
  'pre-update': 'Pre-update',
};

export default function ServerBackupsTab({ server }: { server: Server }) {
  const store = useStore();
  const backups = store.backups.filter((b) => b.serverId === server.id).sort((a, b) => b.createdAt - a.createdAt);
  const schedule = store.schedules[server.id] ?? { enabled: false, frequency: 'daily', time: '04:00', keep: 5 };

  const [createOpen, setCreateOpen] = useState(false);
  const [createNotes, setCreateNotes] = useState('');
  const [scheduleOpen, setScheduleOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [confirmRestore, setConfirmRestore] = useState<string | null>(null);

  const { backupFlow, clearBackupFlow, restoreFlow, clearRestoreFlow } = store;

  const inBackup = backupFlow?.serverId === server.id;
  const inRestore = restoreFlow?.serverId === server.id;

  const deleteTarget = backups.find((b) => b.id === confirmDelete);
  const restoreTarget = backups.find((b) => b.id === confirmRestore);

  const startBackup = () => {
    setCreateOpen(false);
    store.startBackup(server.id, createNotes.trim());
    setCreateNotes('');
  };

  return (
    <div className="tab backups-tab">
      <div className="backups-head">
        <button className="btn btn-secondary" onClick={() => setScheduleOpen(true)}>
          Schedule
        </button>
        <button
          className="btn btn-primary"
          disabled={Boolean(inBackup || inRestore)}
          onClick={() => setCreateOpen(true)}
        >
          <IconPlus size={14} />
          Create backup
        </button>
      </div>

      {schedule.enabled && (
        <Callout tone="info" title={`Automated backups are on · ${schedule.frequency}${schedule.frequency === 'hourly' ? '' : ` at ${schedule.time}`}`}>
          Nooki keeps the last {schedule.keep} scheduled backup{schedule.keep !== 1 ? 's' : ''} and deletes older ones
          automatically.
        </Callout>
      )}

      {backups.length === 0 ? (
        <div className="backups-panel">
          <EmptyState
            icon={<IconBox size={40} />}
            title="No backups for this server yet"
            description="Create one manually or set a schedule. Nooki stores them outside the server folder so nothing you do to the world can damage the backups."
            action={
              <button className="btn btn-primary" onClick={() => setCreateOpen(true)}>
                Create backup
              </button>
            }
          />
        </div>
      ) : (
        <div className="backups-panel">
          <div className="backups-head-row">
            <span>Created</span>
            <span>Type</span>
            <span>Version</span>
            <span>Size</span>
            <span />
          </div>
          {backups.map((backup) => (
            <div key={backup.id} className={`backups-row ${backup.failed ? 'is-failed' : ''}`}>
              <div className="backups-when">
                <span className="backups-date">{formatDateTime(backup.createdAt)}</span>
                <span className="backups-detail">{formatRelative(backup.createdAt)}</span>
                {backup.notes && <span className="backups-notes">{backup.notes}</span>}
              </div>
              <span className="backups-cell">{typeLabels[backup.type]}</span>
              <span className="backups-cell mono">{backup.version}</span>
              <span className="backups-cell mono">{backup.failed ? '—' : formatBytes(backup.size)}</span>
              <div className="backups-actions">
                {backup.failed ? (
                  <button className="btn btn-sm btn-ghost" onClick={() => setConfirmDelete(backup.id)}>
                    Remove
                  </button>
                ) : (
                  <>
                    <button
                      className="btn btn-sm btn-secondary"
                      disabled={server.status === 'running'}
                      onClick={() => setConfirmRestore(backup.id)}
                    >
                      Restore
                    </button>
                    <button
                      className="btn btn-sm btn-ghost"
                      onClick={() => store.revealPath(backup.path)}
                    >
                      <IconFolder size={13} />
                    </button>
                    <button className="btn btn-sm btn-ghost" onClick={() => setConfirmDelete(backup.id)}>
                      Delete
                    </button>
                  </>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* ---------------------------- Create ---------------------------- */}

      <Modal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        title="Create a backup"
        description={`Nooki copies ${server.name} to a safe location. You can restore it later from this list.`}
        width={480}
        footer={
          <>
            <button className="btn btn-secondary" onClick={() => setCreateOpen(false)}>
              Cancel
            </button>
            <button className="btn btn-primary" data-autofocus onClick={startBackup}>
              Create backup
            </button>
          </>
        }
      >
        <Field label="Notes (optional)" hint="A quick reminder of why you made this backup.">
          <input
            className="input"
            placeholder="Before building the new village"
            value={createNotes}
            onChange={(e) => setCreateNotes(e.target.value)}
          />
        </Field>
        {server.status === 'running' && (
          <Callout tone="info" title="World saving is paused briefly">
            Players can keep playing. Nooki takes a clean snapshot and resumes normal saving right after.
          </Callout>
        )}
      </Modal>

      {/* --------------------------- Schedule --------------------------- */}

      <Modal
        open={scheduleOpen}
        onClose={() => setScheduleOpen(false)}
        title="Backup schedule"
        description={`Nooki creates a backup of ${server.name} on the schedule below. Manual and pre-update backups are kept separately.`}
        width={480}
        footer={
          <button className="btn btn-primary" onClick={() => setScheduleOpen(false)}>
            Done
          </button>
        }
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--s-5)' }}>
          <Toggle
            checked={schedule.enabled}
            onChange={(enabled) => store.setSchedule(server.id, { ...schedule, enabled })}
            label="Run automatic backups"
            hint="Schedules run while Nooki is open, including while it is hidden in the tray."
          />

          {schedule.enabled && (
            <>
              <div className="two-col">
                <Field label="How often">
                  <Select
                    value={schedule.frequency}
                    options={[{ value: 'hourly', label: 'Hourly' }, { value: 'daily', label: 'Daily' }, { value: 'weekly', label: 'Weekly' }]}
                    onChange={(frequency) => store.setSchedule(server.id, { ...schedule, frequency: frequency as typeof schedule.frequency })}
                  />
                </Field>
                {schedule.frequency !== 'hourly' && <Field label="At what time">
                  <input type="time" className="input mono" value={schedule.time} onChange={(e) => store.setSchedule(server.id, { ...schedule, time: e.target.value })} />
                </Field>}
              </div>

              {schedule.frequency === 'weekly' && (
                <Field label="Day of week">
                  <Select
                    value={String(schedule.weekday ?? 0)}
                    options={['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'].map((day, index) => ({ value: String(index), label: day }))}
                    onChange={(weekday) => store.setSchedule(server.id, { ...schedule, weekday: Number(weekday) })}
                  />
                </Field>
              )}

              <Field label="Keep the most recent" hint="Older backups are deleted automatically to save disk space.">
                <Select
                  value={String(schedule.keep)}
                  options={[3, 5, 7, 10, 14, 20].map((count) => ({ value: String(count), label: `${count} backup${count !== 1 ? 's' : ''}` }))}
                  onChange={(keep) => store.setSchedule(server.id, { ...schedule, keep: Number(keep) })}
                />
              </Field>
            </>
          )}
        </div>
      </Modal>

      {/* ---------------------------- Delete ---------------------------- */}

      <ConfirmDialog
        open={confirmDelete !== null}
        title={`Delete this backup?`}
        description={
          deleteTarget
            ? `${formatDateTime(deleteTarget.createdAt)} · ${typeLabels[deleteTarget.type]}${
                deleteTarget.notes ? ` · ${deleteTarget.notes}` : ''
              }`
            : ''
        }
        confirmLabel="Delete backup"
        tone="danger"
        notes={['You cannot undo this. The backup file is deleted from disk.']}
        onCancel={() => setConfirmDelete(null)}
        onConfirm={() => {
          if (confirmDelete) store.deleteBackup(confirmDelete);
          setConfirmDelete(null);
        }}
      />

      {/* --------------------------- Restore ---------------------------- */}

      <ConfirmDialog
        open={confirmRestore !== null && !inRestore}
        title={`Restore ${server.name}?`}
        description={
          restoreTarget
            ? `This replaces the current world with the backup from ${formatDateTime(restoreTarget.createdAt)}.`
            : ''
        }
        confirmLabel="Restore backup"
        tone="danger"
        notes={[
          'The server must be stopped before restoring.',
          'Nooki creates a safety backup of the current world before replacing it.',
        ]}
        onCancel={() => setConfirmRestore(null)}
        onConfirm={() => {
          if (restoreTarget) store.startRestore(restoreTarget.id);
          setConfirmRestore(null);
        }}
      />

      {/* ------------------------- Progress flow ------------------------ */}

      <Modal
        open={inBackup}
        onClose={backupFlow?.phase === 'done' || backupFlow?.phase === 'failed' ? clearBackupFlow : () => {}}
        dismissable={backupFlow?.phase !== 'running'}
        title={
          backupFlow?.phase === 'done'
            ? 'Backup finished'
            : backupFlow?.phase === 'cancelled'
              ? 'Backup cancelled'
            : backupFlow?.phase === 'failed'
              ? 'Backup failed'
              : 'Creating a backup'
        }
        description={
          backupFlow?.phase === 'done'
            ? `${server.name} is backed up and safe.`
            : backupFlow?.phase === 'failed'
              ? undefined
              : `Saving ${server.name} to a safe location.`
        }
        width={480}
        footer={
          backupFlow?.phase === 'done' || backupFlow?.phase === 'failed' || backupFlow?.phase === 'cancelled' ? (
            <button className="btn btn-primary" onClick={clearBackupFlow}>
              Close
            </button>
          ) : (
            <><span className="text-muted text-sm">Partial archives are discarded</span><button className="btn btn-secondary" disabled={!backupFlow?.operationId} onClick={() => backupFlow?.operationId && void store.cancelOperation(backupFlow.operationId)}>Cancel backup</button></>
          )
        }
      >
        {backupFlow?.phase === 'running' && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--s-3)' }}>
            <ProgressBar value={backupFlow.progress} tone="info" />
            <p className="text-sm text-secondary" style={{ textAlign: 'center' }}>
              {backupFlow.message}
            </p>
          </div>
        )}
        {backupFlow?.phase === 'failed' && (
          <Callout tone="error" title="Could not finish">
            {backupFlow.message}
          </Callout>
        )}
        {backupFlow?.phase === 'cancelled' && <Callout tone="info" title="Backup cancelled">{backupFlow.message}</Callout>}
      </Modal>

      <Modal
        open={inRestore}
        onClose={restoreFlow?.phase === 'done' || restoreFlow?.phase === 'failed' || restoreFlow?.phase === 'cancelled' ? clearRestoreFlow : () => {}}
        dismissable={restoreFlow?.phase !== 'safety' && restoreFlow?.phase !== 'restoring'}
        title={
          restoreFlow?.phase === 'done'
            ? 'Restore finished'
            : restoreFlow?.phase === 'cancelled'
              ? 'Restore cancelled'
            : restoreFlow?.phase === 'failed'
              ? 'Restore failed'
              : 'Restoring a backup'
        }
        description={
          restoreFlow?.phase === 'done'
            ? `${server.name} is back to the saved state.`
            : restoreFlow?.phase === 'failed'
              ? undefined
              : `Replacing the world files with the backup you chose.`
        }
        width={480}
        footer={
          restoreFlow?.phase === 'done' || restoreFlow?.phase === 'failed' || restoreFlow?.phase === 'cancelled' ? (
            <button className="btn btn-primary" onClick={clearRestoreFlow}>
              Close
            </button>
          ) : (
            <><span className="text-muted text-sm">Existing data stays protected</span><button className="btn btn-secondary" disabled={!restoreFlow?.operationId} onClick={() => restoreFlow?.operationId && void store.cancelOperation(restoreFlow.operationId)}>Cancel restore</button></>
          )
        }
      >
        {restoreFlow && ['safety', 'restoring'].includes(restoreFlow.phase) && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--s-3)' }}>
            <ProgressBar value={restoreFlow.progress} tone="warning" />
            <p className="text-sm text-secondary" style={{ textAlign: 'center' }}>
              {restoreFlow.message}
            </p>
          </div>
        )}
        {restoreFlow?.phase === 'done' && (
          <Callout tone="success" title="The world was replaced">
            A safety backup of what was there before is in the backup list. Start the server when you are ready.
          </Callout>
        )}
        {restoreFlow?.phase === 'failed' && (
          <Callout tone="error" title="Something went wrong">
            {restoreFlow.message}
          </Callout>
        )}
        {restoreFlow?.phase === 'cancelled' && <Callout tone="info" title="Restore cancelled">{restoreFlow.message}</Callout>}
      </Modal>
    </div>
  );
}
