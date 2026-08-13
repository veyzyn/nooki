import Editor, { type Monaco, type OnMount } from '@monaco-editor/react';
import { useRef } from 'react';
import { IconChevronRight, IconFileText, IconSave } from '../../components/Icons';
import { Spinner } from '../../components/ui';
import { formatBytes } from '../../format';
import type { ServerTextFile } from '../../types';
import '../../monaco';

interface Props {
  serverId: string;
  file: ServerTextFile;
  draft: string;
  dirty: boolean;
  saving: boolean;
  onDraftChange: (value: string) => void;
  onSave: () => void;
  onClose: () => void;
}

export default function ServerFileEditor({ serverId, file, draft, dirty, saving, onDraftChange, onSave, onClose }: Props) {
  const saveRef = useRef(onSave);
  saveRef.current = onSave;
  const fileName = file.path.split('/').pop() ?? file.path;

  const configureEditor = (monaco: Monaco) => {
    monaco.editor.defineTheme('nooki', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '777781', fontStyle: 'italic' },
        { token: 'string', foreground: 'D2B88A' },
        { token: 'number', foreground: '83B5E5' },
        { token: 'keyword', foreground: 'B39DDB' },
        { token: 'delimiter', foreground: 'A3A3AD' },
      ],
      colors: {
        'editor.background': '#111113',
        'editor.foreground': '#E5E5E8',
        'editorLineNumber.foreground': '#55555D',
        'editorLineNumber.activeForeground': '#A7A7AE',
        'editorCursor.foreground': '#C7C7CE',
        'editor.selectionBackground': '#4A52665C',
        'editor.inactiveSelectionBackground': '#4A526638',
        'editor.lineHighlightBackground': '#FFFFFF07',
        'editorIndentGuide.background1': '#FFFFFF0B',
        'editorIndentGuide.activeBackground1': '#FFFFFF29',
        'editorWidget.background': '#1A1A1E',
        'editorWidget.border': '#FFFFFF18',
        'input.background': '#0D0D0F',
        'input.border': '#FFFFFF18',
        'focusBorder': '#7C849766',
        'scrollbarSlider.background': '#FFFFFF16',
        'scrollbarSlider.hoverBackground': '#FFFFFF25',
        'minimap.background': '#111113',
      },
    });
  };

  const mountEditor: OnMount = (editor, monaco) => {
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => saveRef.current());
    editor.focus();
  };

  return (
    <div className="files-editor-content">
      <div className="files-editor-toolbar">
        <div className="files-editor-title">
          <button className="icon-btn" onClick={onClose} aria-label="Back to files"><IconChevronRight className="files-back-icon" size={16} /></button>
          <span className="files-editor-file-icon"><IconFileText size={16} /></span>
          <div>
            <strong>{fileName}</strong>
            <span>{file.path}</span>
          </div>
          {dirty && <span className="files-dirty">Unsaved</span>}
        </div>
        <div className="files-editor-actions">
          <span className="files-save-hint">Ctrl+S</span>
          <button className="btn btn-primary btn-sm" disabled={!dirty || saving} onClick={onSave}>
            {saving ? <Spinner size={12} /> : <IconSave size={13} />}
            Save
          </button>
        </div>
      </div>
      <div className="files-editor-shell">
        <Editor
          path={`nooki://${serverId}/${file.path}`}
          language={file.language}
          value={draft}
          theme="nooki"
          beforeMount={configureEditor}
          onMount={mountEditor}
          onChange={(value) => onDraftChange(value ?? '')}
          loading={<div className="files-loading files-editor-inline-loading"><Spinner size={18} /><span>Preparing editor</span></div>}
          options={{
            automaticLayout: true,
            fontFamily: 'Cascadia Code, Cascadia Mono, Consolas, monospace',
            fontSize: 13,
            lineHeight: 21,
            minimap: { enabled: true, renderCharacters: false, maxColumn: 90 },
            padding: { top: 14, bottom: 14 },
            renderLineHighlight: 'line',
            scrollBeyondLastLine: false,
            smoothScrolling: false,
            wordWrap: 'on',
            bracketPairColorization: { enabled: true },
            guides: { bracketPairs: true, indentation: true },
          }}
        />
      </div>
      <div className="files-editor-status"><span>{file.language}</span><span>UTF-8</span><span>{formatBytes(new Blob([draft]).size)}</span></div>
    </div>
  );
}
