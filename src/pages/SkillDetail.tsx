import { useEffect, useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import { skillsApi } from "../skills-api";
import type { Skill, SkillMatrixRow, SyncPreview } from "../types";
import type { SkillActivationQueue } from "../skill-activation";
import { SkillAgentIcon, skillAgents } from "./SkillAgentIcon";

export function skillBody(document: string) {
  return document.replace(/^\uFEFF?---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/, "");
}

export function SkillDetail({ skill, workspace, allowCopy, onBack, actions, children, activationQueue }: {
  skill: Skill; workspace: string; allowCopy: boolean; onBack: () => void;
  actions: ReactNode; children: ReactNode; activationQueue: SkillActivationQueue;
}) {
  const [storedRow, setRow] = useState<SkillMatrixRow | null>(null);
  const row = storedRow ? { ...storedRow, targets: activationQueue.targets(storedRow) } : null;
  const [tab, setTab] = useState("content");
  const [source, setSource] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [diff, setDiff] = useState<SyncPreview | null>(null);
  const projectPath = workspace === "global" ? null : workspace;
  useEffect(() => {
    let active = true;
    void skillsApi.matrix(projectPath, skill.id).then(m => { if (active) setRow(m.rows.find(r => r.skillId === skill.id) ?? null); }).catch(e => { if (active) setError(String(e)); });
    return () => { active = false; };
  }, [skill, projectPath]);
  const run = async (fn: () => Promise<void>) => {
    setBusy(true); setError(null);
    try { await fn(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const activate = (agentId: string, enabled: boolean) => {
    if (row) activationQueue.set(row, [{ skillId: skill.id, agentId, enabled }]);
  };
  return <section className="skill-full-detail" aria-busy={busy} aria-label={`${skill.name} 详情`}>
    <div className="skill-detail-toolbar"><button className="button" disabled={busy} onClick={onBack}>← 返回技能库</button><div className="button-row">{actions}</div></div>
    <h1>{skill.name}</h1><p className="skill-full-description">{skill.description}</p>
    <p className="muted">{skill.sourceUrl ?? "本地"} · <span className="mono">{skill.centralPath}</span></p>
    <section className="skill-activation-panel">
      <div className="skill-detail-toolbar"><strong>{projectPath ? "项目工作空间" : "全局工作空间"}</strong><span className="muted">已启用 {row?.targets.filter(t => t.enabled === true).length ?? 0} / 3</span></div>
      <div className="skill-activation-agents">{Object.entries(skillAgents).map(([id, name]) => {
        const state = row?.targets.find(t => t.agentId === id);
        return <label key={id} className={`skill-agent-toggle ${state?.enabled ? "active" : ""}`} title={state?.capabilityReason ?? undefined}>
          <input type="checkbox" aria-label={`启用 ${name}`} disabled={busy || !state || state.enabled === null || state.capability === "unsupported"} checked={state?.enabled === true} onChange={e => activate(id, e.target.checked)} />
          {state && <SkillAgentIcon state={state} pending={activationQueue.pending(skill.id, id)} />}<span>{name}</span><span className="skill-mini-switch" aria-hidden="true" />
          <small>{state?.enabled === null ? "状态未知" : id === "codex" && !projectPath && row?.centralState === "missing" ? "尚未导入" : id === "codex" && !projectPath ? (state?.enabled ? "配置已启用" : "配置已关闭") : state?.enabled ? "已启用" : "已关闭"}</small>
        </label>;
      })}</div>
      {error && <p className="inline-warning" role="alert">{error}</p>}
      {row?.targets.filter(t => t.error).map(t => <p key={t.agentId} className="inline-warning">{skillAgents[t.agentId as keyof typeof skillAgents]}：{t.error}</p>)}
      <p className="muted">{projectPath ? "项目入口关闭不影响全局技能。" : "Codex 配置变更需重启 Codex 后生效。"}</p>
    </section>
    <div className="skill-content-tabs" role="tablist" aria-label="技能详情">
      {[["content", "内容"], ["diff", "差异"], ["origin", "来源"]].map(([id, name]) => <button role="tab" aria-selected={tab === id} key={id} onClick={() => setTab(id)}>{name}</button>)}
      {tab === "content" && <button className="text-button" onClick={() => setSource(!source)}>{source ? "渲染正文" : "查看源码"}</button>}
    </div>
    <div role="tabpanel">
      {tab === "content" && (source ? <pre className="skill-document">{skill.document}</pre> : <div className="skill-markdown"><ReactMarkdown skipHtml>{skillBody(skill.document)}</ReactMarkdown></div>)}
      {tab === "origin" && row?.sources.map(s => <div className="skill-source-detail" key={s.targetPath}><strong>{s.agentId === "central" ? "中央库" : s.agentId === "legacy" ? "旧 .skills-manager 库" : s.agentId}</strong><p className="mono">{s.targetPath}</p><p>{s.mode} · {s.managed ? "工具管理" : "外部来源"}</p><p className="mono">SHA-256：{s.contentHash ?? "读取失败"}</p></div>)}
      {tab === "diff" && <><p>选择 Agent，读取中央版本与目标安装的真实文件差异。</p><div className="button-row">{Object.entries(skillAgents).filter(([id]) => id !== "codex" || projectPath).map(([id, name]) => <button className="button" disabled={busy || !row?.sources.some(s => s.agentId === "central")} key={id} onClick={() => void run(async () => setDiff(await skillsApi.preview(skill.id, id, projectPath, "auto", allowCopy)))}>{name}</button>)}</div>{diff && <div className="skill-differences">{diff.differences.length ? diff.differences.map(d => <details key={d.path}><summary>{d.path}</summary><div className="skill-diff-columns"><pre>{d.central ?? "无文件或二进制文件"}</pre><pre>{d.target ?? "无文件或二进制文件"}</pre></div></details>) : <p>未发现文件内容差异。</p>}</div>}</>}
    </div>
    <details className="skill-advanced"><summary>高级信息：安装路径、模式、指纹、备份与兼容入口</summary><p className="mono">内容 SHA-256：{skill.contentHash}</p><p>支持文件：{skill.fileCount} 个</p>{children}</details>
  </section>;
}
