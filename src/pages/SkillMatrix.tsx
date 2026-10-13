import { useEffect, useMemo, useState } from "react";
import { MoreVertical, Power } from "lucide-react";
import { Empty, Modal } from "../components";
import { desktop } from "../api";
import { skillsApi } from "../skills-api";
import { SkillAgentIcon, skillAgents } from "./SkillAgentIcon";
import type { SkillToastItem } from "./SkillOperationToast";
import type {
  BatchSkillPreview,
  BatchSkillRequest,
  BatchResolution,
  Skill,
  SkillMatrix as Matrix,
  SkillMatrixRow,
  SkillRepository,
} from "../types";

const agents = { codex: "Codex", claude: "Claude Code", dsh: "DSH" };
const modes: Record<string, string> = {
  auto: "自动链接",
  junction: "目录联接",
  symlink: "符号链接",
  copy: "目录副本",
  central: "直接发现中央库",
};
const toggle = (items: string[], id: string) =>
  items.includes(id) ? items.filter((x) => x !== id) : [...items, id];

export function SkillMatrix({
  skills,
  refreshKey,
  workspace,
  query,
  source,
  agent,
  status,
  repos,
  allowCopy,
  onDetail,
  onChanged,
  onAction,
  notify,
  updateToast,
}: {
  skills: Skill[];
  refreshKey: number;
  workspace: string;
  query: string;
  source: string;
  agent: string;
  status: string;
  repos: SkillRepository[];
  allowCopy: boolean;
  onDetail: (id: string) => void;
  onChanged: () => Promise<void>;
  onAction?: (action: "edit" | "export" | "delete", id: string) => void;
  notify: (item: Omit<SkillToastItem, "id">) => string;
  updateToast: (id: string, patch: Partial<SkillToastItem>) => void;
}) {
  const [matrix, setMatrix] = useState<Matrix | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [selectedAgents, setSelectedAgents] = useState<string[]>([
    "claude",
    "dsh",
  ]);
  const [preview, setPreview] = useState<BatchSkillPreview | null>(null);
  const [resolutions, setResolutions] = useState<
    Record<string, BatchResolution>
  >({});
  const [origins, setOrigins] = useState<SkillMatrixRow | null>(null);
  const [busy, setBusy] = useState(false);
  const [multi, setMulti] = useState(false);
  const [activation, setActivation] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [menu, setMenu] = useState<string | null>(null);
  useEffect(() => {
    if (!desktop) return;
    let cancelled = false;
    setMatrix(null);
    setPreview(null);
    setError(null);
    void skillsApi
      .matrix(workspace === "global" ? null : workspace)
      .then((value) => {
        if (!cancelled) setMatrix(value);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [skills, workspace, refreshKey]);
  const rows = useMemo(
    () =>
      (matrix?.rows ?? []).filter((row) => {
        const skill = skills.find((s) => s.id === row.skillId);
        const ts =
          agent === "all"
            ? row.targets
            : row.targets.filter((t) => t.agentId === agent);
        return (
          `${row.name} ${row.description}`
            .toLowerCase()
            .includes(query.toLowerCase()) &&
          (source === "all" || skill?.sourceType === source || (source === "local" && !skill)) &&
          (agent === "all" || ts.some((t) => t.enabled !== null)) &&
          (status === "all" ||
            (status === "duplicate" && row.duplicate) ||
            (status === "target_only" &&
              !row.sources.some((s) => s.agentId === "central")) ||
            (status === "conflict" &&
              (row.centralState === "conflict" ||
                ts.some((t) => t.state === "conflict"))) ||
            (status === "missing" && ts.some((t) => t.enabled === false)) ||
            (status === "target_newer" && ts.some((t) => t.modified)) ||
            (status === "error" && row.sources.some((t) => t.error)) ||
            (status === "updates" &&
              repos.some(
                (r) => r.url === skill?.sourceUrl && r.updateAvailable,
              )))
        );
      }),
    [matrix, skills, workspace, query, source, agent, status, repos],
  );
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const makePreview = (action: BatchSkillRequest["action"]) =>
    void run(async () => {
      setActivation(false);
      const p = await skillsApi.batchPreview({
        skillIds: selected,
        agentIds: selectedAgents,
        projectPath: workspace === "global" ? null : workspace,
        action,
        conflictPolicy: "keep-target",
        allowCopy,
      });
      setResolutions(
        Object.fromEntries(
          p.rows.map((row) => [
            row.id,
            {
              id: row.id,
              choice: row.defaultChoice,
              sourcePath: row.sourcePath,
            },
          ]),
        ),
      );
      setPreview(p);
    });
  const activationPreview = (enabled: boolean) => void run(async () => {
    const p = await skillsApi.activationPreview({ changes: selected.flatMap(skillId => selectedAgents.map(agentId => ({ skillId, agentId, enabled }))), projectPath: workspace === "global" ? null : workspace, allowCopy });
    setActivation(true);
    setResolutions(Object.fromEntries(p.rows.map(row => [row.id, { id: row.id, choice: row.defaultChoice, sourcePath: row.sourcePath }])));
    setPreview(p);
  });
  const change = (id: string, patch: Partial<BatchResolution>) =>
    setResolutions((current) => ({
      ...current,
      [id]: { ...current[id], ...patch },
    }));
  const actionable = (row: SkillMatrixRow) => row.targets.filter(t => t.capability !== "unsupported" && t.enabled !== null);
  const switchState = (row: SkillMatrixRow) => {
    const states = actionable(row).map(t => t.enabled);
    if (!states.length) return "disabled";
    if (states.every(Boolean)) return "on";
    if (states.every(v => !v)) return "off";
    return "mixed";
  };
  const toggleActivation = (row: SkillMatrixRow, agentId?: string, requested?: boolean) => void (async () => {
    const targets = agentId ? row.targets.filter(t => t.agentId === agentId) : actionable(row);
    const eligible = targets.filter(t => t.capability !== "unsupported" && t.enabled !== null);
    if (!eligible.length) return;
    const desired = requested ?? (agentId ? eligible[0].enabled !== true : switchState(row) !== "on");
    const changes = eligible.map(t => ({ skillId: row.skillId, agentId: t.agentId, enabled: desired }));
    const key = agentId ? `${row.skillId}:${agentId}` : `${row.skillId}:all`;
    if (pending.has(key)) return;
    const previous = matrix;
    setPending(current => new Set(current).add(key));
    setMatrix(current => current ? { ...current, rows: current.rows.map(r => r.skillId === row.skillId ? { ...r, targets: r.targets.map(t => changes.find(c => c.agentId === t.agentId) ? { ...t, enabled: desired } : t) } : r) } : current);
    const toastId = notify({ kind: "pending", title: `${desired ? "正在启用" : "正在关闭"} ${row.name}`, message: agentId ? agents[agentId as keyof typeof agents] : "全部可管理 Agent" });
    try {
      const result = await skillsApi.quickActivation({ skillId: row.skillId, changes, projectPath: workspace === "global" ? null : workspace, allowCopy });
      if (result.requiresConfirmation && result.preview) {
        setMatrix(previous);
        setActivation(true);
        setPreview(result.preview);
        setResolutions(Object.fromEntries(result.preview.rows.map(r => [r.id, { id: r.id, choice: r.defaultChoice, sourcePath: r.sourcePath }])));
        updateToast(toastId, { kind: "confirm", title: `${row.name} 需要确认`, message: "存在冲突或外部目录，请查看预览后应用", actionLabel: "查看预览", onAction: () => setPreview(result.preview!) });
      } else {
        setMatrix(current => result.delta && current ? { ...current, rows: current.rows.map(r => r.skillId === row.skillId ? { ...r, targets: r.targets.map(t => result.delta!.targets.find(next => next.agentId === t.agentId) ?? t) } : r) } : current);
        updateToast(toastId, { kind: "success", title: `${row.name} 已${desired ? "启用" : "关闭"}`, message: result.result?.message });
      }
    } catch (e) {
      setMatrix(previous);
      updateToast(toastId, { kind: "error", title: `${row.name} 操作失败`, message: `${e}` });
    } finally {
      setPending(current => { const next = new Set(current); next.delete(key); return next; });
    }
  })();
  const allVisibleSelected = rows.length > 0 && rows.every(row => selected.includes(row.skillId));
  const someVisibleSelected = rows.some(row => selected.includes(row.skillId));
  const hasWork = preview?.rows.some(
    (row) =>
      !["noop", "skip"].includes(
        resolutions[row.id]?.choice ?? row.defaultChoice,
      ),
  );
  return (
    <div className="skill-matrix-section" aria-busy={busy}>
      {error && !preview && (
        <p className="inline-warning" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="inline-success" role="status">
          {notice}
        </p>
      )}
      {matrix?.diagnostics.map((d, i) => (
        <p className="inline-warning" key={i}>
          {d}
        </p>
      ))}
      <div className="skill-card-toolbar"><span>{rows.length} 个技能</span><button className="button" aria-pressed={multi} onClick={() => { setMulti(!multi); setSelected([]); }}>{multi ? "退出多选" : "多选"}</button></div>
      {multi && <div className="skill-batch-toolbar">
        <span aria-live="polite">
          显示 {rows.length} 个技能 · 已选 {selected.length} 个
        </span>
        <label className="skill-checkbox skill-select-all">
          <input type="checkbox" aria-label="全选当前结果" checked={allVisibleSelected} ref={input => { if (input) input.indeterminate = !allVisibleSelected && someVisibleSelected; }} onChange={() => setSelected(allVisibleSelected ? selected.filter(id => !rows.some(row => row.skillId === id)) : [...new Set([...selected, ...rows.map(row => row.skillId)])])} />
          全选当前结果
        </label>
        <fieldset disabled={busy}>
          <legend>操作 Agent</legend>
          {Object.entries(agents).map(([id, label]) => (
            <label key={id} className="skill-checkbox">
              <input
                type="checkbox"
                aria-label={`操作 ${label}`}
                checked={selectedAgents.includes(id)}
                onChange={() => setSelectedAgents(toggle(selectedAgents, id))}
              />
              {label}
            </label>
          ))}
        </fieldset>
        <div className="button-row">
          <button
            className="button primary"
            disabled={busy || !selected.length || !selectedAgents.length}
            onClick={() => activationPreview(true)}
          >
            批量启用
          </button>
          <button
            className="button"
            disabled={busy || !selected.length || !selectedAgents.length}
            onClick={() => activationPreview(false)}
          >
            批量关闭
          </button>
        </div>
        <details><summary>高级操作</summary><button className="button" disabled={busy || !selected.length} onClick={() => makePreview("remove")}>兼容入口清理预览</button></details>
      </div>}
      <div className="skill-card-grid">
        {rows.map(row => <article className={`skill-card ${menu === row.skillId ? "menu-open" : ""}`} key={row.skillId}>
          {multi ? <input type="checkbox" className="skill-card-select" aria-label={`选择技能 ${row.name}`} disabled={busy} checked={selected.includes(row.skillId)} onChange={() => setSelected(toggle(selected, row.skillId))} /> : <>
            <button type="button" className="skill-card-more" aria-label={`${row.name} 更多操作`} aria-expanded={menu === row.skillId} onClick={() => setMenu(menu === row.skillId ? null : row.skillId)}><MoreVertical size={17} /></button>
            {menu === row.skillId && <div className="skill-card-menu" role="menu">
              <button role="menuitem" onClick={() => { setMenu(null); onDetail(row.skillId); }}>查看完整详情</button>
              <button role="menuitem" onClick={() => { setMenu(null); toggleActivation(row, undefined, true); }}>启用全部 Agent</button>
              <button role="menuitem" onClick={() => { setMenu(null); toggleActivation(row, undefined, false); }}>关闭全部 Agent</button>
              <button role="menuitem" onClick={() => { setMenu(null); setOrigins(row); }}>查看来源与差异</button>
              {onAction && <>
                <button role="menuitem" onClick={() => { setMenu(null); onAction("edit", row.skillId); }}>编辑 Skill</button>
                <button role="menuitem" onClick={() => { setMenu(null); onAction("export", row.skillId); }}>导出 Skill</button>
                <button role="menuitem" onClick={() => { setMenu(null); onAction("delete", row.skillId); }}>删除中央库 Skill</button>
              </>}
            </div>}
          </>}
          {!multi && <button type="button" className={`skill-card-switch ${switchState(row)} ${pending.has(`${row.skillId}:all`) ? "pending" : ""}`} disabled={switchState(row) === "disabled" || pending.has(`${row.skillId}:all`)} aria-label={`${row.name} 全部 Agent 启停`} aria-pressed={switchState(row) === "on"} onClick={() => toggleActivation(row)}><Power size={15} /></button>}
          <button className="skill-card-open" aria-label={`查看 ${row.name}`} onClick={() => onDetail(row.skillId)}>
            <strong>{row.name}</strong>
            <span className="skill-card-description">{row.description || "查看技能内容与来源"}</span>
          </button>
          <div className="skill-card-flags">
            {!row.sources.some(s => s.agentId === "central") && <span>尚未导入</span>}
            {row.centralState === "conflict" && <span className="danger-text">版本冲突</span>}
          </div>
          <footer>
            <span className="skill-source-tag">{skills.find(s => s.id === row.skillId)?.sourceType === "github" ? "GitHub" : row.sources.some(s => s.agentId === "central") ? "本地" : "Agent 独有"}</span>
            <div className="skill-card-agents">{Object.keys(skillAgents).map(id => { const state = row.targets.find(t => t.agentId === id); const key = `${row.skillId}:${id}`; return state && <SkillAgentIcon key={id} state={state} pending={pending.has(key)} onToggle={() => toggleActivation(row, id)} />; })}</div>
          </footer>
        </article>)}
      </div>
      {!rows.length && (
        <Empty
          title={matrix ? "没有匹配的 Skill" : "连接你的中央库"}
          description={
            matrix
              ? "调整筛选，或导入和新建 Skill。"
              : "桌面连接后显示中央库和各 Agent 的安装状态。"
          }
        />
      )}
      <Modal
        open={!!origins}
        onClose={() => setOrigins(null)}
        title={`${origins?.name ?? "Skill"} 来源`}
        description="入口路径、内容指纹与管理状态"
        wide
      >
        {origins?.sources.map((s) => (
          <div
            className="skill-source-detail"
            key={`${s.agentId}:${s.targetPath}`}
          >
            <strong>
              {agents[s.agentId as keyof typeof agents] ??
                (s.agentId === "central"
                  ? "中央库"
                  : s.agentId === "legacy"
                    ? "旧 .skills-manager 库"
                    : "Codex 兼容目录")}
            </strong>
            <p className="mono">{s.targetPath}</p>
            <p>
              {modes[s.mode]} · {s.managed ? "工具管理" : "已有内容"}
              {s.modified && " · 本地已修改"}
            </p>
            <p className="mono">SHA-256：{s.contentHash ?? "读取失败"}</p>
            {s.error && <p role="alert">{s.error}</p>}
          </div>
        ))}
      </Modal>
      <Modal
        error={error}
        open={!!preview}
        onClose={() => {
          if (!busy) setPreview(null);
        }}
        title="批量操作预览"
        description="确认影响范围后应用；目标原内容备份，冲突默认保留已有版本"
        wide
      >
        {preview && (
          <>
            <p>
              共 {preview.rows.length} 项 · {preview.conflicts.length} 项冲突 ·
              使用自动链接
              {allowCopy ? "，允许链接失败后复制" : "，失败时显示原因"}
            </p>
            <p>
              {activation ? "Codex 修改配置，重启后生效；其他 Agent 关闭时备份安装入口。" : "兼容入口清理保留中央内容，不修改 Codex 启停配置。"}
            </p>
            <div className="skill-batch-preview">
              {preview.rows.map((row) => (
                <section className="skill-source-detail" key={row.id}>
                  <h3>
                    {row.skillId} ·{" "}
                    {row.agentId === "central"
                      ? "导入中央库"
                      : agents[row.agentId as keyof typeof agents]}
                  </h3>
                  <p>
                    {row.action === "direct"
                      ? "中央库直接可用，无需创建入口"
                      : row.action === "remove"
                        ? "移除安装并保留备份"
                        : row.action === "import"
                          ? "复制完整 Skill 到中央库"
                          : "安装入口"}{" "}
                    · {modes[row.mode]}
                  </p>
                  {row.modified && (
                    <p className="inline-warning">
                      存在本地修改，默认保留已有版本并跳过。
                    </p>
                  )}
                  <dl className="skill-batch-paths">
                    <dt>来源</dt>
                    <dd className="mono">
                      {resolutions[row.id]?.sourcePath ?? row.sourcePath}
                    </dd>
                    <dt>目标</dt>
                    <dd className="mono">{row.targetPath}</dd>
                    <dt>来源指纹</dt>
                    <dd className="mono">
                      {
                        preview.targetFingerprints[
                          resolutions[row.id]?.sourcePath ?? row.sourcePath
                        ]
                      }
                    </dd>
                    <dt>目标指纹</dt>
                    <dd className="mono">
                      {preview.targetFingerprints[row.targetPath]}
                    </dd>
                  </dl>
                  {row.action !== "direct" && row.defaultChoice !== "noop" && (
                    <label className="form-stack">
                      {row.conflict ? "内容冲突处理" : "操作选择"}
                      <select
                        aria-label={`处理 ${row.id}`}
                        value={resolutions[row.id]?.choice}
                        disabled={busy}
                        onChange={(e) =>
                          change(row.id, { choice: e.target.value })
                        }
                      >
                        <option value="skip">保留已有版本并跳过</option>
                        <option value="central">
                          {row.action === "remove"
                            ? "确认移除当前安装（备份）"
                            : row.action === "import"
                              ? "导入所选来源"
                              : "使用中央版本"}
                        </option>
                        {row.conflict && row.action !== "import" && (
                          <option value="target">
                            保留目标并导入中央
                            {row.action === "remove" ? "，再移除安装" : ""}
                          </option>
                        )}
                      </select>
                    </label>
                  )}
                  {row.action === "import" && row.sourceOptions.length > 1 && (
                    <label className="form-stack">
                      导入来源
                      <select
                        aria-label={`导入来源 ${row.skillId}`}
                        disabled={busy}
                        value={
                          resolutions[row.id]?.sourcePath ?? row.sourcePath
                        }
                        onChange={(e) =>
                          change(row.id, { sourcePath: e.target.value })
                        }
                      >
                        {row.sourceOptions.map((p) => (
                          <option key={p} value={p}>
                            {p}
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                  {!!row.differences.length && (
                    <details>
                      <summary>
                        查看文件差异（{row.differences.length}）
                      </summary>
                      {row.differences.map((d) => (
                        <div key={d.path}>
                          <strong>{d.path}</strong>
                          <div className="skill-diff-columns">
                            <pre>{d.central ?? "无文件或二进制文件"}</pre>
                            <pre>{d.target ?? "无文件或二进制文件"}</pre>
                          </div>
                        </div>
                      ))}
                    </details>
                  )}
                </section>
              ))}
            </div>
            <div className="modal-actions">
              <button
                className="button"
                disabled={busy}
                onClick={() => setPreview(null)}
              >
                取消
              </button>
              <button
                className="button primary"
                disabled={busy || !hasWork}
                onClick={() =>
                  void run(async () => {
                    const result = await (activation ? skillsApi.activationApply : skillsApi.batchApply)(
                      preview.id,
                      Object.values(resolutions),
                    );
                    setPreview(null);
                    setNotice(
                      `${result.message}${result.backupPath ? `；备份：${result.backupPath}` : ""}`,
                    );
                    await onChanged();
                  })
                }
              >
                确认应用批量操作
              </button>
            </div>
          </>
        )}
      </Modal>
    </div>
  );
}
