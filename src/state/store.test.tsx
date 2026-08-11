import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppEvent, AppSnapshot, Server } from '../types';

const mocks = vi.hoisted(() => ({ initialize: vi.fn() }));

vi.mock('../api/tauri', () => ({
  api: {
    initialize: mocks.initialize,
    listVersions: vi.fn(), scanServerFolder: vi.fn(), createServer: vi.fn(), importServer: vi.fn(),
    removeServer: vi.fn(), revealPath: vi.fn(), serverAction: vi.fn(), saveServerSettings: vi.fn(),
    dismissAlert: vi.fn(), sendCommand: vi.fn(), playerAction: vi.fn(), createBackup: vi.fn(),
    restoreBackup: vi.fn(), deleteBackup: vi.fn(), saveSchedule: vi.fn(), changeSoftware: vi.fn(),
    saveAppSettings: vi.fn(), activateRelay: vi.fn(), quit: vi.fn(), listLogs: vi.fn(), readLog: vi.fn(), exportLog: vi.fn(),
    detectJava: vi.fn(), installJava: vi.fn(), removeJava: vi.fn(),
    cancelOperation: vi.fn(),
  },
}));

import { StoreProvider, useStore } from './store';

const snapshot: AppSnapshot = {
  servers: [], players: [], rosters: {}, backups: [], schedules: {}, activity: [], consoleLines: {},
  settings: { serverFolder: 'C:\\Servers', backupFolder: 'C:\\Backups', minimizeToTray: true, launchOnLogin: false },
  relayAccess: { activated: false, serversAllowed: 0 },
  host: { totalMemory: 16384, usedMemory: 4096, cpu: 2, diskTotal: 100000, diskUsed: 50000 },
  javaRuntimes: [], logSessions: [], appVersion: '0.1.0',
};

const server: Server = {
  id: 'server-1', name: 'Survival', type: 'vanilla', version: '1.21.8', build: 'release', status: 'stopped',
  players: 0, maxPlayers: 20, startedAt: null, memory: 0, minMemory: 1024, maxMemory: 4096, cpu: 0,
  diskUsed: 10, port: 25565, folder: 'C:\\Servers\\Survival', jarPath: 'C:\\Servers\\Survival\\server.jar',
  accent: '#5fb87f', motd: 'Survival', gameMode: 'survival', difficulty: 'normal', pvp: true,
  whitelistEnabled: false, onlineMode: true, javaRuntimeId: 'java-21', javaRuntime: 'Java 21', jvmArgs: '',
  history: [], alerts: [], updateAvailable: null, richManagement: true, ephemeral: false,
  sharing: { status: 'offline', address: null, deviceId: null, lastError: null, vanity: null },
};

function Probe() {
  const store = useStore();
  return <span>{store.ready ? `ready:${store.servers.length}` : 'loading'}</span>;
}

function ConsoleProbe() {
  const store = useStore();
  return <span>{store.consoleLines['server-1']?.map((line) => line.text).join('|') || 'empty'}</span>;
}

describe('StoreProvider', () => {
  beforeEach(() => mocks.initialize.mockReset());

  it('shows loading until the backend snapshot arrives', async () => {
    let finish!: (value: AppSnapshot) => void;
    mocks.initialize.mockReturnValue(new Promise<AppSnapshot>((resolve) => { finish = resolve; }));
    render(<StoreProvider><Probe /></StoreProvider>);
    expect(screen.getByText('loading')).toBeTruthy();
    await act(async () => finish(snapshot));
    expect(screen.getByText('ready:0')).toBeTruthy();
  });

  it('applies streamed server changes after initialization', async () => {
    let handler!: (event: AppEvent) => void;
    mocks.initialize.mockImplementation((onEvent: (event: AppEvent) => void) => { handler = onEvent; return Promise.resolve(snapshot); });
    render(<StoreProvider><Probe /></StoreProvider>);
    expect(await screen.findByText('ready:0')).toBeTruthy();
    act(() => handler({ event: 'serverChanged', data: server }));
    expect(screen.getByText('ready:1')).toBeTruthy();
  });

  it('adds streamed console output to the matching server in real time', async () => {
    let handler!: (event: AppEvent) => void;
    mocks.initialize.mockImplementation((onEvent: (event: AppEvent) => void) => { handler = onEvent; return Promise.resolve(snapshot); });
    render(<StoreProvider><ConsoleProbe /></StoreProvider>);
    expect(await screen.findByText('empty')).toBeTruthy();
    act(() => handler({
      event: 'consoleLine',
      data: { serverId: 'server-1', line: { id: 'line-1', at: 1, level: 'info', source: 'Server', text: 'Done loading' } },
    }));
    expect(screen.getByText('Done loading')).toBeTruthy();
  });

  it('ignores repeated console event IDs and clears the visible stream when a server starts', async () => {
    let handler!: (event: AppEvent) => void;
    mocks.initialize.mockImplementation((onEvent: (event: AppEvent) => void) => { handler = onEvent; return Promise.resolve(snapshot); });
    render(<StoreProvider><ConsoleProbe /></StoreProvider>);
    expect(await screen.findByText('empty')).toBeTruthy();
    const event: AppEvent = {
      event: 'consoleLine',
      data: { serverId: 'server-1', line: { id: 'line-1', at: 1, level: 'info', source: 'Server', text: 'Starting' } },
    };
    act(() => { handler(event); handler(event); });
    expect(screen.getByText('Starting')).toBeTruthy();
    expect(screen.queryByText('Starting|Starting')).toBeNull();
    act(() => handler({ event: 'consoleCleared', data: { serverId: 'server-1' } }));
    expect(screen.getByText('empty')).toBeTruthy();
  });
});
