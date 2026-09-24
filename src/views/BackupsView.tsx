import { useMemo, useState } from 'react';
import { useStore } from '../state/store';
import type { BackupType } from '../types';
import { IconBox, IconFolder } from '../components/Icons';
import { Callout, ConfirmDialog, EmptyState, Modal, ProgressBar, Segmented, Select } from '../components/ui';
import { formatBytes, formatDateTime, formatRelative, formatUntil } from '../format';
import { PageActions } from '../components/TopBar';
import ServerIcon from '../components/ServerIcon';
import './tabs/ServerBackupsTab.css';
import './BackupsView.css';

const typeLabels: Record<BackupType, string> = { manual: 'Manual', scheduled: 'Scheduled', safety: 'Safety', 'pre-update': 'Pre-update' };

export default function BackupsView() {
  const store = useStore();
  const [serverId, setServerId] = useState('all');
  const [type, setType] = useState<'all' | BackupType>('all');
  const [restoreId, setRestoreId] = useState<string | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const visible = useMemo(() => store.backups.filter((backup) =>
    (serverId === 'all' || backup.serverId === serverId) && (type === 'all' || backup.type === type),
  ), [store.backups, serverId, type]);
  const total = store.backups.filter((backup) => !backup.failed).reduce((sum, backup) => sum + backup.size, 0);
  const restore = store.backups.find((backup) => backup.id === restoreId);
  const remove = store.backups.find((backup) => backup.id === deleteId);
  const upcoming = Object.entries(store.schedules)
    .filter(([, schedule]) => schedule.enabled && schedule.nextRunAt)
    .sort((a, b) => (a[1].nextRunAt ?? 0) - (b[1].nextRunAt ?? 0))[0];

  return (
    <div className="view">
      <PageActions>
        <button className="btn btn-sm btn-secondary" onClick={() => store.revealPath(store.settings.backupFolder)}><IconFolder size={13} /> Open folder</button>
      </PageActions>

      {store.backups.length > 0 && (
        <div className="page-toolbar">
          <div className="backups-server-filter">
            <Select
              value={serverId}
              ariaLabel="Filter backups by server"
              options={[{ value: 'all', label: 'All servers' }, ...store.servers.map((server) => ({ value: server.id, label: server.name }))]}
              onChange={setServerId}
            />
          </div>
          <Segmented value={type} onChange={setType} options={[
            { value: 'all', label: 'All' }, { value: 'manual', label: 'Manual' },
            { value: 'scheduled', label: 'Scheduled' }, { value: 'safety', label: 'Safety' },
            { value: 'pre-update', label: 'Pre-update' },
          ]} />
          <div className="grow" />
          <span className="backups-summary">
            {store.backups.length} backup{store.backups.length !== 1 ? 's' : ''} · {formatBytes(total)}
            {upcoming && <> · next {formatUntil(upcoming[1].nextRunAt!)} for {store.servers.find((server) => server.id === upcoming[0])?.name ?? 'Unknown server'}</>}
          </span>
        </div>
      )}

      <div className="page">
        {store.backups.length === 0 ? (
          <div className="page-empty">
            <EmptyState icon={<IconBox size={18} />} title="No backups yet" description="Backups are created manually or on a schedule. Open a server to set one up." />
          </div>
        ) : visible.length === 0 ? (
          <div className="page-empty">
            <EmptyState title="No backups match" description="Change the server or type filter." />
          </div>
        ) : (
          <div className="backups-table" role="table">
            <div className="backups-table-head" role="row"><span>Created</span><span>Server</span><span>Type</span><span>Version</span><span>Size</span><span /></div>
            {visible.map((backup) => {
              const server = store.servers.find((item) => item.id === backup.serverId);
              const missing = Boolean(backup.failed || backup.errorMessage);
              return <div key={backup.id} role="row" className={`backups-table-row ${missing ? 'is-failed' : ''}`}>
                <div className="backups-when">
                  <span className="backups-date">{formatDateTime(backup.createdAt)}</span>
                  <span className="backups-detail">{backup.notes ? backup.notes : formatRelative(backup.createdAt)}</span>
                </div>
                <span className="backups-cell backups-server">{server && <ServerIcon server={server} size={18} />}<span>{backup.serverName}</span></span>
                <span className="backups-cell"><span className={`backup-type type-${backup.type}`}>{typeLabels[backup.type]}</span></span>
                <span className="backups-cell mono">{backup.version}</span>
                <span className="backups-cell mono">{missing ? 'Missing' : formatBytes(backup.size)}</span>
                <div className="backups-actions">
                  {!missing && <button className="btn btn-sm btn-secondary" disabled={!server || server.status !== 'stopped'} title={server && server.status !== 'stopped' ? 'Stop the server to restore' : undefined} onClick={() => setRestoreId(backup.id)}>Restore</button>}
                  <button className="btn btn-sm btn-ghost" onClick={() => setDeleteId(backup.id)}>{missing ? 'Remove record' : 'Delete'}</button>
                </div>
              </div>;
            })}
          </div>
        )}
      </div>
      <ConfirmDialog open={Boolean(restore)} title={`Restore ${restore?.serverName ?? 'backup'}?`} description="The current managed server data will be replaced. Nooki creates a safety backup first." confirmLabel="Restore backup" tone="danger" onCancel={() => setRestoreId(null)} onConfirm={() => { if (restore) store.startRestore(restore.id); setRestoreId(null); }} />
      <ConfirmDialog open={Boolean(remove)} title="Delete this backup?" description={remove?.path ?? ''} confirmLabel="Delete backup" tone="danger" onCancel={() => setDeleteId(null)} onConfirm={() => { if (remove) store.deleteBackup(remove.id); setDeleteId(null); }} />
      <Modal open={store.restoreFlow !== null} onClose={store.restoreFlow?.phase === 'done' || store.restoreFlow?.phase === 'failed' || store.restoreFlow?.phase === 'cancelled' ? store.clearRestoreFlow : () => {}} dismissable={store.restoreFlow?.phase === 'done' || store.restoreFlow?.phase === 'failed' || store.restoreFlow?.phase === 'cancelled'} title={store.restoreFlow?.phase === 'done' ? 'Restore finished' : store.restoreFlow?.phase === 'cancelled' ? 'Restore cancelled' : store.restoreFlow?.phase === 'failed' ? 'Restore failed' : 'Restoring backup'} width={480} footer={store.restoreFlow?.phase === 'done' || store.restoreFlow?.phase === 'failed' || store.restoreFlow?.phase === 'cancelled' ? <button className="btn btn-primary" onClick={store.clearRestoreFlow}>Close</button> : <><span className="text-muted text-sm">Existing data stays protected</span><button className="btn btn-secondary" disabled={!store.restoreFlow?.operationId} onClick={() => store.restoreFlow?.operationId && void store.cancelOperation(store.restoreFlow.operationId)}>Cancel restore</button></>}>
        {store.restoreFlow && (store.restoreFlow.phase === 'safety' || store.restoreFlow.phase === 'restoring') && <div className="stack-sm"><ProgressBar value={store.restoreFlow.progress} tone="warning" /><span className="text-muted text-sm">{store.restoreFlow.message}</span></div>}
        {store.restoreFlow?.phase === 'done' && <Callout tone="success" title="Server data restored">A safety backup of the previous data remains in the backup list.</Callout>}
        {store.restoreFlow?.phase === 'failed' && <Callout tone="error" title="Restore did not finish">{store.restoreFlow.message}</Callout>}
        {store.restoreFlow?.phase === 'cancelled' && <Callout tone="info" title="Restore cancelled">{store.restoreFlow.message}</Callout>}
      </Modal>
    </div>
  );
}
