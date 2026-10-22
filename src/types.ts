export type Page =
  | "overview"
  | "agents"
  | "sessions"
  | "skills"
  | "agent-news"
  | "local-tools"
  | "environment"
  | "launchers"
  | "rename"
  | "scripts"
  | "settings"
  | "sources";
export type Theme = "light" | "dark" | "system";
export interface Source {
  id: string;
  name: string;
  root: string;
  executable: string;
  enabled: boolean;
  version: string | null;
}
export interface SourceStatus extends Source {
  status: string;
  lastScan: string | null;
  lastActivity: string | null;
  files: number;
  diagnostics: string[];
}
export interface Usage {
  input: number;
  output: number;
  cached: number;
  cacheWrite: number;
  reasoning: number | null;
  calls: number | null;
  cost: number | null;
}
export interface UsageRecord {
  id: string;
  sessionId: string;
  agent: string;
  project: string;
  model: string;
  day: string;
  timestamp: string;
  startedAt: string;
  sourceFile: string;
  parentId: string | null;
  usage: Usage;
}
export interface Snapshot {
  sources: SourceStatus[];
  records: UsageRecord[];
  collectedAt: string;
  dataDirectory: string;
}
export interface LaunchEntry {
  id: string;
  name: string;
  kind: "directory" | "file" | "application" | "url";
  target: string;
  args: string[];
  cwd: string;
}
export interface Script {
  id: string;
  name: string;
  path: string;
  interpreter: string;
  cwd: string;
  args: string[];
  timeoutSeconds: number;
}
export interface Settings {
  theme: Theme;
  refreshSeconds: number;
  closeToTray: boolean;
  sources: Source[];
  entries: LaunchEntry[];
  scripts: Script[];
  skills: SkillsSettings;
}
export interface SkillsSettings {
  centralPath: string;
  defaultSyncMode: string;
  allowCopy: boolean;
  repositories: string[];
  projects: string[];
  autoCheckUpdates: boolean;
  migration: { sourcePath: string; status: string; backupPath: string | null };
}
export interface Skill {
  id: string;
  name: string;
  description: string;
  centralPath: string;
  sourceType: string;
  sourceUrl: string | null;
  sourceSubpath: string | null;
  revision: string | null;
  contentHash: string;
  updatedAt: string;
  fileCount: number;
  document: string;
}
export interface SkillTarget {
  id: string;
  skillId: string;
  agentId: string;
  scope: string;
  projectPath: string | null;
  targetPath: string;
  mode: string;
  status: string;
  lastSyncedAt: string | null;
  sourceHash: string;
  error: string | null;
  managed: boolean;
}
export interface SkillScan {
  skills: Skill[];
  diagnostics: string[];
  centralPath: string;
}
export interface FileDifference {
  path: string;
  status: string;
  central: string | null;
  target: string | null;
}
export interface SyncPreview {
  id: string;
  skillId: string;
  sourcePath: string;
  targetPath: string;
  mode: string;
  status: string;
  conflict: boolean;
  managed: boolean;
  differences: FileDifference[];
  targetDocument: string | null;
}
export interface SkillAgentState {
  enabled: boolean | null;
  activationMethod: "entry" | "codex_config" | "entry_and_config" | "read_only" | "unknown" | string;
  capability: "toggleable" | "requires_confirmation" | "unsupported";
  capabilityReason: string | null;
  agentId: string;
  state:
    | "installed"
    | "missing"
    | "central_direct"
    | "target_only"
    | "conflict"
    | "error";
  targetPath: string;
  mode: string;
  managed: boolean;
  contentHash: string | null;
  error: string | null;
  modified: boolean;
}
export interface SkillActivationChange { skillId: string; agentId: string; enabled: boolean; }
export interface SkillActivationRequest { changes: SkillActivationChange[]; projectPath: string | null; allowCopy: boolean; }
export interface SkillActivationPreview extends BatchSkillPreview { requiresConfirmation: boolean; restartRequired: boolean; }
export interface SkillActivationDelta {
  skillId: string;
  targets: SkillAgentState[];
  backupPaths: string[];
  restartRequired: boolean;
}
export interface QuickSkillActivationRequest {
  skillId: string;
  changes: SkillActivationChange[];
  projectPath: string | null;
  allowCopy: boolean;
}
export interface QuickSkillActivationResult {
  requiresConfirmation: boolean;
  preview: SkillActivationPreview | null;
  result: { message: string; backupPath: string | null } | null;
  delta: SkillActivationDelta | null;
}
export interface SkillMatrixRow {
  skillId: string;
  name: string;
  description: string;
  centralState: "present" | "missing" | "conflict";
  targets: SkillAgentState[];
  sources: SkillAgentState[];
  duplicate: boolean;
}
export interface SkillMatrix {
  rows: SkillMatrixRow[];
  diagnostics: string[];
}
export interface BatchSkillRequest {
  skillIds: string[];
  agentIds: string[];
  projectPath: string | null;
  action: "install" | "remove" | "import-and-install";
  conflictPolicy: "keep-target" | "choose-per-item";
  allowCopy: boolean;
}
export interface BatchSkillRow {
  id: string;
  skillId: string;
  agentId: string;
  action: string;
  sourcePath: string;
  targetPath: string;
  mode: string;
  conflict: boolean;
  modified: boolean;
  defaultChoice: string;
  sourceOptions: string[];
  differences: FileDifference[];
}
export interface BatchSkillPreview {
  id: string;
  rows: BatchSkillRow[];
  sourceFingerprint: string;
  targetFingerprints: Record<string, string>;
  conflicts: string[];
}
export interface BatchResolution {
  id: string;
  choice: string;
  sourcePath: string | null;
}
export interface MigrationRow {
  name: string;
  sourcePath: string;
  targetPath: string;
  status: string;
  links: string[];
  differences: FileDifference[];
}
export interface MigrationPreview {
  id: string;
  sourcePath: string;
  centralPath: string;
  rows: MigrationRow[];
  diagnostics: string[];
  directoryLinks: string[];
}
export interface SkillRepository {
  id: string;
  url: string;
  reference: string;
  checkoutPath: string;
  revision: string;
  lastCheckedAt: string;
  error: string | null;
  updateAvailable: boolean;
}
export interface SkillCandidate {
  path: string;
  name: string;
  description: string;
  contentHash: string;
}
export interface SkillOperation {
  at: string;
  operation: string;
  path: string;
  backupPath: string | null;
}
export interface RenameRow {
  from: string;
  to: string;
  error: string | null;
}
export interface RenamePlan {
  id: string;
  rows: RenameRow[];
}
export interface ScriptResult {
  id: string;
  status: string;
  output: string;
  exitCode: number | null;
  startedAt: string;
  finishedAt: string | null;
}
