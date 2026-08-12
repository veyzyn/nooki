export type ServerType = 'vanilla' | 'paper' | 'forge' | 'neoforge' | 'fabric';
export type ServerStatus = 'running' | 'stopped' | 'crashed' | 'starting' | 'stopping' | 'restarting' | 'updating';

export interface ResourceSample { at: number; cpu: number; memory: number; players: number }
export interface ServerAlert {
  id: string; kind: 'crash' | 'port-conflict' | 'restart-required' | 'update-available' | 'low-disk' | 'stop-timeout';
  title: string; detail: string; severity: 'warning' | 'error' | 'info';
}
export interface ActiveOperation { id: string; kind: string; phase: string; progress?: number; message: string }
export type SharingStatus = 'offline' | 'connecting' | 'online' | 'error';
export interface ServerSharing {
  status: SharingStatus; address?: string | null; deviceId?: string | null; lastError?: string | null; vanity?: string | null;
}
export interface Server {
  id: string; name: string; type: ServerType; version: string; build: string; status: ServerStatus;
  players: number; maxPlayers: number; startedAt: number | null; memory: number; minMemory: number;
  maxMemory: number; cpu: number; diskUsed: number; port: number; folder: string; jarPath: string;
  accent: string; iconData?: string | null; motd: string; gameMode: 'survival' | 'creative' | 'adventure' | 'spectator';
  difficulty: 'peaceful' | 'easy' | 'normal' | 'hard'; pvp: boolean; whitelistEnabled: boolean;
  onlineMode: boolean; javaRuntimeId: string; javaRuntime: string; jvmArgs: string; history: ResourceSample[];
  alerts: ServerAlert[]; updateAvailable: { version: string; build: string; notes: string; experimental?: boolean } | null;
  lastExit?: string | null; activeOperation?: ActiveOperation | null; richManagement: boolean;
  sharing: ServerSharing;
  ephemeral: boolean;
}

export interface Player { id: string; username: string; serverId: string; connectedAt: number; isOp: boolean; avatar: string }
export interface RosterEntry { id: string; username: string; avatar: string; addedAt: number; reason?: string }
export interface ServerRoster { whitelist: RosterEntry[]; operators: RosterEntry[]; banned: RosterEntry[] }
export type BackupType = 'manual' | 'scheduled' | 'safety' | 'pre-update';
export interface Backup {
  id: string; serverId: string; serverName: string; type: BackupType; createdAt: number; size: number;
  version: string; notes?: string | null; failed?: boolean | null; path: string; checksum?: string | null;
  errorMessage?: string | null;
}
export interface BackupSchedule {
  enabled: boolean; frequency: 'hourly' | 'daily' | 'weekly'; time: string; keep: number;
  weekday?: number | null; lastRunAt?: number | null; nextRunAt?: number | null;
}
export type ActivityKind = 'backup' | 'restart' | 'crash' | 'update' | 'start' | 'stop' | 'restore' | 'settings' | 'sharing';
export interface ActivityEvent { id: string; kind: ActivityKind; serverId?: string | null; serverName?: string | null; at: number; message: string }
export type LogLevel = 'info' | 'warn' | 'error';
export interface LogLine { id: string; at: number; level: LogLevel; source: string; text: string }
export interface LogSession {
  id: string; serverId: string; startedAt: number; duration: number; size: number;
  outcome: 'clean-stop' | 'crashed' | 'running'; path: string; lines: LogLine[];
}
export interface JavaRuntime {
  id: string; label: string; version: string; major: number; path: string; bundled: boolean; usedBy: number; architecture: string;
}
export interface AppSettings {
  serverFolder: string; backupFolder: string; minimizeToTray: boolean; launchOnLogin: boolean;
}
export interface RelayAccess {
  activated: boolean; activationId?: string | null; deviceId?: string | null; serversAllowed: number;
}
export interface HostInfo { totalMemory: number; usedMemory: number; cpu: number; diskTotal: number; diskUsed: number }
export interface Toast { id: string; tone: 'success' | 'error' | 'warning' | 'info' | 'progress'; title: string; detail?: string; progress?: number; sticky?: boolean }
export type NavView = 'dashboard' | 'servers' | 'backups' | 'quick-server' | 'settings';
export type ServerTab = 'overview' | 'console' | 'players' | 'plugins' | 'mods' | 'worlds' | 'databases' | 'settings' | 'logs' | 'backups';

export type DatabaseKind = 'mysql' | 'postgresql' | 'mongodb' | 'redis';
export type DatabaseStatus = 'running' | 'stopped' | 'creating' | 'error' | 'missing';
export interface ManagedDatabase {
  id: string; serverId: string; kind: DatabaseKind; name: string; status: DatabaseStatus;
  host: string; port: number; username: string; password: string; database: string;
  connectionUri: string; containerName: string; volumeName: string; createdAt: number;
  lastError?: string | null;
}
export interface DatabaseEnvironment {
  available: boolean; version?: string | null; message?: string | null; code?: string | null;
  cliPath?: string | null; context?: string | null; details: string[]; suggestions: string[];
}
export interface CreateDatabaseInput { kind: DatabaseKind; name: string }

export type WorldKind = 'overworld' | 'nether' | 'end' | 'custom';
export interface WorldEntry {
  id: string; name: string; folderName: string; kind: WorldKind; path: string; generated: boolean;
  primary: boolean; custom: boolean; seed?: string | null; version?: string | null; dataVersion?: number | null;
  size: number; regionFiles: number; playerFiles: number; lastPlayed?: number | null;
  spawnX?: number | null; spawnY?: number | null; spawnZ?: number | null; borderSize?: number | null;
  dayTime?: number | null; weather: string; gameMode?: string | null; difficulty?: string | null;
  hardcore: boolean; allowCommands: boolean; metadataError?: string | null;
}
export interface WorldSettingsInput {
  seed: string; spawnX: number; spawnY: number; spawnZ: number; borderSize: number;
  dayTime: number; weather: 'clear' | 'rain' | 'thunder';
}

export interface PluginFile {
  fileName: string; name: string; version?: string | null; description?: string | null;
  authors: string[]; enabled: boolean; size: number; modifiedAt: number; hangar?: HangarPluginMetadata | null;
}
export interface HangarPluginMetadata {
  serverId: string; fileName: string; projectId: number; namespace: string; slug: string;
  name: string; description: string; author: string; version: string;
}
export interface PluginProject {
  projectId: number; namespace: string; slug: string; name: string; description: string; author: string;
  downloads: number; stars: number; lastUpdated: number;
}
export interface PluginCatalog { projects: PluginProject[]; total: number; offset: number; hasMore: boolean }

export type ModProvider = 'modrinth' | 'curseforge';
export interface ModMetadata {
  serverId: string; fileName: string; provider: ModProvider; projectId: string; slug: string;
  name: string; description: string; author: string; version: string; iconUrl?: string | null; websiteUrl: string;
}
export interface ModFile {
  fileName: string; name: string; version?: string | null; description?: string | null;
  authors: string[]; enabled: boolean; size: number; modifiedAt: number; metadata?: ModMetadata | null;
}
export interface ModProject {
  provider: ModProvider; projectId: string; slug: string; name: string; description: string; author: string;
  downloads: number; followers: number; lastUpdated: number; iconUrl?: string | null; websiteUrl: string;
}
export interface ModCatalog { projects: ModProject[]; total: number; offset: number; hasMore: boolean }

export interface ModpackProject {
  provider: ModProvider; projectId: string; slug: string; name: string; description: string;
  author: string; downloads: number; iconUrl?: string | null; websiteUrl: string;
}
export interface ModpackCatalog { projects: ModpackProject[]; total: number; offset: number; hasMore: boolean }
export interface ModpackVersionOption {
  id: string; name: string; versionNumber: string; minecraftVersion: string; loader: 'fabric' | 'forge' | 'neoforge';
  releaseType: 'release' | 'beta' | 'alpha'; publishedAt: number; size: number; automatic: boolean;
}
export interface CreateModpackServerInput {
  provider: ModProvider; projectId: string; versionId: string; name: string; minMemory: number;
  maxMemory: number; port: number; parentFolder: string; eula: boolean; javaRuntimeId?: string | null; iconUrl?: string | null;
}
export interface ManualModDownload {
  token: string; projectName: string; fileName: string; downloadUrl: string; downloadsFolder: string;
}
export interface ModInstallResult { mods: ModFile[]; manualDownload?: ManualModDownload | null }

export interface AppSnapshot {
  servers: Server[]; ephemeralServer?: Server | null; players: Player[]; rosters: Record<string, ServerRoster>; backups: Backup[];
  schedules: Record<string, BackupSchedule>; activity: ActivityEvent[]; consoleLines: Record<string, LogLine[]>;
  settings: AppSettings; relayAccess: RelayAccess; host: HostInfo; javaRuntimes: JavaRuntime[]; logSessions: LogSession[]; appVersion: string;
}
export interface VersionOption {
  id: string; version: string; build: string; releaseType: string; experimental: boolean;
  javaMajor?: number | null; publishedAt?: string | null;
}
export interface VersionCatalog { serverType: ServerType; versions: VersionOption[]; fetchedAt: number }
export interface JarCandidate { path: string; fileName: string; serverType?: ServerType | null; version?: string | null; build?: string | null }
export interface ImportScan {
  folder: string; valid: boolean; detectedName: string; detectedType?: ServerType | null;
  detectedVersion?: string | null; port?: number | null; eulaAccepted: boolean; candidates: JarCandidate[]; warnings: string[];
}
export interface EphemeralWorldScan {
  sourcePath: string; sourceKind: 'folder' | 'zip'; worldName: string;
  detectedVersion?: string | null; warnings: string[];
}
export interface CreateEphemeralServerInput { sourcePath: string; version: string }
export interface OperationEventData { operationId: string; phase?: string; progress?: number; message: string }
export type OperationEvent = { event: 'started' | 'progress' | 'finished'; data: OperationEventData };
export type AppEvent =
  | { event: 'serverChanged'; data: Server }
  | { event: 'serverMetrics'; data: { serverId: string; cpu: number; memory: number; diskUsed: number; sample: ResourceSample } }
  | { event: 'serverRemoved'; data: { serverId: string } }
  | { event: 'consoleLine'; data: { serverId: string; line: LogLine } }
  | { event: 'consoleCleared'; data: { serverId: string } }
  | { event: 'playersChanged'; data: { serverId: string; players: Player[] } }
  | { event: 'rostersChanged'; data: { serverId: string; roster: ServerRoster } }
  | { event: 'backupChanged'; data: Backup }
  | { event: 'backupRemoved'; data: { backupId: string } }
  | { event: 'scheduleChanged'; data: { serverId: string; schedule: BackupSchedule } }
  | { event: 'activityAdded'; data: ActivityEvent }
  | { event: 'hostMetrics'; data: HostInfo }
  | { event: 'runtimesChanged'; data: JavaRuntime[] }
  | { event: 'quitRequested'; data: { runningServers: number } };

export interface AppError { code: string; message: string; detail?: string | null; field?: string | null; recoverable: boolean }
export interface CreateServerInput {
  name: string; type: ServerType; version: string; build?: string | null; minMemory: number; maxMemory: number;
  port: number; parentFolder: string; eula: boolean; javaRuntimeId?: string | null; iconData?: string | null; experimental?: boolean;
}
export interface ImportServerInput {
  name: string; folder: string; jarPath: string; type: ServerType; version: string; build: string;
  minMemory: number; maxMemory: number; port: number; eula: boolean; javaRuntimeId?: string | null; iconData?: string | null;
}
export interface ServerSettingsInput {
  name: string; motd: string; gameMode: Server['gameMode']; difficulty: Server['difficulty']; maxPlayers: number;
  pvp: boolean; whitelistEnabled: boolean; onlineMode: boolean; port: number; minMemory: number; maxMemory: number;
  javaRuntimeId: string; jvmArgs: string; vanity?: string | null;
}
export interface PlayerActionInput { action: 'kick' | 'ban' | 'unban' | 'whitelistAdd' | 'whitelistRemove' | 'op' | 'deop'; username: string; reason?: string | null }
export interface ChangeSoftwareInput { version: string; build?: string | null; experimental: boolean; confirmation?: string | null }
