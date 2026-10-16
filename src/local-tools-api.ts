import { command } from "./api";

export interface ToolDefinition {
  id: string;
  name: string;
  description: string;
  commands: string[];
  homepage: string;
  repository: string;
  runtimeProvider: string | null;
  packages: { provider: string; package: string }[];
  registered: boolean;
}
export interface ToolInstallation {
  id: string;
  toolId: string;
  version: string | null;
  path: string;
  provider: string | null;
  package: string | null;
  evidence: string;
  resolved: boolean;
  status: string;
  diagnostic: string | null;
}
export interface ProviderCapabilities {
  id: string;
  available: boolean;
  executable: string | null;
  version: string | null;
  diagnostic: string | null;
  install: boolean;
  upgrade: boolean;
  uninstall: boolean;
  versions: boolean;
  setDefault: boolean;
  guide: string;
}
export interface ToolMetadata {
  toolId: string;
  description: string;
  homepage: string;
  repository: string;
  license: string | null;
  latestRelease: string | null;
  releaseUrl: string | null;
  fetchedAt: string;
  error: string | null;
}
export interface ToolSnapshot {
  definitions: ToolDefinition[];
  installations: ToolInstallation[];
  providers: ProviderCapabilities[];
  metadata: ToolMetadata[];
  projects: string[];
  directories: string[];
  scannedAt: string | null;
  diagnostics: string[];
  updates: ToolUpdate[];
  updatesCheckedAt: string | null;
}
export interface ToolUpdate {
  toolId: string;
  installationId: string;
  provider: string;
  installedVersion: string;
  targetVersion: string;
}
export interface ToolUpdateCheck {
  updates: ToolUpdate[];
  checkedAt: string;
  diagnostics: string[];
}
export interface ToolCandidate {
  name: string;
  description: string;
  repository: string;
  homepage: string;
  provider: string;
  package: string | null;
  toolId: string | null;
}
export interface ToolSearch {
  candidates: ToolCandidate[];
  diagnostics: string[];
}
export interface ToolVersion {
  version: string;
  installed: boolean;
  default: boolean;
  path: string | null;
}
export interface ToolVersions {
  provider: string;
  versions: ToolVersion[];
  defaultVersion: string | null;
  diagnostic: string | null;
}
export interface FileChange {
  path: string;
  before: string | null;
  after: string;
}
export interface ProjectToolConfig {
  directory: string;
  files: FileChange[];
  selectedVersion: string | null;
  effectiveVersion: string | null;
  diagnostics: string[];
}
export type ToolAction =
  "install" | "upgrade" | "uninstall" | "set-default" | "pin-project";
export interface ToolOperationRequest {
  toolId: string;
  action: ToolAction;
  installationId: string | null;
  provider: string | null;
  version: string | null;
  projectPath: string | null;
}
export interface ToolOperationPlan {
  id: string;
  request: ToolOperationRequest;
  executable: string | null;
  args: string[];
  files: FileChange[];
  impact: string[];
  createdAt: string;
}
export interface ToolOperation {
  id: string;
  toolId: string;
  action: string;
  provider: string;
  status: string;
  startedAt: string;
  finishedAt: string | null;
  output: string;
  exitCode: number | null;
  backupPaths: string[];
}

export const localToolsApi = {
  list: () => command<ToolSnapshot>("list_local_tools"),
  scan: () => command<ToolSnapshot>("scan_local_tools"),
  search: (query: string) =>
    command<ToolSearch>("search_local_tools", { query }),
  add: (candidate: ToolCandidate) =>
    command<ToolSnapshot>("add_local_tool", { candidate }),
  metadata: (toolId: string) =>
    command<ToolMetadata>("refresh_local_tool_metadata", { toolId }),
  versions: (toolId: string, remote = false) =>
    command<ToolVersions>("local_tool_versions", { toolId, remote }),
  project: (toolId: string, directory: string) =>
    command<ProjectToolConfig>("local_tool_project", { toolId, directory }),
  preview: (request: ToolOperationRequest) =>
    command<ToolOperationPlan>("preview_local_tool_operation", { request }),
  execute: (id: string) =>
    command<ToolOperation>("execute_local_tool_operation", { id }),
  operations: () => command<ToolOperation[]>("list_local_tool_operations"),
  directories: (directories: string[]) =>
    command<void>("set_local_tool_directories", { directories }),
  updates: () => command<ToolUpdateCheck>("check_local_tool_updates"),
};

export const toolActionText: Record<string, string> = {
  install: "安装",
  upgrade: "升级",
  uninstall: "卸载",
  "set-default": "设为默认",
  "pin-project": "固定项目版本",
};
export const toolStatusText: Record<string, string> = {
  queued: "等待执行",
  running: "执行中",
  success: "已完成",
  failed: "失败",
  interrupted: "上次执行中断",
};
