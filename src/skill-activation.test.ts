import { describe, expect, it, vi } from "vitest";
import { SkillActivationQueue, type ActivationEvent } from "./skill-activation";
import type { QuickSkillActivationResult, SkillAgentState, SkillMatrixRow } from "./types";

const target = (agentId: string, enabled = false): SkillAgentState => ({ agentId, enabled, state: enabled ? "installed" : "missing", activationMethod: "entry", capability: "toggleable", capabilityReason: null, targetPath: `/tmp/${agentId}`, mode: "symlink", managed: true, contentHash: null, error: null, modified: false });
const row = (skillId = "alpha"): SkillMatrixRow => ({ skillId, name: skillId, description: "test", centralState: "present", targets: [target("claude"), target("codex"), target("dsh")], sources: [], duplicate: false });
function deferred() {
  let resolve!: (value: QuickSkillActivationResult) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<QuickSkillActivationResult>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const success = (skillId: string, agentId: string, enabled: boolean): QuickSkillActivationResult => ({ requiresConfirmation: false, preview: null, result: { message: "saved", backupPath: null }, delta: { skillId, targets: [target(agentId, enabled)], backupPaths: [], restartRequired: false } });
const flush = async () => { await Promise.resolve(); await Promise.resolve(); };

describe("Skill activation queue", () => {
  it("立即更新，连续点击只补交最后意图，旧响应不让开关闪回", async () => {
    const first = deferred(), last = deferred();
    const submit = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(last.promise);
    const events: ActivationEvent[] = [];
    const queue = new SkillActivationQueue(submit, null, false, event => events.push(event));
    const skill = row();
    const change = (enabled: boolean) => queue.set(skill, [{ skillId: "alpha", agentId: "claude", enabled }]);
    change(true); change(false); change(true); change(false);
    expect(queue.targets(skill)[0].enabled).toBe(false);
    expect(queue.pending("alpha", "claude")).toBe(true);
    expect(submit).toHaveBeenCalledTimes(1);
    first.resolve(success("alpha", "claude", true)); await flush();
    expect(queue.targets(skill)[0].enabled).toBe(false);
    expect(submit).toHaveBeenCalledTimes(2);
    expect(submit.mock.calls[1][0].changes).toEqual([{ skillId: "alpha", agentId: "claude", enabled: false }]);
    last.resolve(success("alpha", "claude", false)); await flush();
    expect(queue.pending("alpha")).toBe(false);
    expect(events.map(event => event.kind)).toEqual(["success"]);
  });
  it("不同技能可并行，失败只回滚失败技能，不覆盖其他技能", async () => {
    const first = deferred(), other = deferred();
    const submit = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(other.promise);
    const events: ActivationEvent[] = [];
    const queue = new SkillActivationQueue(submit, null, false, event => events.push(event));
    const alpha = row(), beta = row("beta");
    queue.set(alpha, [{ skillId: "alpha", agentId: "claude", enabled: true }]);
    queue.set(beta, [{ skillId: "beta", agentId: "dsh", enabled: true }]);
    expect(submit).toHaveBeenCalledTimes(2);
    first.reject(new Error("disk unavailable")); await flush();
    expect(queue.targets(alpha)[0].enabled).toBe(false);
    expect(queue.targets(beta)[2].enabled).toBe(true);
    expect(queue.pending("beta")).toBe(true);
    other.resolve(success("beta", "dsh", true)); await flush();
    expect(events.map(event => event.kind)).toEqual(["error", "success"]);
  });
  it("同一技能不同 Agent 的增量和失败不会覆盖已确认目标", async () => {
    const first = deferred(), second = deferred();
    const submit = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const queue = new SkillActivationQueue(submit, null, false, () => {});
    const skill = row();
    queue.set(skill, [{ skillId: "alpha", agentId: "claude", enabled: true }]);
    queue.set(skill, [{ skillId: "alpha", agentId: "dsh", enabled: true }]);
    first.resolve(success("alpha", "claude", true)); await flush();
    expect(queue.targets(skill)[0].enabled).toBe(true);
    expect(queue.targets(skill)[2].enabled).toBe(true);
    second.reject(new Error("failed")); await flush();
    expect(queue.targets(skill)[0].enabled).toBe(true);
    expect(queue.targets(skill)[2].enabled).toBe(false);
  });
  it("确认预览会恢复显示，等待确认时不显示成功", async () => {
    const request = deferred(); const events: ActivationEvent[] = [];
    const queue = new SkillActivationQueue(() => request.promise, null, false, event => events.push(event));
    const skill = row();
    queue.set(skill, [{ skillId: "alpha", agentId: "claude", enabled: true }]);
    request.resolve({ requiresConfirmation: true, preview: { id: "preview", rows: [], conflicts: [], sourceFingerprint: "", targetFingerprints: {}, restartRequired: false, requiresConfirmation: true }, result: null, delta: null }); await flush();
    expect(queue.targets(skill)[0].enabled).toBe(false);
    expect(events.map(event => event.kind)).toEqual(["confirm"]);
  });
  it("未知或不支持的目标不会提交；缺少真实增量不会假成功", async () => {
    const request = deferred(); const events: ActivationEvent[] = [];
    const submit = vi.fn(() => request.promise);
    const queue = new SkillActivationQueue(submit, null, false, event => events.push(event));
    const skill = row(); skill.targets[0].enabled = null;
    queue.set(skill, [{ skillId: "alpha", agentId: "claude", enabled: true }]);
    expect(submit).not.toHaveBeenCalled();
    queue.set(skill, [{ skillId: "alpha", agentId: "dsh", enabled: true }]);
    request.resolve({ requiresConfirmation: false, preview: null, result: null, delta: null }); await flush();
    expect(events.map(event => event.kind)).toEqual(["error"]);
    expect(queue.targets(skill)[2].enabled).toBe(false);
  });
  it("短操作不闪处理中通知，长操作只有一个处理中通知", async () => {
    vi.useFakeTimers();
    try {
      const request = deferred(); const events: ActivationEvent[] = [];
      const queue = new SkillActivationQueue(() => request.promise, null, false, event => events.push(event));
      queue.set(row(), [{ skillId: "alpha", agentId: "claude", enabled: true }]);
      await vi.advanceTimersByTimeAsync(179);
      expect(events).toHaveLength(0);
      await vi.advanceTimersByTimeAsync(1000);
      expect(events.map(event => event.kind)).toEqual(["pending"]);
      request.resolve(success("alpha", "claude", true)); await flush();
      expect(events.map(event => event.kind)).toEqual(["pending", "success"]);
    } finally { vi.useRealTimers(); }
  });
});
