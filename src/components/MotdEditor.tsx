import { useEffect, useLayoutEffect, useRef, useState, type ClipboardEvent, type CSSProperties, type KeyboardEvent } from 'react';
import {
  IconBold,
  IconAlignCenter,
  IconAlignLeft,
  IconAlignRight,
  IconItalic,
  IconObfuscated,
  IconRemoveFormatting,
  IconStrikethrough,
  IconUnderline,
} from './Icons';
import './MotdEditor.css';

interface TextStyle {
  color: string | null;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  obfuscated: boolean;
}

const DEFAULT_STYLE: TextStyle = { color: null, bold: false, italic: false, underline: false, strike: false, obfuscated: false };

const MINECRAFT_COLORS = [
  { code: '0', name: 'Black', hex: '#000000' },
  { code: '1', name: 'Dark blue', hex: '#0000aa' },
  { code: '2', name: 'Dark green', hex: '#00aa00' },
  { code: '3', name: 'Dark aqua', hex: '#00aaaa' },
  { code: '4', name: 'Dark red', hex: '#aa0000' },
  { code: '5', name: 'Dark purple', hex: '#aa00aa' },
  { code: '6', name: 'Gold', hex: '#ffaa00' },
  { code: '7', name: 'Gray', hex: '#aaaaaa' },
  { code: '8', name: 'Dark gray', hex: '#555555' },
  { code: '9', name: 'Blue', hex: '#5555ff' },
  { code: 'a', name: 'Green', hex: '#55ff55' },
  { code: 'b', name: 'Aqua', hex: '#55ffff' },
  { code: 'c', name: 'Red', hex: '#ff5555' },
  { code: 'd', name: 'Light purple', hex: '#ff55ff' },
  { code: 'e', name: 'Yellow', hex: '#ffff55' },
  { code: 'f', name: 'White', hex: '#ffffff' },
] as const;

const COLOR_BY_CODE = new Map<string, string>(MINECRAFT_COLORS.map((color) => [color.code, color.hex]));
const CODE_BY_COLOR = new Map<string, string>(MINECRAFT_COLORS.map((color) => [color.hex, color.code]));
const MOTD_PIXEL_WIDTH = 270;

function cleanMotd(value: string) {
  return value
    .replace(/\\u00a7/gi, '§')
    .replace(/\\n/g, '\n')
    .replace(/\r/g, '')
    .split('\n')
    .slice(0, 2)
    .join('\n');
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character] ?? character);
}

function sameStyle(left: TextStyle, right: TextStyle) {
  return left.color === right.color
    && left.bold === right.bold
    && left.italic === right.italic
    && left.underline === right.underline
    && left.strike === right.strike
    && left.obfuscated === right.obfuscated;
}

function styleAttributes(style: TextStyle) {
  const declarations = [
    style.color ? `color:${style.color}` : '',
    style.bold ? 'font-weight:700' : '',
    style.italic ? 'font-style:italic' : '',
    style.underline || style.strike
      ? `text-decoration:${[style.underline ? 'underline' : '', style.strike ? 'line-through' : ''].filter(Boolean).join(' ')}`
      : '',
  ].filter(Boolean).join(';');
  return `${declarations ? ` style="${declarations}"` : ''}${style.color ? ` data-mc-color="${style.color}"` : ''}${style.obfuscated ? ' data-mc-obfuscated="true"' : ''}`;
}

function minecraftTextWidth(value: string) {
  const plain = value.replace(/[§&][0-9a-fk-or]/gi, '');
  let width = 0;
  for (const character of plain) {
    if (character === ' ') width += 4;
    else if ("iIl.,'!:;|".includes(character)) width += 2;
    else if ('[](){}tfrk'.includes(character)) width += 5;
    else if ('@MWmw%'.includes(character)) width += 7;
    else width += 6;
  }
  return width;
}

function alignmentPadding(value: string, alignment: 'left' | 'center' | 'right') {
  if (alignment === 'left') return 0;
  const remaining = Math.max(0, MOTD_PIXEL_WIDTH - minecraftTextWidth(value));
  return Math.floor(remaining / (alignment === 'center' ? 8 : 4));
}

function decodeAlignment(value: string) {
  const leading = value.match(/^ */)?.[0].length ?? 0;
  const content = value.slice(leading);
  if (leading < 2) return { alignment: 'left' as const, content: value };
  const center = alignmentPadding(content, 'center');
  const right = alignmentPadding(content, 'right');
  if (Math.abs(leading - right) <= 1) return { alignment: 'right' as const, content };
  if (Math.abs(leading - center) <= 1) return { alignment: 'center' as const, content };
  return { alignment: 'left' as const, content: value };
}

function motdToHtml(input: string) {
  const lines = cleanMotd(input).split('\n');
  let style = { ...DEFAULT_STYLE };
  let html = '';

  for (const rawLine of lines) {
    const { alignment, content } = decodeAlignment(rawLine);
    let buffer = '';
    let lineHtml = '';
    const flush = () => {
      if (!buffer) return;
      lineHtml += `<span${styleAttributes(style)}>${escapeHtml(buffer)}</span>`;
      buffer = '';
    };
    for (let index = 0; index < content.length; index += 1) {
      const character = content[index];
      const marker = (character === '§' || character === '&') ? content[index + 1]?.toLowerCase() : undefined;
      if (!marker || !/[0-9a-fk-or]/.test(marker)) {
        buffer += character;
        continue;
      }
      flush();
      index += 1;
      if (COLOR_BY_CODE.has(marker)) style = { ...DEFAULT_STYLE, color: COLOR_BY_CODE.get(marker) ?? null };
      else if (marker === 'k') style = { ...style, obfuscated: true };
      else if (marker === 'l') style = { ...style, bold: true };
      else if (marker === 'm') style = { ...style, strike: true };
      else if (marker === 'n') style = { ...style, underline: true };
      else if (marker === 'o') style = { ...style, italic: true };
      else if (marker === 'r') style = { ...DEFAULT_STYLE };
    }
    flush();
    html += `<div data-mc-align="${alignment}" style="text-align:${alignment}">${lineHtml || '<br>'}</div>`;
  }
  return html;
}

function normalizedHex(value: string | null) {
  if (!value) return null;
  const compact = value.trim().toLowerCase();
  if (/^#[0-9a-f]{6}$/.test(compact)) return compact;
  const match = compact.match(/^rgb\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)$/);
  if (!match) return null;
  return `#${match.slice(1).map((part) => Number(part).toString(16).padStart(2, '0')).join('')}`;
}

function elementStyle(element: HTMLElement, inherited: TextStyle): TextStyle {
  const tag = element.tagName.toLowerCase();
  const decoration = element.style.textDecoration.toLowerCase();
  return {
    color: normalizedHex(element.dataset.mcColor || element.getAttribute('color') || element.style.color) ?? inherited.color,
    bold: inherited.bold || tag === 'b' || tag === 'strong' || ['bold', '700', '800', '900'].includes(element.style.fontWeight),
    italic: inherited.italic || tag === 'i' || tag === 'em' || element.style.fontStyle === 'italic',
    underline: inherited.underline || tag === 'u' || decoration.includes('underline'),
    strike: inherited.strike || tag === 's' || tag === 'strike' || tag === 'del' || decoration.includes('line-through'),
    obfuscated: inherited.obfuscated || element.dataset.mcObfuscated === 'true',
  };
}

function stylePrefix(previous: TextStyle, next: TextStyle) {
  if (sameStyle(previous, next)) return '';
  const removedStyle = (previous.bold && !next.bold)
    || (previous.italic && !next.italic)
    || (previous.underline && !next.underline)
    || (previous.strike && !next.strike)
    || (previous.obfuscated && !next.obfuscated);
  const colorChanged = previous.color !== next.color;
  let prefix = '';
  if (removedStyle || (colorChanged && next.color === null)) prefix += '§r';
  if (colorChanged && next.color) prefix += `§${CODE_BY_COLOR.get(next.color) ?? 'f'}`;
  const rebuild = removedStyle || colorChanged;
  if ((rebuild || !previous.obfuscated) && next.obfuscated) prefix += '§k';
  if ((rebuild || !previous.bold) && next.bold) prefix += '§l';
  if ((rebuild || !previous.strike) && next.strike) prefix += '§m';
  if ((rebuild || !previous.underline) && next.underline) prefix += '§n';
  if ((rebuild || !previous.italic) && next.italic) prefix += '§o';
  return prefix;
}

function editorToMotd(root: HTMLElement) {
  let output = '';
  let active = { ...DEFAULT_STYLE };

  const appendText = (text: string, style: TextStyle) => {
    if (!text) return;
    output += stylePrefix(active, style) + text;
    active = { ...style };
  };

  const walk = (node: Node, inherited: TextStyle) => {
    if (node.nodeType === Node.TEXT_NODE) {
      appendText(node.textContent ?? '', inherited);
      return;
    }
    if (!(node instanceof HTMLElement)) return;
    if (node.tagName === 'BR') {
      if (!output.endsWith('\n')) output += '\n';
      return;
    }
    const block = node !== root && (node.tagName === 'DIV' || node.tagName === 'P');
    if (block && output && !output.endsWith('\n')) output += '\n';
    const lineStart = output.length;
    const next = elementStyle(node, inherited);
    node.childNodes.forEach((child) => walk(child, next));
    if (block) {
      const alignmentValue = node.style.textAlign || node.getAttribute('align') || node.dataset.mcAlign;
      const alignment = alignmentValue === 'center' || alignmentValue === 'right' ? alignmentValue : 'left';
      if (alignment !== 'left') {
        const line = output.slice(lineStart).replace(/^ +/, '');
        output = `${output.slice(0, lineStart)}${' '.repeat(alignmentPadding(line, alignment))}${line}`;
      }
    }
    if (block && !output.endsWith('\n')) output += '\n';
  };

  root.childNodes.forEach((node) => walk(node, DEFAULT_STYLE));
  return cleanMotd(output.replace(/\n+$/, ''));
}

export function MotdEditor({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const editorRef = useRef<HTMLDivElement>(null);
  const selectedRange = useRef<Range | null>(null);
  const emittedValue = useRef(cleanMotd(value));
  const initialized = useRef(false);
  const [hasSelection, setHasSelection] = useState(false);
  const [selectionObfuscated, setSelectionObfuscated] = useState(false);

  useLayoutEffect(() => {
    const editor = editorRef.current;
    const normalized = cleanMotd(value);
    if (!editor || (initialized.current && normalized === emittedValue.current)) return;
    editor.innerHTML = motdToHtml(normalized);
    emittedValue.current = normalized;
    initialized.current = true;
  }, [value]);

  useEffect(() => {
    const updateSelection = () => {
      const editor = editorRef.current;
      const selection = document.getSelection();
      if (!editor || !selection || selection.rangeCount === 0 || selection.isCollapsed) {
        setHasSelection(false);
        setSelectionObfuscated(false);
        return;
      }
      const range = selection.getRangeAt(0);
      if (!editor.contains(range.commonAncestorContainer)) {
        setHasSelection(false);
        setSelectionObfuscated(false);
        return;
      }
      const commonElement = range.commonAncestorContainer instanceof HTMLElement
        ? range.commonAncestorContainer
        : range.commonAncestorContainer.parentElement;
      selectedRange.current = range.cloneRange();
      setHasSelection(true);
      setSelectionObfuscated(Boolean(commonElement?.closest('[data-mc-obfuscated="true"]')));
    };
    document.addEventListener('selectionchange', updateSelection);
    return () => document.removeEventListener('selectionchange', updateSelection);
  }, []);

  const emit = () => {
    const editor = editorRef.current;
    if (!editor) return;
    const next = editorToMotd(editor);
    emittedValue.current = next;
    onChange(next);
  };

  const restoreSelection = () => {
    const range = selectedRange.current;
    if (!range) return false;
    const selection = document.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    return true;
  };

  const format = (command: string, value?: string) => {
    if (!restoreSelection()) return;
    document.execCommand(command, false, value);
    emit();
  };

  const obfuscate = () => {
    if (!restoreSelection()) return;
    const editor = editorRef.current;
    const selection = document.getSelection();
    if (!editor || !selection || selection.rangeCount === 0) return;
    const range = selection.getRangeAt(0);
    const commonElement = range.commonAncestorContainer instanceof HTMLElement
      ? range.commonAncestorContainer
      : range.commonAncestorContainer.parentElement;
    const existing = commonElement?.closest<HTMLElement>('[data-mc-obfuscated="true"]');
    if (existing && editor.contains(existing)) {
      const first = existing.firstChild;
      const last = existing.lastChild;
      const parent = existing.parentNode;
      if (!first || !last || !parent) return;
      while (existing.firstChild) parent.insertBefore(existing.firstChild, existing);
      existing.remove();
      const nextRange = document.createRange();
      nextRange.setStartBefore(first);
      nextRange.setEndAfter(last);
      selection.removeAllRanges();
      selection.addRange(nextRange);
      selectedRange.current = nextRange.cloneRange();
      setSelectionObfuscated(false);
      emit();
      return;
    }
    const wrapper = document.createElement('span');
    wrapper.dataset.mcObfuscated = 'true';
    wrapper.append(range.extractContents());
    range.insertNode(wrapper);
    selection.selectAllChildren(wrapper);
    selectedRange.current = selection.getRangeAt(0).cloneRange();
    setSelectionObfuscated(true);
    emit();
  };

  const limitLines = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Enter') return;
    const lines = (editorRef.current?.innerText ?? '').replace(/\r/g, '').split('\n').length;
    if (lines >= 2) event.preventDefault();
  };

  const paste = (event: ClipboardEvent<HTMLDivElement>) => {
    event.preventDefault();
    const currentLines = (editorRef.current?.innerText ?? '').replace(/\r/g, '').split('\n').length;
    const text = event.clipboardData.getData('text/plain').replace(/\r/g, '').split('\n').slice(0, Math.max(1, 3 - currentLines)).join('\n');
    document.execCommand('insertText', false, text);
  };

  return (
    <div className={`motd-editor ${hasSelection ? 'has-selection' : ''}`}>
      <div className="motd-toolbar" aria-hidden={!hasSelection} onMouseDown={(event) => event.preventDefault()}>
        <div className="motd-format-actions">
          <button type="button" onClick={() => format('bold')} title="Bold" aria-label="Bold"><IconBold size={14} /></button>
          <button type="button" onClick={() => format('italic')} title="Italic" aria-label="Italic"><IconItalic size={14} /></button>
          <button type="button" onClick={() => format('underline')} title="Underline" aria-label="Underline"><IconUnderline size={14} /></button>
          <button type="button" onClick={() => format('strikeThrough')} title="Strikethrough" aria-label="Strikethrough"><IconStrikethrough size={14} /></button>
          <button type="button" className={selectionObfuscated ? 'active' : ''} aria-pressed={selectionObfuscated} onClick={obfuscate} title="Obfuscated" aria-label="Obfuscated"><IconObfuscated size={14} /></button>
          <button type="button" onClick={() => format('removeFormat')} title="Clear formatting" aria-label="Clear formatting"><IconRemoveFormatting size={14} /></button>
        </div>
        <span className="motd-toolbar-divider" />
        <div className="motd-format-actions" aria-label="Line alignment">
          <button type="button" onClick={() => format('justifyLeft')} title="Align left" aria-label="Align left"><IconAlignLeft size={14} /></button>
          <button type="button" onClick={() => format('justifyCenter')} title="Center in the server list" aria-label="Center in the server list"><IconAlignCenter size={14} /></button>
          <button type="button" onClick={() => format('justifyRight')} title="Align right" aria-label="Align right"><IconAlignRight size={14} /></button>
        </div>
        <span className="motd-toolbar-divider" />
        <div className="motd-colors" aria-label="Text color">
          {MINECRAFT_COLORS.map((color) => (
            <button
              type="button"
              key={color.code}
              className={`motd-color motd-color-${color.code}`}
              style={{ '--motd-color': color.hex } as CSSProperties}
              title={color.name}
              aria-label={color.name}
              onClick={() => format('foreColor', color.hex)}
            />
          ))}
        </div>
      </div>
      <div
        ref={editorRef}
        className="motd-content"
        contentEditable
        role="textbox"
        aria-label="Message in the server list"
        aria-multiline="true"
        spellCheck
        suppressContentEditableWarning
        onInput={emit}
        onKeyDown={limitLines}
        onPaste={paste}
      />
      <span className="motd-line-count">{Math.min(2, cleanMotd(value).split('\n').length)}/2 lines</span>
    </div>
  );
}
