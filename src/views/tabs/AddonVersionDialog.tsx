import type { AddonVersionOption } from '../../types';
import { Field, Modal, Select, Spinner } from '../../components/ui';
import { formatRelative } from '../../format';

interface AddonVersionDialogProps {
  open: boolean;
  projectName: string;
  kind: 'mod' | 'plugin';
  versions: AddonVersionOption[];
  selectedId: string;
  loading: boolean;
  error: string;
  onSelect: (id: string) => void;
  onClose: () => void;
  onInstall: () => void;
}

function releaseLabel(value: string) {
  return value ? value.charAt(0).toUpperCase() + value.slice(1).toLowerCase() : 'Release';
}

export default function AddonVersionDialog({
  open,
  projectName,
  kind,
  versions,
  selectedId,
  loading,
  error,
  onSelect,
  onClose,
  onInstall,
}: AddonVersionDialogProps) {
  const selected = versions.find((version) => version.id === selectedId) ?? null;
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={`Install ${projectName}`}
      description={`Choose the ${kind} version to install.`}
      width={460}
      footer={<>
        <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
        <button className="btn btn-primary" disabled={loading || !selected || Boolean(error)} onClick={onInstall}>
          Install version
        </button>
      </>}
    >
      <div className="addon-version-dialog">
        {loading ? (
          <div className="addon-version-loading"><Spinner size={16} /><span>Loading compatible versions</span></div>
        ) : error ? (
          <div className="addon-version-error">{error}</div>
        ) : (
          <>
            <Field label="Version">
              <Select
                value={selectedId}
                onChange={onSelect}
                ariaLabel={`Choose ${projectName} version`}
                options={versions.map((version) => ({
                  value: version.id,
                  label: `${version.version}${version.releaseType.toLowerCase() === 'release' ? '' : ` · ${releaseLabel(version.releaseType)}`}`,
                }))}
              />
            </Field>
            {selected && (
              <div className="addon-version-details">
                <span><small>Channel</small><strong>{releaseLabel(selected.releaseType)}</strong></span>
                <span><small>Published</small><strong>{selected.publishedAt ? formatRelative(selected.publishedAt) : 'Unknown'}</strong></span>
                {!selected.automatic && <span><small>Download</small><strong>Manual</strong></span>}
              </div>
            )}
          </>
        )}
      </div>
    </Modal>
  );
}
