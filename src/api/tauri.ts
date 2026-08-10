import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  AppError, AppEvent, AppSettings, AppSnapshot, Backup, BackupSchedule, ChangeSoftwareInput, RelayAccess,
  CreateDatabaseInput, DatabaseEnvironment, ManagedDatabase,
  WorldEntry, WorldSettingsInput,
  CreateServerInput, ImportScan, ImportServerInput, JavaRuntime, LogLine, LogSession, OperationEvent,
  CreateModpackServerInput, ModCatalog, ModFile, ModInstallResult, ModpackCatalog, ModpackVersionOption, ModProvider, PlayerActionInput, PluginCatalog, PluginFile, Server, ServerSettingsInput, ServerType, VersionCatalog,
} from '../types';

function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return invoke<T>(command, args).catch((error: AppError | string) => {
    if (typeof error === 'string') throw { code: 'unknown', message: error, recoverable: true } satisfies AppError;
    throw error;
  });
}

function progressChannel(handler: (event: OperationEvent) => void) {
  const channel = new Channel<OperationEvent>();
  channel.onmessage = handler;
  return channel;
}

const activeAppEventChannels = new Set<Channel<AppEvent>>();

export const api = {
  initialize(onEvent: (event: AppEvent) => void) {
    const channel = new Channel<AppEvent>();
    channel.onmessage = onEvent;
    activeAppEventChannels.clear();
    activeAppEventChannels.add(channel);
    return call<AppSnapshot>('initialize', { onEvent: channel });
  },
  listVersions(serverType: ServerType, includeExperimental: boolean) {
    return call<VersionCatalog>('list_software_versions', { serverType, includeExperimental });
  },
  scanServerFolder(path: string) { return call<ImportScan>('scan_server_folder', { path }); },
  createServer(input: CreateServerInput, onProgress: (event: OperationEvent) => void) {
    return call<Server>('create_server', { input, onProgress: progressChannel(onProgress) });
  },
  searchModpacks(provider: ModProvider, query: string, offset = 0) {
    return call<ModpackCatalog>('search_modpacks', { provider, query, offset });
  },
  listModpackVersions(provider: ModProvider, projectId: string) {
    return call<ModpackVersionOption[]>('list_modpack_versions', { provider, projectId });
  },
  createModpackServer(input: CreateModpackServerInput, onProgress: (event: OperationEvent) => void) {
    return call<Server>('create_modpack_server', { input, onProgress: progressChannel(onProgress) });
  },
  importServer(input: ImportServerInput, onProgress: (event: OperationEvent) => void) {
    return call<Server>('import_server', { input, onProgress: progressChannel(onProgress) });
  },
  serverAction(id: string, action: 'start' | 'stop' | 'restart' | 'forceStop') { return call<void>('server_action', { id, action }); },
  sendCommand(id: string, command: string) { return call<void>('send_console_command', { id, command }); },
  saveServerSettings(id: string, input: ServerSettingsInput) { return call<Server>('save_server_settings', { id, input }); },
  dismissAlert(id: string, alertId: string) { return call<Server>('dismiss_server_alert', { id, alertId }); },
  playerAction(id: string, input: PlayerActionInput) { return call<void>('player_action', { id, input }); },
  createBackup(serverId: string, notes: string | null, backupType: string, onProgress: (event: OperationEvent) => void) {
    return call<Backup>('create_backup', { serverId, notes, backupType, onProgress: progressChannel(onProgress) });
  },
  restoreBackup(backupId: string, onProgress: (event: OperationEvent) => void) {
    return call<void>('restore_backup', { backupId, onProgress: progressChannel(onProgress) });
  },
  deleteBackup(backupId: string) { return call<void>('delete_backup', { backupId }); },
  saveSchedule(serverId: string, schedule: BackupSchedule) { return call<BackupSchedule>('save_backup_schedule', { serverId, schedule }); },
  removeServer(id: string, mode: 'forget' | 'recycle', confirmation?: string) { return call<void>('remove_server', { id, mode, confirmation }); },
  detectJava() { return call<JavaRuntime[]>('detect_java_runtimes'); },
  installJava(major: number, onProgress: (event: OperationEvent) => void) {
    return call<JavaRuntime>('install_java_runtime', { major, onProgress: progressChannel(onProgress) });
  },
  removeJava(id: string) { return call<void>('remove_java_runtime', { id }); },
  listLogs(serverId: string) { return call<LogSession[]>('list_log_sessions', { serverId }); },
  readLog(sessionId: string) { return call<LogLine[]>('read_log_session', { sessionId }); },
  exportLog(sessionId: string, destination: string) { return call<void>('export_log', { sessionId, destination }); },
  saveAppSettings(settings: AppSettings) { return call<AppSettings>('save_app_settings', { settings }); },
  activateRelay(activationKey: string) { return call<RelayAccess>('activate_relay', { activationKey }); },
  revealPath(path: string) { return call<void>('reveal_path', { path }); },
  checkUpdates() { return call<Server[]>('check_server_updates'); },
  changeSoftware(id: string, input: ChangeSoftwareInput, onProgress: (event: OperationEvent) => void) {
    return call<Server>('change_server_software', { id, input, onProgress: progressChannel(onProgress) });
  },
  listPlugins(serverId: string) { return call<PluginFile[]>('list_plugins', { serverId }); },
  setPluginEnabled(serverId: string, fileName: string, enabled: boolean) {
    return call<PluginFile[]>('set_plugin_enabled', { serverId, fileName, enabled });
  },
  deletePlugin(serverId: string, fileName: string) {
    return call<PluginFile[]>('delete_plugin', { serverId, fileName });
  },
  searchPlugins(query: string, offset = 0) { return call<PluginCatalog>('search_plugins', { query, offset }); },
  loadPluginIcon(projectId: number) { return call<string | null>('load_plugin_icon', { projectId }); },
  installPlugin(serverId: string, namespace: string, slug: string, onProgress: (event: OperationEvent) => void) {
    return call<PluginFile[]>('install_plugin', { serverId, namespace, slug, onProgress: progressChannel(onProgress) });
  },
  listMods(serverId: string) { return call<ModFile[]>('list_mods', { serverId }); },
  setModEnabled(serverId: string, fileName: string, enabled: boolean) {
    return call<ModFile[]>('set_mod_enabled', { serverId, fileName, enabled });
  },
  deleteMod(serverId: string, fileName: string) {
    return call<ModFile[]>('delete_mod', { serverId, fileName });
  },
  searchMods(provider: ModProvider, loader: 'fabric' | 'forge' | 'neoforge', gameVersion: string, query: string, offset = 0) {
    return call<ModCatalog>('search_mods', { provider, loader, gameVersion, query, offset });
  },
  loadModIcon(provider: ModProvider, iconUrl: string) {
    return call<string | null>('load_mod_icon', { provider, iconUrl });
  },
  installMod(serverId: string, provider: ModProvider, projectId: string, onProgress: (event: OperationEvent) => void) {
    return call<ModInstallResult>('install_mod', { serverId, provider, projectId, onProgress: progressChannel(onProgress) });
  },
  checkManualModDownload(token: string) { return call<ModInstallResult>('check_manual_mod_download', { token }); },
  cancelManualModDownload(token: string) { return call<void>('cancel_manual_mod_download', { token }); },
  cancelOperation(operationId: string) { return call<void>('cancel_operation', { operationId }); },
  databaseEnvironment() { return call<DatabaseEnvironment>('database_environment'); },
  listDatabases(serverId: string) { return call<ManagedDatabase[]>('list_databases', { serverId }); },
  createDatabase(serverId: string, input: CreateDatabaseInput, onProgress: (event: OperationEvent) => void) {
    return call<ManagedDatabase>('create_database', { serverId, input, onProgress: progressChannel(onProgress) });
  },
  databaseAction(id: string, action: 'start' | 'stop' | 'restart') {
    return call<ManagedDatabase>('database_action', { id, action });
  },
  deleteDatabase(id: string) { return call<void>('delete_database', { id }); },
  listWorlds(serverId: string) { return call<WorldEntry[]>('list_worlds', { serverId }); },
  saveWorldSettings(serverId: string, worldId: string, input: WorldSettingsInput) {
    return call<WorldEntry[]>('save_world_settings', { serverId, worldId, input });
  },
  regenerateWorld(serverId: string, worldId: string, resetPlayers: boolean) {
    return call<WorldEntry[]>('regenerate_world', { serverId, worldId, resetPlayers });
  },
  deleteWorld(serverId: string, worldId: string, confirmation: string) {
    return call<WorldEntry[]>('delete_world', { serverId, worldId, confirmation });
  },
  openManualModDownload(token: string) { return call<void>('open_manual_mod_download', { token }); },
  quit(force = false) { return call<boolean>('quit_application', { force }); },
};
