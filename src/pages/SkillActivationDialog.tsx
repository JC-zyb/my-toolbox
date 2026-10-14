import { useEffect, useState } from "react";
import { Modal } from "../components";
import { skillsApi } from "../skills-api";
import type { BatchResolution, SkillActivationPreview } from "../types";
import { skillAgents } from "./SkillAgentIcon";
import type { SkillToastItem } from "./SkillOperationToast";

export function SkillActivationDialog({ preview, onClose, onApplied, notify }: {
  preview: SkillActivationPreview | null;
  onClose: () => void;
  onApplied: () => Promise<void>;
  notify: (item: Omit<SkillToastItem, "id">) => string;
}) {
  const [choices, setChoices] = useState<Record<string, BatchResolution>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setChoices(Object.fromEntries((preview?.rows ?? []).map(row => [row.id, { id: row.id, choice: row.defaultChoice, sourcePath: row.sourcePath }])));
    setError(null);
  }, [preview]);
  const apply = async () => {
    if (!preview) return;
    setBusy(true); setError(null);
    try {
      const result = await skillsApi.activationApply(preview.id, Object.values(choices));
      notify({ kind: "success", title: "启停更改已保存", message: `${result.message}${result.backupPath ? ` · 备份：${result.backupPath}` : ""}` });
      await onApplied();
      onClose();
    } catch (e) {
      setError(String(e));
      notify({ kind: "error", title: "启停操作失败", message: String(e) });
    } finally { setBusy(false); }
  };
  return <Modal open={!!preview} error={error} onClose={() => { if (!busy) onClose(); }} title="启停确认" description="外部内容、版本冲突或首次导入需要确认；原内容保留备份" wide>
    {preview?.rows.map(row => <section className="skill-source-detail" key={row.id}>
      <h3>{row.skillId} · {row.agentId === "central" ? "导入中央库" : skillAgents[row.agentId as keyof typeof skillAgents]}</h3>
      <p>{row.action === "config" ? "更新 Codex 配置，重启后生效" : row.action === "remove" ? "关闭并备份当前安装入口" : row.action === "import" ? "首次导入中央库" : "启用安装入口"} · {row.mode}{row.modified && " · 本地已修改"}</p>
      <p className="mono">来源：{choices[row.id]?.sourcePath}</p><p className="mono">目标：{row.targetPath}</p>
      <p className="mono">指纹：{preview.targetFingerprints[row.targetPath]}</p>
      {row.defaultChoice !== "noop" && <label>处理方式<select aria-label={`处理 ${row.id}`} disabled={busy} value={choices[row.id]?.choice ?? row.defaultChoice} onChange={e => setChoices(current => ({ ...current, [row.id]: { ...current[row.id], choice: e.target.value } }))}>
        <option value="skip">保留已有版本并跳过</option><option value="central">{row.action === "remove" ? "确认备份并关闭" : row.action === "import" ? "确认导入来源" : "使用中央版本"}</option>
        {row.conflict && !["config", "import"].includes(row.action) && <option value="target">保留目标并导入中央</option>}
      </select></label>}
      {row.action === "import" && <label>导入来源<select aria-label={`导入来源 ${row.skillId}`} disabled={busy} value={choices[row.id]?.sourcePath ?? row.sourcePath} onChange={e => setChoices(current => ({ ...current, [row.id]: { ...current[row.id], sourcePath: e.target.value } }))}>{row.sourceOptions.map(path => <option key={path}>{path}</option>)}</select></label>}
    </section>)}
    <div className="modal-actions"><button className="button" disabled={busy} onClick={onClose}>取消</button><button className="button primary" disabled={busy || !preview} onClick={() => void apply()}>{busy ? "正在应用…" : "确认应用启停"}</button></div>
  </Modal>;
}
