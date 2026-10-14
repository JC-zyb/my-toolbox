import type { SkillAgentState } from "../types";

export const skillAgents = { claude: "Claude Code", codex: "Codex", dsh: "DSH" };
const files = { claude: "claude_code", codex: "codex", dsh: "deepseek_harness" };
export function SkillAgentIcon({ state, onToggle, pending = false }: { state: SkillAgentState; onToggle?: () => void; pending?: boolean }) {
  const id = state.agentId as keyof typeof files;
  const status = state.enabled === null ? "状态未知" : state.enabled ? "已启用" : "已关闭";
  const label = `${skillAgents[id]} · ${status}${state.state === "conflict" ? " · 版本冲突" : ""}${state.capabilityReason ? ` · ${state.capabilityReason}` : ""}`;
  const content = <><img className={id === "codex" ? "codex-icon" : ""} src={`/agent-icons/${files[id]}.svg`} alt="" />{state.enabled === null && <span className="skill-icon-unknown">?</span>}</>;
  if (onToggle) return <button type="button" className={`skill-agent-icon ${state.enabled === false ? "disabled" : state.enabled === true ? "enabled" : ""} ${state.enabled === null ? "unknown" : ""} ${pending ? "pending" : ""}`} disabled={state.enabled === null || state.capability === "unsupported"} title={label} aria-label={label} aria-pressed={state.enabled === true} onClick={onToggle}>{content}{pending && <span className="skill-agent-sync" aria-hidden="true" />}</button>;
  return <span className={`skill-agent-icon ${state.enabled === false ? "disabled" : ""} ${state.enabled === null ? "unknown" : ""} ${pending ? "pending" : ""}`} title={label}>
    {content}
  </span>;
}
