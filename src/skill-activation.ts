import type { QuickSkillActivationRequest, QuickSkillActivationResult, SkillActivationChange, SkillActivationPreview, SkillAgentState, SkillMatrixRow } from "./types";

export interface ActivationEvent {
  operationId: number;
  kind: "pending" | "success" | "error" | "confirm";
  name: string;
  enabled: boolean | null;
  message: string;
  preview?: SkillActivationPreview;
}
interface TargetUpdate { target: SkillAgentState; pending: boolean }
interface Work {
  id: number;
  row: SkillMatrixRow;
  confirmed: Map<string, SkillAgentState>;
  desired: Map<string, boolean>;
  timer: ReturnType<typeof setTimeout>;
}

// 每个技能只有一个在途请求；界面保留最新意图，响应只更新本次涉及的 Agent。
export class SkillActivationQueue {
  private listeners = new Set<() => void>();
  private updates: Record<string, TargetUpdate> = {};
  private work = new Map<string, Work>();
  private sequence = 0;
  constructor(
    private submit: (request: QuickSkillActivationRequest) => Promise<QuickSkillActivationResult>,
    private projectPath: string | null,
    private allowCopy: boolean,
    private event: (event: ActivationEvent) => void,
  ) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  snapshot = () => this.updates;
  private publish() { this.updates = { ...this.updates }; this.listeners.forEach(listener => listener()); }
  targets(row: SkillMatrixRow) {
    return row.targets.map(target => this.updates[`${row.skillId}:${target.agentId}`]?.target ?? target);
  }
  pending(skillId: string, agentId?: string) {
    return Object.entries(this.updates).some(([key, update]) => update.pending && (agentId ? key === `${skillId}:${agentId}` : key.startsWith(`${skillId}:`)));
  }
  clear() {
    this.updates = Object.fromEntries(Object.entries(this.updates).filter(([, update]) => update.pending));
    this.publish();
  }
  set(row: SkillMatrixRow, changes: SkillActivationChange[]) {
    const targets = this.targets(row);
    const eligible = changes.filter(change => change.skillId === row.skillId && targets.some(target => target.agentId === change.agentId && target.enabled !== null && target.capability !== "unsupported"));
    if (!eligible.length) return;
    let work = this.work.get(row.skillId);
    const running = !!work;
    if (!work) {
      const id = ++this.sequence;
      work = { id, row, confirmed: new Map(targets.map(target => [target.agentId, target])), desired: new Map(), timer: setTimeout(() => {
        const active = this.work.get(row.skillId);
        if (active?.id === id) this.emit(active, "pending", "正在保存更改，可继续操作");
      }, 180) };
      this.work.set(row.skillId, work);
    }
    for (const change of eligible) {
      work.desired.set(change.agentId, change.enabled);
      const target = targets.find(target => target.agentId === change.agentId)!;
      this.updates[`${row.skillId}:${change.agentId}`] = { target: { ...target, enabled: change.enabled }, pending: true };
    }
    this.publish();
    if (!running) void this.drain(work);
  }
  private emit(work: Work, kind: ActivationEvent["kind"], message: string, preview?: SkillActivationPreview) {
    const values = [...work.desired.values()];
    const enabled = values.every(Boolean) ? true : values.every(value => !value) ? false : null;
    this.event({ operationId: work.id, name: work.row.name, kind, enabled, message, preview });
  }
  private async drain(work: Work) {
    try {
      let message = "已保存更改";
      while (true) {
        const changes = [...work.desired].filter(([agentId, enabled]) => work.confirmed.get(agentId)?.enabled !== enabled)
          .map(([agentId, enabled]) => ({ skillId: work.row.skillId, agentId, enabled }));
        if (!changes.length) { this.emit(work, "success", message); break; }
        const result = await this.submit({ skillId: work.row.skillId, changes, projectPath: this.projectPath, allowCopy: this.allowCopy });
        if (result.requiresConfirmation && result.preview) {
          this.emit(work, "confirm", "请确认来源、冲突或外部目录；原内容将保留备份", result.preview);
          break;
        }
        if (!result.delta) throw new Error("启停接口未返回真实状态");
        for (const change of changes) {
          const target = result.delta.targets.find(target => target.agentId === change.agentId);
          if (!target || target.enabled !== change.enabled) throw new Error("目标状态未达到请求值，请重新检查安装入口");
          work.confirmed.set(change.agentId, target);
          this.updates[`${work.row.skillId}:${change.agentId}`] = { target: { ...target, enabled: work.desired.get(change.agentId)! }, pending: true };
        }
        message = result.delta.restartRequired ? "配置已保存，重启 Codex 后生效" : "更改已保存";
        if (result.delta.backupPaths.length) message += ` · 备份：${result.delta.backupPaths.join("\n")}`;
        this.publish();
      }
    } catch (error) {
      this.emit(work, "error", `${String(error)}；界面已恢复到最后确认的状态`);
    } finally {
      clearTimeout(work.timer);
      for (const agentId of work.desired.keys()) {
        this.updates[`${work.row.skillId}:${agentId}`] = { target: work.confirmed.get(agentId)!, pending: false };
      }
      this.work.delete(work.row.skillId);
      this.publish();
    }
  }
}
