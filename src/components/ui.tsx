import { useState, type PointerEvent as ReactPointerEvent, type ReactElement, type ReactNode } from 'react';
import { open as openDirectory } from '@tauri-apps/plugin-dialog';
import { CircleAlert, LoaderCircle } from 'lucide-react';
import { IconCheck, IconWarning, IconX } from './Icons';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog';
import { Switch as ShadcnSwitch } from './ui/switch';
import { Label } from './ui/label';
import { Select as ShadcnSelect, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './ui/dropdown-menu';
import { Progress as ShadcnProgress } from './ui/progress';
import { Alert, AlertAction, AlertDescription, AlertTitle } from './ui/alert';
import './ui.css';

/* ------------------------------- Modal ------------------------------- */

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children?: ReactNode;
  footer?: ReactNode;
  width?: number;
  tone?: 'default' | 'danger';
  dismissable?: boolean;
  className?: string;
}

export function Modal({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 480,
  tone = 'default',
  dismissable = true,
  className = '',
}: ModalProps) {
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => { if (!nextOpen && dismissable) onClose(); }}>
      <DialogContent
        className={`modal ${tone === 'danger' ? 'modal-danger' : ''} ${className}`}
        style={{ width, maxWidth: 'calc(100vw - 32px)' }}
        showCloseButton={dismissable}
      >
        <DialogHeader className="modal-head">
          <DialogTitle className="modal-title">{title}</DialogTitle>
          {description && <DialogDescription className="modal-desc">{description}</DialogDescription>}
        </DialogHeader>
        {children && <div className="modal-body">{children}</div>}
        {footer && <DialogFooter className="modal-foot">{footer}</DialogFooter>}
      </DialogContent>
    </Dialog>
  );
}

/* --------------------------- Confirm dialog -------------------------- */

interface ConfirmProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  cancelLabel?: string;
  tone?: 'default' | 'danger';
  notes?: string[];
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  cancelLabel = 'Cancel',
  tone = 'default',
  notes,
  onConfirm,
  onCancel,
}: ConfirmProps) {
  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={title}
      description={description}
      width={440}
      tone={tone}
      footer={
        <>
          <button className="btn btn-secondary" onClick={onCancel}>
            {cancelLabel}
          </button>
          <button
            className={tone === 'danger' ? 'btn btn-danger' : 'btn btn-primary'}
            onClick={onConfirm}
            data-autofocus
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      {notes && notes.length > 0 && (
        <ul className="note-list">
          {notes.map((note) => (
            <li key={note}>
              <IconWarning size={13} />
              <span>{note}</span>
            </li>
          ))}
        </ul>
      )}
    </Modal>
  );
}

/* ------------------------------- Toggle ------------------------------ */

interface ToggleProps {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  hint?: string;
  error?: string;
  disabled?: boolean;
  restartHint?: boolean;
}

export function Toggle({ checked, onChange, label, hint, error, disabled, restartHint }: ToggleProps) {
  return (
    <label className={`toggle-row ${disabled ? 'is-disabled' : ''}`}>
      <span className="toggle-text">
        <span className="toggle-label">
          {label}
          {restartHint && <span className="restart-tag">restart needed</span>}
          {error && <span className="toggle-inline-error"><IconWarning size={11} /> {error}</span>}
        </span>
        {hint && <span className="toggle-hint">{hint}</span>}
      </span>
      <ShadcnSwitch
        checked={checked}
        onCheckedChange={onChange}
        aria-label={label}
        disabled={disabled}
      />
    </label>
  );
}

/* ------------------------------- Field ------------------------------- */

interface FieldProps {
  label: string;
  hint?: string;
  error?: string;
  restartHint?: boolean;
  children: ReactNode;
  htmlFor?: string;
}

export function Field({ label, hint, error, restartHint, children, htmlFor }: FieldProps) {
  return (
    <div className={`field ${error ? 'has-error' : ''}`}>
      <Label className="field-label" htmlFor={htmlFor}>
        {label}
        {restartHint && <span className="restart-tag">restart needed</span>}
      </Label>
      {children}
      {error ? <span className="field-error">{error}</span> : hint ? <span className="field-hint">{hint}</span> : null}
    </div>
  );
}

/* ------------------------------- Select ------------------------------ */

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

interface SelectProps {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  ariaLabel?: string;
  className?: string;
}

export function Select({
  value,
  options,
  onChange,
  placeholder = 'Select an option',
  disabled = false,
  ariaLabel,
  className = '',
}: SelectProps) {
  return (
    <div className={`custom-select ${disabled ? 'is-disabled' : ''} ${className}`}>
      <ShadcnSelect value={value || null} onValueChange={(next) => onChange(String(next))} disabled={disabled || options.length === 0}>
        <SelectTrigger className="custom-select-trigger" aria-label={ariaLabel}>
          <SelectValue>{options.find((option) => option.value === value)?.label ?? placeholder}</SelectValue>
        </SelectTrigger>
        <SelectContent align="start" alignItemWithTrigger={false}>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value} disabled={option.disabled}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </ShadcnSelect>
    </div>
  );
}

/* ----------------------------- Segmented ---------------------------- */

interface SegmentedProps<T extends string> {
  value: T;
  options: { value: T; label: string; hint?: string }[];
  onChange: (value: T) => void;
  full?: boolean;
}

export function Segmented<T extends string>({ value, options, onChange, full }: SegmentedProps<T>) {
  return (
    <div className={`segmented ${full ? 'is-full' : ''}`} role="tablist">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="tab"
          aria-selected={value === option.value}
          className={`segmented-btn ${value === option.value ? 'active' : ''}`}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/* ----------------------------- Progress ----------------------------- */

export function ProgressBar({ value, tone = 'accent' }: { value: number; tone?: 'accent' | 'warning' | 'danger' | 'info' }) {
  return <ShadcnProgress className={`progress tone-${tone}`} value={Math.max(0, Math.min(100, value))} />;
}

export function Meter({
  label,
  value,
  max,
  display,
  tone = 'accent',
}: {
  label: string;
  value: number;
  max: number;
  display: string;
  tone?: 'accent' | 'warning' | 'danger' | 'info';
}) {
  const pct = max > 0 ? (value / max) * 100 : 0;
  return (
    <div className="meter">
      <div className="meter-head">
        <span className="meter-label">{label}</span>
        <span className="meter-value">{display}</span>
      </div>
      <ProgressBar value={pct} tone={tone} />
    </div>
  );
}

/* ----------------------------- Sparkline ---------------------------- */

export function Sparkline({
  data,
  color = 'var(--accent)',
  height = 40,
  label,
  maxValue,
  showStatus = false,
}: {
  data: number[];
  color?: string;
  height?: number;
  label?: string;
  maxValue?: number;
  showStatus?: boolean;
}) {
  const values = data.filter(Number.isFinite).map((value) => Math.max(0, value));
  const domainMax = Math.max(1, maxValue ?? chartDomainMax(values));
  const chartTop = 8;
  const chartBottom = 92;
  const yFor = (value: number) => chartBottom - (Math.min(domainMax, value) / domainMax) * (chartBottom - chartTop);
  const points = values
    .map((v, i) => {
      const x = (i / Math.max(1, values.length - 1)) * 100;
      const y = yFor(v);
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(' ');
  const lastValue = values.length > 0 ? values[values.length - 1] : undefined;
  const lastY = lastValue === undefined ? chartBottom : yFor(lastValue);

  return (
    <div className="sparkline-frame" style={{ height }} aria-label={label}>
      <svg className="sparkline" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
        {[29, 50, 71, chartBottom].map((y) => <line key={y} className="sparkline-grid" x1="0" x2="100" y1={y} y2={y} />)}
        {values.length > 1 && <polyline className="sparkline-line" points={points} fill="none" stroke={color} strokeWidth="1.6" vectorEffect="non-scaling-stroke" />}
        {lastValue !== undefined && <circle className="sparkline-point" cx="99" cy={lastY} r="2.2" fill={color} vectorEffect="non-scaling-stroke" />}
      </svg>
      {showStatus && values.length < 2 && <span className="sparkline-status">{values.length === 0 ? 'Waiting for data' : 'Collecting history'}</span>}
    </div>
  );
}

export function MetricChart({
  data,
  color = 'var(--accent)',
  maxValue,
  label,
  startAt,
  endAt,
}: {
  data: Array<{ at: number; value: number }>;
  color?: string;
  maxValue: number;
  label: string;
  startAt: number;
  endAt: number;
}) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const values = data
    .filter((point) => Number.isFinite(point.at) && Number.isFinite(point.value))
    .map((point) => ({ at: point.at, value: Math.max(0, point.value) }))
    .sort((a, b) => a.at - b.at);
  const domainMax = Math.max(1, maxValue);
  const width = 600;
  const height = 132;
  const top = 8;
  const bottom = 124;
  const firstSampleAt = values[0]?.at ?? startAt;
  const lastSampleAt = values[values.length - 1]?.at ?? endAt;
  const domainStart = Math.min(startAt, firstSampleAt);
  const domainEnd = Math.max(domainStart + 1, endAt, lastSampleAt);
  const duration = domainEnd - domainStart;
  const yFor = (value: number) => bottom - (Math.min(domainMax, value) / domainMax) * (bottom - top);
  const xFor = (at: number) => Math.max(0, Math.min(width, ((at - domainStart) / duration) * width));
  const points = values.map((point) => {
    return `${xFor(point.at).toFixed(2)},${yFor(point.value).toFixed(2)}`;
  }).join(' ');
  const last = values.length > 0 ? values[values.length - 1] : undefined;
  const lastX = last === undefined ? 0 : xFor(last.at);
  const lastY = last === undefined ? bottom : yFor(last.value);
  const hovered = hoveredIndex === null ? undefined : values[hoveredIndex];
  const hoveredX = hovered === undefined ? 0 : xFor(hovered.at);
  const hoveredY = hovered === undefined ? bottom : yFor(hovered.value);
  const midpoint = Math.round(domainMax / 2);
  const ticks = Array.from({ length: 5 }, (_, index) => domainStart + (duration * index) / 4);
  const timeFormatter = new Intl.DateTimeFormat(undefined, {
    month: duration >= 86_400_000 ? 'short' : undefined,
    day: duration >= 86_400_000 ? 'numeric' : undefined,
    hour: '2-digit',
    minute: '2-digit',
    second: duration < 600_000 ? '2-digit' : undefined,
  });
  const hoverTimeFormatter = new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (values.length === 0) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (event.clientX - bounds.left) / Math.max(1, bounds.width)));
    const targetAt = domainStart + ratio * duration;
    let low = 0;
    let high = values.length - 1;
    while (low < high) {
      const middle = Math.floor((low + high) / 2);
      if (values[middle].at < targetAt) low = middle + 1;
      else high = middle;
    }
    const nearest = low > 0 && Math.abs(values[low - 1].at - targetAt) < Math.abs(values[low].at - targetAt)
      ? low - 1
      : low;
    setHoveredIndex(nearest);
  };
  const hoverValue = hovered === undefined
    ? ''
    : `${hovered.value < 10 && !Number.isInteger(hovered.value) ? hovered.value.toFixed(1) : Math.round(hovered.value)}%`;
  const hoverPosition = hovered === undefined ? 0 : (hoveredX / width) * 100;
  const hoverTransform = hoverPosition < 14 ? 'translateX(0)' : hoverPosition > 86 ? 'translateX(-100%)' : 'translateX(-50%)';

  return (
    <div className="metric-chart" role="img" aria-label={`${label} usage over time`}>
      <div className="metric-chart-y" aria-hidden="true">
        <span>{domainMax}%</span>
        <span>{midpoint}%</span>
        <span>0%</span>
      </div>
      <div className="metric-chart-body">
        <div className="metric-chart-plot" onPointerMove={handlePointerMove} onPointerLeave={() => setHoveredIndex(null)}>
          <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-hidden="true">
            {[top, (top + bottom) / 2, bottom].map((y) => (
              <line key={`h-${y}`} className="metric-chart-grid" x1="0" x2={width} y1={y} y2={y} />
            ))}
            {[150, 300, 450].map((x) => (
              <line key={`v-${x}`} className="metric-chart-grid metric-chart-grid-vertical" x1={x} x2={x} y1={top} y2={bottom} />
            ))}
            {values.length > 1 && (
              <polyline className="metric-chart-line" points={points} fill="none" stroke={color} vectorEffect="non-scaling-stroke" />
            )}
            {hovered !== undefined && (
              <g className="metric-chart-hover">
                <line className="metric-chart-crosshair" x1={hoveredX} x2={hoveredX} y1={top} y2={bottom} vectorEffect="non-scaling-stroke" />
                <circle className="metric-chart-hover-point" cx={hoveredX} cy={hoveredY} r="4" fill={color} vectorEffect="non-scaling-stroke" />
              </g>
            )}
            {last !== undefined && (
              <circle className="metric-chart-point" cx={lastX} cy={lastY} r="3" fill={color} vectorEffect="non-scaling-stroke" />
            )}
          </svg>
          {hovered !== undefined && (
            <div className="metric-chart-tooltip" style={{ left: `${hoverPosition}%`, transform: hoverTransform }}>
              <strong>{hoverValue}</strong>
              <span>{hoverTimeFormatter.format(hovered.at)}</span>
            </div>
          )}
        </div>
        <div className="metric-chart-x" aria-hidden="true">
          {ticks.map((at, index) => (
            <span
              key={`${at}-${index}`}
              style={{ left: `${index * 25}%`, transform: `translateX(-${index * 25}%)` }}
            >
              {timeFormatter.format(at)}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

export function chartDomainMax(data: number[], floor = 10): number {
  const peak = Math.max(0, ...data.filter(Number.isFinite));
  if (peak === 0) return floor;
  const step = peak <= 25 ? 5 : peak <= 50 ? 10 : 25;
  return Math.max(floor, Math.ceil((peak * 1.12) / step) * step);
}

/* ------------------------------ Avatar ------------------------------ */

export function Avatar({ name, color, size = 32 }: { name: string; color: string; size?: number }) {
  return (
    <span
      className="avatar"
      style={{ width: size, height: size, background: color, fontSize: size * 0.4 }}
      aria-hidden="true"
    >
      {name.slice(0, 1).toUpperCase()}
    </span>
  );
}

/* ------------------------------- Menu ------------------------------- */

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  hint?: string;
}

export function Menu({ items, trigger, align = 'right' }: { items: MenuItem[]; trigger: ReactNode; align?: 'left' | 'right' }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={trigger as ReactElement} />
      <DropdownMenuContent align={align === 'right' ? 'end' : 'start'} className="min-w-52">
        {items.map((item) => (
          <DropdownMenuItem
            key={item.label}
            variant={item.danger ? 'destructive' : 'default'}
            disabled={item.disabled}
            onClick={item.onSelect}
          >
            <span>{item.label}</span>
            {item.hint && <span className="menu-hint">{item.hint}</span>}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/* ---------------------------- Empty state --------------------------- */

export function EmptyState({
  title,
  description,
  action,
  icon,
}: {
  title: string;
  description: string;
  action?: ReactNode;
  icon?: ReactNode;
}) {
  return (
    <div className="empty">
      {icon && <div className="empty-icon">{icon}</div>}
      <p className="empty-title">{title}</p>
      <p className="empty-desc">{description}</p>
      {action && <div className="empty-action">{action}</div>}
    </div>
  );
}

/* ------------------------------ Callout ----------------------------- */

export function Callout({
  tone = 'info',
  title,
  children,
  action,
  onDismiss,
}: {
  tone?: 'info' | 'warning' | 'error' | 'success';
  title: string;
  children?: ReactNode;
  action?: ReactNode;
  onDismiss?: () => void;
}) {
  return (
    <Alert className={`callout tone-${tone}`} variant={tone === 'error' ? 'destructive' : 'default'}>
      <CircleAlert />
      <AlertTitle className="callout-title">{title}</AlertTitle>
      {children && <AlertDescription className="callout-text">{children}</AlertDescription>}
      {(action || onDismiss) && <AlertAction className="callout-actions">
        {action}
        {onDismiss && (
          <button className="icon-btn" onClick={onDismiss} aria-label="Dismiss">
            <IconX size={13} />
          </button>
        )}
      </AlertAction>}
    </Alert>
  );
}

/* ------------------------------ Stepper ----------------------------- */

export function Stepper({ steps, current }: { steps: string[]; current: number }) {
  return (
    <ol className="stepper">
      {steps.map((step, i) => {
        const state = i < current ? 'done' : i === current ? 'active' : 'todo';
        return (
          <li key={step} className={`stepper-item is-${state}`}>
            <span className="stepper-dot">{state === 'done' ? <IconCheck size={11} /> : i + 1}</span>
            <span className="stepper-label">{step}</span>
          </li>
        );
      })}
    </ol>
  );
}

/* --------------------------- Folder picker -------------------------- */

export function FolderPicker({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (path: string) => void;
  suggestions?: string[];
  disabled?: boolean;
}) {
  const browse = async () => {
    const selected = await openDirectory({ directory: true, multiple: false, defaultPath: value || undefined });
    if (typeof selected === 'string') onChange(selected);
  };

  return (
    <div className="picker">
      <input className="input mono" value={value} onChange={(e) => onChange(e.target.value)} disabled={disabled} />
      <button type="button" className="btn btn-secondary" onClick={() => void browse()} disabled={disabled}>Browse</button>
    </div>
  );
}

/* ------------------------------ Spinner ----------------------------- */

export function Spinner({ size = 14 }: { size?: number }) {
  return <LoaderCircle className="spinner" style={{ width: size, height: size }} aria-hidden="true" />;
}
