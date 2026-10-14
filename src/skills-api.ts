import { command } from "./api";
import type {
  Skill,
  SkillTarget,
  SkillScan,
  SyncPreview,
  MigrationPreview,
  SkillRepository,
  SkillCandidate,
  SkillOperation,
  SkillMatrix,
  BatchSkillRequest,
  BatchSkillPreview,
  BatchResolution,
  SkillActivationRequest,
  SkillActivationPreview,
  QuickSkillActivationRequest,
  QuickSkillActivationResult,
} from "./types";
export const skillsApi = {
  matrix: (projectPath: string | null, skillId: string | null = null) =>
    command<SkillMatrix>("skill_matrix", { projectPath, skillId }),
  batchPreview: (request: BatchSkillRequest) =>
    command<BatchSkillPreview>(
      request.action === "remove"
        ? "remove_agent_skill_entries"
        : "preview_batch_skill_operation",
      { request },
    ),
  batchApply: (id: string, resolutions: BatchResolution[]) =>
    command<{ message: string; backupPath: string | null }>(
      "apply_batch_skill_operation",
      { id, resolutions },
    ),
  activationPreview: (request: SkillActivationRequest) =>
    command<SkillActivationPreview>("preview_skill_activation", { request }),
  activationApply: (id: string, resolutions: BatchResolution[]) =>
    command<{ message: string; backupPath: string | null }>("apply_skill_activation", { id, resolutions }),
  quickActivation: (request: QuickSkillActivationRequest) =>
    command<QuickSkillActivationResult>("quick_skill_activation", { request }),
  scan: () => command<SkillScan>("scan_skill_sources"),
  get: (id: string) => command<Skill>("get_skill", { id }),
  discovered: (id: string, projectPath: string | null) => command<Skill>("get_discovered_skill", { id, projectPath }),
  targets: (id: string) => command<SkillTarget[]>("list_skill_targets", { id }),
  preview: (
    skillId: string,
    agentId: string,
    projectPath: string | null,
    mode: string,
    allowCopy: boolean,
  ) =>
    command<SyncPreview>("preview_skill_sync", {
      request: { skillId, agentId, projectPath, mode, allowCopy },
    }),
  sync: (id: string, choice: string) =>
    command<SkillTarget>("apply_skill_sync", { id, choice }),
  remove: (id: string) =>
    command<{ message: string; backupPath: string | null }>(
      "remove_skill_target",
      { id },
    ),
  migration: () => command<MigrationPreview>("preview_migration"),
  migrate: (id: string, resolutions: { name: string; choice: string }[]) =>
    command<{ message: string; backupPath: string | null }>("apply_migration", {
      id,
      resolutions,
    }),
  repositories: (check = false) =>
    command<SkillRepository[]>("check_skill_updates", { check }),
  checkout: (url: string, reference: string) =>
    command<{ repository: SkillRepository; candidates: SkillCandidate[] }>(
      "import_github_repository",
      { url, reference },
    ),
  importRepository: (repositoryId: string, paths: string[]) =>
    command<{ skills: Skill[] }>("import_github_repository", {
      repositoryId,
      paths,
    }),
  project: (project: string) =>
    command<SkillTarget[]>("get_project_skills", { project }),
  operations: () => command<SkillOperation[]>("list_skill_operations"),
};
