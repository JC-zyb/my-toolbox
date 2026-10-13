import { useEffect, useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import { skillsApi } from "../skills-api";
import { Modal } from "../components";
import type { BatchResolution, Skill, SkillActivationPreview, SkillMatrixRow, SyncPreview } from "../types";
import { SkillAgentIcon, skillAgents } from "./SkillAgentIcon";
import type { SkillToastItem } from "./SkillOperationToast";

export function skillBody(document: string) {
  return document.replace(/^\uFEFF?---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/, "");
}

export function SkillDetail({ skill, workspace, allowCopy, onBack, onChanged, actions, children, notify, updateToast }: {
  skill: Skill; workspace: string; allowCopy: boolean; onBack: () => void;
  onChanged: () => Promise<void>; actions: ReactNode; children: ReactNode;
  notify: (item: Omit<SkillToastItem, "id">) => string;
  updateToast: (id: string, patch: Partial<SkillToastItem>) => void;
}) {
  const [row, setRow] = useState<SkillMatrixRow | null>(null);
  const [tab, setTab] = useState("content");
  const [source, setSource] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [preview, setPreview] = useState<SkillActivationPreview | null>(null);
  const [choices, setChoices] = useState<Record<string, BatchResolution>>({});
  const [diff, setDiff] = useState<SyncPreview | null>(null);
  const [pendingAgents, setPendingAgents] = useState<Set<string>>(new Set());
  const projectPath = workspace === "global" ? null : workspace;
  useEffect(() => {
    let active = true;
    void skillsApi.matrix(projectPath).then(m => { if (active) setRow(m.rows.find(r => r.skillId === skill.id) ?? null); }).catch(e => { if (active) setError(String(e)); });
    return () => { active = false; };
  }, [skill, projectPath]);
  const run = async (fn: () => Promise<void>) => {
    setBusy(true); setError(null);
    try { await fn(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const apply = async (p: SkillActivationPreview, resolutions: BatchResolution[]) => {
    const result = await skillsApi.activationApply(p.id, resolutions);
    setPreview(null); setNotice(`${result.message}${result.backupPath ? `；备份：${result.backupPath}` : ""}`);
    notify({ kind: "success", title: `${skill.name} 启停已完成`, message: `${result.message}${result.backupPath ? `；备份：${result.backupPath}` : ""}` });
    await onChanged();
  };
  const activate = (agentId: string, enabled: boolean) => void (async () => {
    const state = row?.targets.find((target) => target.agentId === agentId);
    if (!state || state.enabled === null || state.capability === "unsupported" || pendingAgents.has(agentId)) return;
    const previous = row;
    setPendingAgents((current) => new Set(current).add(agentId));
    setRow((current) => current ? { ...current, targets: current.targets.map((target) => target.agentId === agentId ? { ...target, enabled } : target) } : current);
    const toastId = notify({ kind: "pending", title: `${enabled ? "正在启用" : "正在关闭"} ${skill.name}`, message: skillAgents[agentId as keyof typeof skillAgents] });
    try {
      const quick = await skillsApi.quickActivation({ skillId: skill.id, changes: [{ skillId: skill.id, agentId, enabled }], projectPath, allowCopy });
      if (quick.requiresConfirmation && quick.preview) {
        setRow(previous);
        const resolutions = quick.preview.rows.map((r) => ({ id: r.id, choice: r.defaultChoice, sourcePath: r.sourcePath }));
        setChoices(Object.fromEntries(resolutions.map((r) => [r.id, r])));
        setPreview(quick.preview);
        updateToast(toastId, { kind: "confirm", title: `${skill.name} 需要确认`, message: "存在冲突或外部目录，请查看预览后应用", actionLabel: "查看预览", onAction: () => setPreview(quick.preview!) });
      } else if (quick.delta) {
        setRow((current) => current ? { ...current, targets: current.targets.map((target) => quick.delta!.targets.find((next) => next.agentId === target.agentId) ?? target) } : current);
        const message = quick.result?.message ?? "操作已完成";
        updateToast(toastId, { kind: "success", title: `${skill.name} 已${enabled ? "启用" : "关闭"}`, message: quick.delta.restartRequired && !message.includes("重启 Codex") ? `${message}；重启 Codex 后生效` : message });
      }
    } catch (e) {
      setRow(previous);
      updateToast(toastId, { kind: "error", title: `${skill.name} 操作失败`, message: `${e}；已恢复原状态` });
    } finally {
      setPendingAgents((current) => { const next = new Set(current); next.delete(agentId); return next; });
    }
  })();
  return <section className="skill-full-detail" aria-busy={busy} aria-label={`${skill.name} 详情`}>
    <div className="skill-detail-toolbar"><button className="button" disabled={busy} onClick={onBack}>← 返回技能库</button><div className="button-row">{actions}</div></div>
    <h1>{skill.name}</h1><p className="skill-full-description">{skill.description}</p>
    <p className="muted">{skill.sourceUrl ?? "本地"} · <span className="mono">{skill.centralPath}</span></p>
    <section className="skill-activation-panel">
      <div className="skill-detail-toolbar"><strong>{projectPath ? "项目工作空间" : "全局工作空间"}</strong><span className="muted">已启用 {row?.targets.filter(t => t.enabled === true).length ?? 0} / 3</span></div>
      <div className="skill-activation-agents">{Object.entries(skillAgents).map(([id, name]) => {
        const state = row?.targets.find(t => t.agentId === id);
        return <label key={id} className={`skill-agent-toggle ${state?.enabled ? "active" : ""}`}>
          <input type="checkbox" aria-label={`启用 ${name}`} disabled={busy || pendingAgents.has(id) || !state || state.enabled === null || state.capability === "unsupported"} checked={state?.enabled === true} onChange={e => activate(id, e.target.checked)} />
          {state && <SkillAgentIcon state={state} />}<span>{name}</span>
          <small>{state?.enabled === null ? "状态未知" : id === "codex" && !projectPath && row?.centralState === "missing" ? "尚未导入" : id === "codex" && !projectPath ? (state?.enabled ? "配置已启用" : "配置已关闭") : state?.enabled ? "已启用" : "已关闭"}</small>
        </label>;
      })}</div>
      {Array.from(pendingAgents).map((id) => <p key={id} role="status">正在更新 {skillAgents[id as keyof typeof skillAgents]} 启停状态…</p>)}
      {error && <p className="inline-warning" role="alert">{error}</p>}
      {row?.targets.filter(t => t.error).map(t => <p key={t.agentId} className="inline-warning">{skillAgents[t.agentId as keyof typeof skillAgents]}：{t.error}</p>)}
      {notice && <p className="inline-success" role="status">{notice}</p>}
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
    <Modal open={!!preview} error={error} onClose={() => { if (!busy) setPreview(null); }} title="启停确认" description="外部内容、版本冲突或首次导入需要确认；原内容保留备份" wide>
      {preview?.rows.map(r => <section className="skill-source-detail" key={r.id}><h3>{r.skillId} · {r.agentId === "central" ? "导入中央库" : skillAgents[r.agentId as keyof typeof skillAgents]}</h3><p>{r.action === "config" ? "更新 Codex 配置，重启后生效" : r.action === "remove" ? "关闭并备份当前安装入口" : r.action === "import" ? "首次导入中央库" : "启用安装入口"} · {r.mode}{r.modified && " · 本地已修改"}</p><p className="mono">来源：{choices[r.id]?.sourcePath}</p><p className="mono">目标：{r.targetPath}</p><p className="mono">指纹：{preview.targetFingerprints[r.targetPath]}</p>
        {r.defaultChoice !== "noop" && <label>处理方式<select aria-label={`处理 ${r.id}`} disabled={busy} value={choices[r.id]?.choice} onChange={e => setChoices(c => ({ ...c, [r.id]: { ...c[r.id], choice: e.target.value } }))}><option value="skip">保留已有版本并跳过</option><option value="central">{r.action === "remove" ? "确认备份并关闭" : r.action === "import" ? "确认导入来源" : "使用中央版本"}</option>{r.conflict && !["config", "import"].includes(r.action) && <option value="target">保留目标并导入中央</option>}</select></label>}
        {r.action === "import" && <label>导入来源<select aria-label={`导入来源 ${r.skillId}`} disabled={busy} value={choices[r.id]?.sourcePath ?? r.sourcePath} onChange={e => setChoices(c => ({ ...c, [r.id]: { ...c[r.id], sourcePath: e.target.value } }))}>{r.sourceOptions.map(p => <option key={p}>{p}</option>)}</select></label>}
      </section>)}
      <div className="modal-actions"><button className="button" disabled={busy} onClick={() => setPreview(null)}>取消</button><button className="button primary" disabled={busy || !preview} onClick={() => void run(async () => { if (preview) await apply(preview, Object.values(choices)); })}>确认应用启停</button></div>
    </Modal>
  </section>;
}
