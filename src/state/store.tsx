import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode,
} from 'react';
import { api } from '../api/tauri';
import type {
  ActivityEvent, AppError, AppEvent, AppSettings, Backup, BackupSchedule, ChangeSoftwareInput, CreateEphemeralServerInput, CreateModpackServerInput, CreateServerInput,
  CreateDatabaseInput, DatabaseEnvironment, ManagedDatabase,
  WorldEntry, WorldSettingsInput,
  EphemeralWorldScan, HostInfo, ImportScan, ImportServerInput, JavaRuntime, LogLine, LogSession, NavView, OperationEvent,
  ModCatalog, ModFile, ModInstallResult, ModpackCatalog, ModpackVersionOption, ModProvider, Player, PluginCatalog, PluginFile, RelayAccess, Server, ServerRoster, ServerSettingsInput, ServerTab, ServerType, Toast, VersionCatalog,
} from '../types';
import { uid } from '../format';

export interface UpdateFlow {
  serverId: string;
  phase: 'confirm' | 'backup' | 'download' | 'validate' | 'restart' | 'done' | 'failed' | 'cancelled' | 'rolled-back';
  progress: number;
  message: string;
  operationId?: string;
}
export interface BackupFlow { serverId: string; progress: number; phase: 'running' | 'done' | 'failed' | 'cancelled'; message: string; operationId?: string }
export interface RestoreFlow { backupId: string; serverId: string; progress: number; phase: 'safety' | 'restoring' | 'done' | 'failed' | 'cancelled'; message: string; operationId?: string }

interface StoreValue {
  ready: boolean; initError: AppError | null; retryInitialize: () => void;
  nav: NavView; setNav: (view: NavView) => void; openServerId: string | null;
  openServer: (id: string, tab?: ServerTab) => void; closeServer: () => void;
  serverTab: ServerTab; setServerTab: (tab: ServerTab) => void;
  servers: Server[]; players: Player[]; rosters: Record<string, ServerRoster>; backups: Backup[];
  ephemeralServer: Server | null;
  schedules: Record<string, BackupSchedule>; activity: ActivityEvent[]; consoleLines: Record<string, LogLine[]>;
  settings: AppSettings; relayAccess: RelayAccess; activateRelay: (activationKey: string) => Promise<void>; host: HostInfo; javaRuntimes: JavaRuntime[]; logSessions: LogSession[]; appVersion: string;
  refreshLogs: (serverId: string) => Promise<void>; readLog: (sessionId: string) => Promise<LogLine[]>; exportLog: (sessionId: string, destination: string) => Promise<void>;
  detectJava: () => Promise<void>; installJava: (major: number, onProgress: (event: OperationEvent) => void) => Promise<JavaRuntime>; removeJava: (id: string) => Promise<void>;
  toasts: Toast[]; dismissToast: (id: string) => void;
  startServer: (id: string) => void; stopServer: (id: string) => void; restartServer: (id: string) => void; forceStopServer: (id: string) => void;
  createServer: (input: CreateServerInput, onProgress: (event: OperationEvent) => void) => Promise<Server>;
  searchModpacks: (provider: ModProvider, query: string, offset?: number) => Promise<ModpackCatalog>;
  listModpackVersions: (provider: ModProvider, projectId: string) => Promise<ModpackVersionOption[]>;
  createModpackServer: (input: CreateModpackServerInput, onProgress: (event: OperationEvent) => void) => Promise<Server>;
  importServer: (input: ImportServerInput, onProgress: (event: OperationEvent) => void) => Promise<Server>;
  listVersions: (type: ServerType, includeExperimental: boolean) => Promise<VersionCatalog>;
  scanServerFolder: (path: string) => Promise<ImportScan>;
  scanEphemeralWorld: (path: string) => Promise<EphemeralWorldScan>;
  createEphemeralServer: (input: CreateEphemeralServerInput, onProgress: (event: OperationEvent) => void) => Promise<Server>;
  removeServer: (id: string, mode: 'forget' | 'recycle', confirmation?: string) => Promise<void>;
  revealPath: (path: string) => void;
  patchServer: (id: string, patch: Partial<Server>) => void; dismissAlert: (serverId: string, alertId: string) => void;
  sendCommand: (serverId: string, command: string) => void; clearConsole: (serverId: string) => void;
  kickPlayer: (serverId: string, playerId: string) => void; banPlayer: (serverId: string, playerId: string, reason: string) => void;
  unbanPlayer: (serverId: string, entryId: string) => void; addToWhitelist: (serverId: string, username: string) => void;
  removeFromWhitelist: (serverId: string, entryId: string) => void; grantOperator: (serverId: string, username: string) => void;
  revokeOperator: (serverId: string, entryId: string) => void;
  backupFlow: BackupFlow | null; startBackup: (serverId: string, notes: string) => void; clearBackupFlow: () => void;
  restoreFlow: RestoreFlow | null; startRestore: (backupId: string) => void; clearRestoreFlow: () => void;
  deleteBackup: (backupId: string) => void; setSchedule: (serverId: string, schedule: BackupSchedule) => void;
  updateFlow: UpdateFlow | null; beginUpdate: (serverId: string) => void; runUpdate: (serverId: string) => void; clearUpdateFlow: () => void;
  changeSoftware: (id: string, input: ChangeSoftwareInput, onProgress: (event: OperationEvent) => void) => Promise<Server>;
  listPlugins: (serverId: string) => Promise<PluginFile[]>;
  setPluginEnabled: (serverId: string, fileName: string, enabled: boolean) => Promise<PluginFile[]>;
  deletePlugin: (serverId: string, fileName: string) => Promise<PluginFile[]>;
  searchPlugins: (query: string, offset?: number) => Promise<PluginCatalog>;
  loadPluginIcon: (projectId: number) => Promise<string | null>;
  installPlugin: (serverId: string, namespace: string, slug: string, onProgress: (event: OperationEvent) => void) => Promise<PluginFile[]>;
  listMods: (serverId: string) => Promise<ModFile[]>;
  setModEnabled: (serverId: string, fileName: string, enabled: boolean) => Promise<ModFile[]>;
  deleteMod: (serverId: string, fileName: string) => Promise<ModFile[]>;
  searchMods: (provider: ModProvider, loader: 'fabric' | 'forge' | 'neoforge', gameVersion: string, query: string, offset?: number) => Promise<ModCatalog>;
  loadModIcon: (provider: ModProvider, iconUrl: string) => Promise<string | null>;
  installMod: (serverId: string, provider: ModProvider, projectId: string, onProgress: (event: OperationEvent) => void) => Promise<ModInstallResult>;
  checkManualModDownload: (token: string) => Promise<ModInstallResult>;
  cancelManualModDownload: (token: string) => Promise<void>;
  openManualModDownload: (token: string) => Promise<void>;
  cancelOperation: (operationId: string) => Promise<void>;
  databaseEnvironment: () => Promise<DatabaseEnvironment>;
  listDatabases: (serverId: string) => Promise<ManagedDatabase[]>;
  createDatabase: (serverId: string, input: CreateDatabaseInput, onProgress: (event: OperationEvent) => void) => Promise<ManagedDatabase>;
  databaseAction: (id: string, action: 'start' | 'stop' | 'restart') => Promise<ManagedDatabase>;
  deleteDatabase: (id: string) => Promise<void>;
  listWorlds: (serverId: string) => Promise<WorldEntry[]>;
  saveWorldSettings: (serverId: string, worldId: string, input: WorldSettingsInput) => Promise<WorldEntry[]>;
  regenerateWorld: (serverId: string, worldId: string, resetPlayers: boolean) => Promise<WorldEntry[]>;
  deleteWorld: (serverId: string, worldId: string, confirmation: string) => Promise<WorldEntry[]>;
  patchSettings: (patch: Partial<AppSettings>) => void; restartRequired: boolean; setRestartRequired: (value: boolean) => void;
  quitDialog: 'closed' | 'tray' | 'quit'; setQuitDialog: (value: 'closed' | 'tray' | 'quit') => void; quit: (force?: boolean) => Promise<boolean>;
  wizardOpen: boolean; setWizardOpen: (open: boolean) => void;
  pushToast: (toast: Omit<Toast, 'id'>) => string; updateToast: (id: string, patch: Partial<Toast>) => void;
  logActivity: (event: Omit<ActivityEvent, 'id' | 'at'>) => void;
}

const emptySettings: AppSettings = {
  serverFolder: '', backupFolder: '', minimizeToTray: true, launchOnLogin: false,
};
const emptyHost: HostInfo = { totalMemory: 0, usedMemory: 0, cpu: 0, diskTotal: 0, diskUsed: 0 };
const emptyRelayAccess: RelayAccess = { activated: false, serversAllowed: 0 };
const StoreContext = createContext<StoreValue | null>(null);

function errorMessage(error: unknown) {
  if (typeof error === 'object' && error && 'message' in error) return String((error as { message: unknown }).message);
  return String(error);
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [initError, setInitError] = useState<AppError | null>(null);
  const [initAttempt, setInitAttempt] = useState(0);
  const [nav, setNavState] = useState<NavView>('dashboard');
  const [openServerId, setOpenServerId] = useState<string | null>(null);
  const [serverTab, setServerTab] = useState<ServerTab>('overview');
  const [servers, setServers] = useState<Server[]>([]);
  const [ephemeralServer, setEphemeralServer] = useState<Server | null>(null);
  const [players, setPlayers] = useState<Player[]>([]);
  const [rosters, setRosters] = useState<Record<string, ServerRoster>>({});
  const [backups, setBackups] = useState<Backup[]>([]);
  const [schedules, setSchedules] = useState<Record<string, BackupSchedule>>({});
  const [activity, setActivity] = useState<ActivityEvent[]>([]);
  const [consoleLines, setConsoleLines] = useState<Record<string, LogLine[]>>({});
  const [settings, setSettings] = useState<AppSettings>(emptySettings);
  const [relayAccess, setRelayAccess] = useState<RelayAccess>(emptyRelayAccess);
  const [host, setHost] = useState<HostInfo>(emptyHost);
  const [javaRuntimes, setJavaRuntimes] = useState<JavaRuntime[]>([]);
  const [logSessions, setLogSessions] = useState<LogSession[]>([]);
  const [appVersion, setAppVersion] = useState('0.1.0');
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [backupFlow, setBackupFlow] = useState<BackupFlow | null>(null);
  const [restoreFlow, setRestoreFlow] = useState<RestoreFlow | null>(null);
  const [updateFlow, setUpdateFlow] = useState<UpdateFlow | null>(null);
  const [restartRequired, setRestartRequired] = useState(false);
  const [quitDialog, setQuitDialog] = useState<'closed' | 'tray' | 'quit'>('closed');
  const [wizardOpen, setWizardOpen] = useState(false);
  const timers = useRef<number[]>([]);

  const pushToast = useCallback((toast: Omit<Toast, 'id'>) => {
    const id = uid('toast');
    setToasts((current) => [...current, { ...toast, id }]);
    if (!toast.sticky) timers.current.push(window.setTimeout(() => setToasts((current) => current.filter((item) => item.id !== id)), 5200));
    return id;
  }, []);
  const updateToast = useCallback((id: string, patch: Partial<Toast>) => setToasts((current) => current.map((toast) => toast.id === id ? { ...toast, ...patch } : toast)), []);
  const dismissToast = useCallback((id: string) => setToasts((current) => current.filter((toast) => toast.id !== id)), []);
  const showError = useCallback((title: string, error: unknown) => pushToast({ tone: 'error', title, detail: errorMessage(error) }), [pushToast]);

  const onEvent = useCallback((message: AppEvent) => {
    switch (message.event) {
      case 'serverChanged':
        if (message.data.ephemeral) {
          setEphemeralServer(message.data);
          break;
        }
        setServers((current) => current.some((server) => server.id === message.data.id)
          ? current.map((server) => server.id === message.data.id ? message.data : server)
          : [...current, message.data]);
        break;
      case 'serverRemoved':
        setServers((current) => current.filter((server) => server.id !== message.data.serverId));
        setEphemeralServer((current) => current?.id === message.data.serverId ? null : current);
        setOpenServerId((current) => current === message.data.serverId ? null : current);
        break;
      case 'consoleLine':
        setConsoleLines((current) => {
          const lines = current[message.data.serverId] ?? [];
          if (lines.some((line) => line.id === message.data.line.id)) return current;
          return { ...current, [message.data.serverId]: [...lines, message.data.line].slice(-2000) };
        });
        break;
      case 'consoleCleared':
        setConsoleLines((current) => ({ ...current, [message.data.serverId]: [] }));
        break;
      case 'playersChanged':
        setPlayers((current) => [...current.filter((player) => player.serverId !== message.data.serverId), ...message.data.players]);
        break;
      case 'rostersChanged': setRosters((current) => ({ ...current, [message.data.serverId]: message.data.roster })); break;
      case 'backupChanged':
        setBackups((current) => [message.data, ...current.filter((backup) => backup.id !== message.data.id)].sort((a, b) => b.createdAt - a.createdAt));
        break;
      case 'backupRemoved': setBackups((current) => current.filter((backup) => backup.id !== message.data.backupId)); break;
      case 'scheduleChanged': setSchedules((current) => ({ ...current, [message.data.serverId]: message.data.schedule })); break;
      case 'activityAdded':
        setActivity((current) => [message.data, ...current].slice(0, 500));
        break;
      case 'hostMetrics': setHost(message.data); break;
      case 'runtimesChanged': setJavaRuntimes(message.data); break;
      case 'quitRequested': setQuitDialog('quit'); break;
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    setReady(false); setInitError(null);
    api.initialize(onEvent).then((snapshot) => {
      if (cancelled) return;
      setServers(snapshot.servers); setEphemeralServer(snapshot.ephemeralServer ?? null); setPlayers(snapshot.players); setRosters(snapshot.rosters);
      setBackups(snapshot.backups); setSchedules(snapshot.schedules); setActivity(snapshot.activity);
      setConsoleLines((current) => {
        const merged = { ...snapshot.consoleLines };
        for (const [serverId, streamed] of Object.entries(current)) {
          const existing = merged[serverId] ?? [];
          const ids = new Set(existing.map((line) => line.id));
          merged[serverId] = [...existing, ...streamed.filter((line) => !ids.has(line.id))].slice(-2000);
        }
        return merged;
      }); setSettings(snapshot.settings); setRelayAccess(snapshot.relayAccess); setHost(snapshot.host);
      setJavaRuntimes(snapshot.javaRuntimes); setLogSessions(snapshot.logSessions); setAppVersion(snapshot.appVersion);
      setReady(true);
    }).catch((error: AppError) => { if (!cancelled) setInitError(error); });
    return () => { cancelled = true; timers.current.forEach(window.clearTimeout); };
  }, [initAttempt, onEvent]);

  const setNav = useCallback((view: NavView) => { setNavState(view); if (view !== 'servers') setOpenServerId(null); }, []);
  const openServer = useCallback((id: string, tab: ServerTab = 'overview') => { setNavState('servers'); setOpenServerId(id); setServerTab(tab); }, []);
  const closeServer = useCallback(() => setOpenServerId(null), []);
  const action = useCallback((id: string, name: 'start' | 'stop' | 'restart' | 'forceStop') => {
    void api.serverAction(id, name).catch((error) => showError(`Could not ${name === 'forceStop' ? 'force stop' : name} server`, error));
  }, [showError]);

  const createServer = useCallback((input: CreateServerInput, progress: (event: OperationEvent) => void) => api.createServer(input, progress), []);
  const createEphemeralServer = useCallback(async (input: CreateEphemeralServerInput, progress: (event: OperationEvent) => void) => {
    const server = await api.createEphemeralServer(input, progress);
    setEphemeralServer(server);
    return server;
  }, []);
  const importServer = useCallback((input: ImportServerInput, progress: (event: OperationEvent) => void) => api.importServer(input, progress), []);
  const removeServer = useCallback(async (id: string, mode: 'forget' | 'recycle', confirmation?: string) => {
    await api.removeServer(id, mode, confirmation);
    pushToast({ tone: 'success', title: mode === 'recycle' ? 'Server moved to the Recycle Bin' : 'Server removed from Nooki' });
  }, [pushToast]);
  const revealPath = useCallback((path: string) => { void api.revealPath(path).catch((error) => showError('Could not open that location', error)); }, [showError]);

  const patchServer = useCallback((id: string, patch: Partial<Server>) => {
    const server = servers.find((item) => item.id === id); if (!server) return;
    const changed = { ...server, ...patch };
    const input: ServerSettingsInput = {
      name: changed.name, motd: changed.motd, gameMode: changed.gameMode, difficulty: changed.difficulty,
      maxPlayers: changed.maxPlayers, pvp: changed.pvp, whitelistEnabled: changed.whitelistEnabled,
      onlineMode: changed.onlineMode, port: changed.port, minMemory: changed.minMemory,
      maxMemory: changed.maxMemory, javaRuntimeId: changed.javaRuntimeId, jvmArgs: changed.jvmArgs,
      vanity: changed.sharing.vanity,
    };
    void api.saveServerSettings(id, input).then((saved) => setServers((current) => current.map((item) => item.id === id ? saved : item)))
      .catch((error) => showError('Settings were not saved', error));
  }, [servers, showError]);
  const dismissAlert = useCallback((serverId: string, alertId: string) => {
    void api.dismissAlert(serverId, alertId).then((saved) => setServers((current) => current.map((server) => server.id === serverId ? saved : server)))
      .catch((error) => showError('Notice could not be dismissed', error));
  }, [showError]);
  const sendCommand = useCallback((serverId: string, command: string) => { void api.sendCommand(serverId, command).catch((error) => showError('Command was not sent', error)); }, [showError]);
  const clearConsole = useCallback((serverId: string) => setConsoleLines((current) => ({ ...current, [serverId]: [] })), []);

  const playerAction = useCallback((serverId: string, actionName: Parameters<typeof api.playerAction>[1]['action'], username: string, reason?: string) => {
    void api.playerAction(serverId, { action: actionName, username, reason }).catch((error) => showError('Player action failed', error));
  }, [showError]);
  const playerName = useCallback((serverId: string, playerId: string) => players.find((player) => player.serverId === serverId && player.id === playerId)?.username, [players]);
  const rosterName = useCallback((serverId: string, list: keyof ServerRoster, id: string) => rosters[serverId]?.[list].find((entry) => entry.id === id)?.username, [rosters]);

  const startBackup = useCallback((serverId: string, notes: string) => {
    setBackupFlow({ serverId, progress: 0, phase: 'running', message: 'Preparing server data' });
    void api.createBackup(serverId, notes || null, 'manual', (event) => {
      if (event.event === 'progress') setBackupFlow({ serverId, progress: event.data.progress ?? 0, phase: 'running', message: event.data.message, operationId: event.data.operationId });
    }).then(() => setBackupFlow({ serverId, progress: 100, phase: 'done', message: 'Backup finished' }))
      .catch((error) => setBackupFlow({ serverId, progress: 0, phase: (error as { code?: string })?.code === 'cancelled' ? 'cancelled' : 'failed', message: (error as { code?: string })?.code === 'cancelled' ? 'Backup cancelled. No partial archive was kept.' : errorMessage(error) }));
  }, []);
  const startRestore = useCallback((backupId: string) => {
    const backup = backups.find((item) => item.id === backupId); if (!backup) return;
    setRestoreFlow({ backupId, serverId: backup.serverId, progress: 0, phase: 'safety', message: 'Creating a safety backup' });
    void api.restoreBackup(backupId, (event) => {
      if (event.event === 'progress') setRestoreFlow({ backupId, serverId: backup.serverId, progress: event.data.progress ?? 0, phase: event.data.phase === 'safety' ? 'safety' : 'restoring', message: event.data.message, operationId: event.data.operationId });
    }).then(() => setRestoreFlow({ backupId, serverId: backup.serverId, progress: 100, phase: 'done', message: 'Restore finished' }))
      .catch((error) => setRestoreFlow({ backupId, serverId: backup.serverId, progress: 0, phase: (error as { code?: string })?.code === 'cancelled' ? 'cancelled' : 'failed', message: (error as { code?: string })?.code === 'cancelled' ? 'Restore cancelled. Existing server data was kept.' : errorMessage(error) }));
  }, [backups]);
  const deleteBackup = useCallback((backupId: string) => { void api.deleteBackup(backupId).catch((error) => showError('Backup was not deleted', error)); }, [showError]);
  const setSchedule = useCallback((serverId: string, schedule: BackupSchedule) => {
    setSchedules((current) => ({ ...current, [serverId]: schedule }));
    void api.saveSchedule(serverId, schedule).catch((error) => showError('Schedule was not saved', error));
  }, [showError]);

  const beginUpdate = useCallback((serverId: string) => setUpdateFlow({ serverId, phase: 'confirm', progress: 0, message: 'Ready to update' }), []);
  const runUpdate = useCallback((serverId: string) => {
    const server = servers.find((item) => item.id === serverId); if (!server?.updateAvailable) return;
    setUpdateFlow({ serverId, phase: 'backup', progress: 2, message: 'Creating a pre-update backup' });
    void api.changeSoftware(serverId, { version: server.updateAvailable.version, build: server.updateAvailable.build, experimental: false }, (event) => {
      if (event.event !== 'progress') return;
      const phase = event.data.phase === 'restart' ? 'restart' : event.data.phase === 'download' ? 'download' : event.data.phase === 'resolve' ? 'validate' : 'backup';
      setUpdateFlow({ serverId, phase, progress: event.data.progress ?? 0, message: event.data.message, operationId: event.data.operationId });
    }).then(() => setUpdateFlow({ serverId, phase: 'done', progress: 100, message: 'Update finished' }))
      .catch((error) => setUpdateFlow({ serverId, phase: (error as { code?: string })?.code === 'cancelled' ? 'cancelled' : 'failed', progress: 0, message: (error as { code?: string })?.code === 'cancelled' ? 'The update was cancelled. Existing server files were kept.' : errorMessage(error) }));
  }, [servers]);

  const patchSettings = useCallback((patch: Partial<AppSettings>) => {
    const next = { ...settings, ...patch }; setSettings(next);
    void api.saveAppSettings(next).catch((error) => { setSettings(settings); showError('Application settings were not saved', error); });
  }, [settings, showError]);
  const activateRelay = useCallback(async (activationKey: string) => {
    const access = await api.activateRelay(activationKey);
    setRelayAccess(access);
  }, []);
  const logActivity = useCallback((_event: Omit<ActivityEvent, 'id' | 'at'>) => {}, []);
  const refreshLogs = useCallback(async (serverId: string) => {
    const sessions = await api.listLogs(serverId);
    setLogSessions((current) => [...current.filter((session) => session.serverId !== serverId), ...sessions]);
  }, []);
  const detectJava = useCallback(async () => { setJavaRuntimes(await api.detectJava()); }, []);
  const installJava = useCallback(async (major: number, progress: (event: OperationEvent) => void) => {
    const runtime = await api.installJava(major, progress);
    setJavaRuntimes((current) => [...current.filter((item) => item.id !== runtime.id), runtime]);
    return runtime;
  }, []);
  const removeJava = useCallback(async (id: string) => { await api.removeJava(id); setJavaRuntimes((current) => current.filter((runtime) => runtime.id !== id)); }, []);

  const value = useMemo<StoreValue>(() => ({
    ready, initError, retryInitialize: () => setInitAttempt((value) => value + 1),
    nav, setNav, openServerId, openServer, closeServer, serverTab, setServerTab,
    servers, ephemeralServer, players, rosters, backups, schedules, activity, consoleLines, settings, relayAccess, activateRelay, host, javaRuntimes, logSessions, appVersion,
    refreshLogs, readLog: api.readLog, exportLog: api.exportLog,
    detectJava, installJava, removeJava,
    toasts, dismissToast,
    startServer: (id) => action(id, 'start'), stopServer: (id) => action(id, 'stop'), restartServer: (id) => action(id, 'restart'), forceStopServer: (id) => action(id, 'forceStop'),
    createServer, searchModpacks: api.searchModpacks, listModpackVersions: api.listModpackVersions,
    createModpackServer: api.createModpackServer, importServer, listVersions: api.listVersions, scanServerFolder: api.scanServerFolder,
    scanEphemeralWorld: api.scanEphemeralWorld, createEphemeralServer,
    removeServer, revealPath, patchServer, dismissAlert, sendCommand, clearConsole,
    kickPlayer: (serverId, playerId) => { const name = playerName(serverId, playerId); if (name) playerAction(serverId, 'kick', name); },
    banPlayer: (serverId, playerId, reason) => { const name = playerName(serverId, playerId); if (name) playerAction(serverId, 'ban', name, reason); },
    unbanPlayer: (serverId, entryId) => { const name = rosterName(serverId, 'banned', entryId); if (name) playerAction(serverId, 'unban', name); },
    addToWhitelist: (serverId, username) => playerAction(serverId, 'whitelistAdd', username),
    removeFromWhitelist: (serverId, entryId) => { const name = rosterName(serverId, 'whitelist', entryId); if (name) playerAction(serverId, 'whitelistRemove', name); },
    grantOperator: (serverId, username) => playerAction(serverId, 'op', username),
    revokeOperator: (serverId, entryId) => { const name = rosterName(serverId, 'operators', entryId); if (name) playerAction(serverId, 'deop', name); },
    backupFlow, startBackup, clearBackupFlow: () => setBackupFlow(null), restoreFlow, startRestore, clearRestoreFlow: () => setRestoreFlow(null),
    deleteBackup, setSchedule, updateFlow, beginUpdate, runUpdate, clearUpdateFlow: () => setUpdateFlow(null), changeSoftware: api.changeSoftware,
    listPlugins: api.listPlugins, setPluginEnabled: api.setPluginEnabled, deletePlugin: api.deletePlugin,
    searchPlugins: api.searchPlugins, loadPluginIcon: api.loadPluginIcon, installPlugin: api.installPlugin,
    listMods: api.listMods, setModEnabled: api.setModEnabled, deleteMod: api.deleteMod,
    searchMods: api.searchMods, loadModIcon: api.loadModIcon, installMod: api.installMod,
    checkManualModDownload: api.checkManualModDownload, cancelManualModDownload: api.cancelManualModDownload,
    openManualModDownload: api.openManualModDownload,
    cancelOperation: api.cancelOperation,
    databaseEnvironment: api.databaseEnvironment,
    listDatabases: api.listDatabases,
    createDatabase: api.createDatabase,
    databaseAction: api.databaseAction,
    deleteDatabase: api.deleteDatabase,
    listWorlds: api.listWorlds,
    saveWorldSettings: api.saveWorldSettings,
    regenerateWorld: api.regenerateWorld,
    deleteWorld: api.deleteWorld,
    patchSettings, restartRequired, setRestartRequired, quitDialog, setQuitDialog,
    quit: (force = false) => api.quit(force), wizardOpen, setWizardOpen, pushToast, updateToast, logActivity,
  }), [
    ready, initError, nav, setNav, openServerId, openServer, closeServer, serverTab, servers, ephemeralServer, players, rosters, backups,
    schedules, activity, consoleLines, settings, host, javaRuntimes, logSessions, appVersion, toasts, dismissToast,
    action, createServer, createEphemeralServer, importServer, removeServer,
    revealPath, patchServer, dismissAlert, sendCommand, clearConsole, playerName, playerAction, rosterName, backupFlow,
    startBackup, restoreFlow, startRestore, deleteBackup, setSchedule, updateFlow, beginUpdate, runUpdate, patchSettings,
    restartRequired, quitDialog, wizardOpen, pushToast, updateToast, logActivity, refreshLogs, detectJava, installJava, removeJava, relayAccess, activateRelay,
  ]);
  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}

export function useStore(): StoreValue {
  const store = useContext(StoreContext);
  if (!store) throw new Error('useStore must be used inside StoreProvider');
  return store;
}
