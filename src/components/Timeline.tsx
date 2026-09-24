import type { ActivityEvent, ActivityKind } from '../types';
import { IconBox, IconDownload, IconPlay, IconRefresh, IconSettings, IconShare, IconStop, IconWarning } from './Icons';
import { formatRelative } from '../format';
import './Timeline.css';

function activityIcon(kind: ActivityKind) {
  switch (kind) {
    case 'start': return <IconPlay size={11} />;
    case 'stop': return <IconStop size={10} />;
    case 'restart': return <IconRefresh size={11} />;
    case 'crash': return <IconWarning size={11} />;
    case 'backup': case 'restore': return <IconBox size={11} />;
    case 'update': return <IconDownload size={11} />;
    case 'sharing': return <IconShare size={11} />;
    default: return <IconSettings size={11} />;
  }
}

export default function Timeline({ events, showServer = false }: { events: ActivityEvent[]; showServer?: boolean }) {
  return (
    <ol className="timeline">
      {events.map((event) => (
        <li key={event.id} className={`timeline-item kind-${event.kind}`}>
          <span className="timeline-icon" aria-hidden="true">{activityIcon(event.kind)}</span>
          <div className="timeline-body">
            <span className="timeline-msg">{event.message}</span>
            <span className="timeline-meta">
              {showServer && event.serverName && <span className="timeline-server">{event.serverName}</span>}
              <span>{formatRelative(event.at)}</span>
            </span>
          </div>
        </li>
      ))}
    </ol>
  );
}
