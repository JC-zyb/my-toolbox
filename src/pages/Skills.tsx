import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Copy,
  FolderOpen,
  GitBranch,
  Link2,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Trash2,
} from "lucide-react";
import { api, chooseFile, command, desktop } from "../api";
import { Modal, Panel } from "../components";
import { skillsApi } from "../skills-api";
import { useStore } from "../store";
import type {
  FileDifference,
  MigrationPreview,
  Skill,
  SkillCandidate,
  SkillOperation,
  SkillRepository,
  SkillScan,
  SkillTarget,
  SkillActivationPreview,
  SyncPreview,
} from "../types";
import "./skills.css";
import { SkillMatrix } from "./SkillMatrix";
import { SkillDetail } from "./SkillDetail";
import { SkillOperationToast } from "./SkillOperationToast";
import type { SkillToastItem } from "./SkillOperationToast";
import { useSkillActivation } from "./useSkillActivation";
import { SkillActivationDialog } from "./SkillActivationDialog";

const agents: Record<string, string> = {
  codex: "Codex",
  "codex-legacy": "Codex 兼容目录",
  claude: "Claude Code",
  dsh: "DSH",
};
const states: Record<string, string> = {
  central_only: "仅中央库",
  target_only: "仅目标",
  in_sync: "内容一致",
  central_newer: "中央库已变化",
  target_newer: "目标已变化",
  conflict: "内容冲突",
  error: "读取失败",
  identical: "内容一致",
  source_only: "待迁移",
};
const modes: Record<string, string> = {
  auto: "自动链接（Windows 优先目录联接）",
  symlink: "符号链接",
  junction: "目录联接",
  copy: "复制模式",
  central: "直接发现中央库",
};
export function SkillStatus({ status }: { status: string }) {
  const good = status === "in_sync" || status === "identical";
  const Icon = good
    ? CheckCircle2
    : status === "conflict" || status === "error"
      ? AlertCircle
      : RefreshCw;
  return (
    <span className={`skill-status ${good ? "good" : ""}`}>
      <Icon size={14} aria-hidden="true" />
      {states[status] ?? status}
    </span>
  );
}
function Differences({ differences }: { differences: FileDifference[] }) {
  return (
    <div className="skill-differences">
      {differences.length ? (
        differences.map((d) => (
          <details key={d.path}>
            <summary>
              {d.path} · {states[d.status] ?? "文件变化"}
            </summary>
            <div className="skill-diff-columns">
              <div>
                <strong>中央版本</strong>
                <pre>
                  {d.central ?? "无文件或二进制/大文件；请打开目录查看"}
                </pre>
              </div>
              <div>
                <strong>目标版本</strong>
                <pre>{d.target ?? "无文件或二进制/大文件；请打开目录查看"}</pre>
              </div>
            </div>
          </details>
        ))
      ) : (
        <p className="muted">未发现文件内容差异。</p>
      )}
    </div>
  );
}
export function Skills() {
  const libraryScroll = useRef(0);
  const { settings, save, palette } = useStore();
  const [scan, setScan] = useState<SkillScan | null>(null),
    [targets, setTargets] = useState<SkillTarget[]>([]),
    [repos, setRepos] = useState<SkillRepository[]>([]);
  const [selected, setSelected] = useState<Skill | null>(null),
    [query, setQuery] = useState(""),
    [source, setSource] = useState("all"),
    [agent, setAgent] = useState("all"),
    [status, setStatus] = useState("all"),
    [workspace, setWorkspace] = useState("global");
  const [matrixRefreshKey, setMatrixRefreshKey] = useState(0);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null),
    [notice, setNotice] = useState<string | null>(null);
  const [modal, setModal] = useState<
      | "github"
      | "editor"
      | "migration"
      | "settings"
      | "delete"
      | "remove"
      | null
    >(null),
    [migration, setMigration] = useState<MigrationPreview | null>(null),
    [choices, setChoices] = useState<Record<string, string>>({});
  const [preview, setPreview] = useState<SyncPreview | null>(null),
    [choice, setChoice] = useState("central"),
    [remove, setRemove] = useState<SkillTarget | null>(null);
  const [mode, setMode] = useState(settings.skills.defaultSyncMode),
    [allowCopy, setAllowCopy] = useState(settings.skills.allowCopy ?? false),
    [syncAgent, setSyncAgent] = useState("claude");
  useEffect(() => {
    setMode(settings.skills.defaultSyncMode);
    setAllowCopy(settings.skills.allowCopy ?? false);
  }, [settings.skills.defaultSyncMode, settings.skills.allowCopy]);
  const [editorName, setEditorName] = useState(""),
    [document, setDocument] = useState(""),
    [editing, setEditing] = useState<Skill | null>(null),
    [permanent, setPermanent] = useState(false),
    [deleteName, setDeleteName] = useState("");
  const [url, setUrl] = useState(""),
    [reference, setReference] = useState("HEAD"),
    [repository, setRepository] = useState<SkillRepository | null>(null),
    [candidates, setCandidates] = useState<SkillCandidate[]>([]),
    [paths, setPaths] = useState<string[]>([]);
  const [operations, setOperations] = useState<SkillOperation[]>([]);
  const [toasts, setToasts] = useState<SkillToastItem[]>([]);
  const dismissToast = useCallback((id: string) => {
    setToasts((items) => items.filter((item) => item.id !== id));
  }, []);
  const notify = useCallback((item: Omit<SkillToastItem, "id">) => {
    const id = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;
    setToasts((items) => [...items, { ...item, id }]);
    if (item.kind === "success") {
      window.setTimeout(() => dismissToast(id), 3000);
    }
    return id;
  }, [dismissToast]);
  const updateToast = useCallback((id: string, patch: Partial<SkillToastItem>) => {
    setToasts((items) => items.map((item) => item.id === id ? { ...item, ...patch } : item));
    if (patch.kind === "success") window.setTimeout(() => dismissToast(id), 3000);
  }, [dismissToast]);
  const [activationPreview, setActivationPreview] = useState<SkillActivationPreview | null>(null);
  const activation = useSkillActivation(workspace === "global" ? null : workspace, allowCopy, notify, updateToast, setActivationPreview);
  const activationRef = useRef(activation);
  activationRef.current = activation;
  useEffect(() => {
    if (palette) {
      setSelected(null);
      setPreview(null);
      setModal(null);
    }
  }, [palette]);
  const run = useCallback(async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);
  const load = useCallback(async () => {
    const result = await skillsApi.scan();
    activationRef.current.clear();
    setScan(result);
    setMatrixRefreshKey((value) => value + 1);
    setRepos(await skillsApi.repositories());
    useStore.setState({
      skillNames: result.skills.map((s) => ({ id: s.id, name: s.name })),
    });
  }, []);
  const detail = useCallback(async (id: string) => {
    const s = await skillsApi.discovered(id, workspace === "global" ? null : workspace);
    const t = scan?.skills.some(skill => skill.id === id) ? await skillsApi.targets(id) : [];
    if (!useStore.getState().selectedSkillId) libraryScroll.current = window.scrollY;
    setSelected(s);
    setTargets(t);
    useStore.setState({ selectedSkillId: id });
    window.scrollTo(0, 0);
  }, [workspace, scan]);
  useEffect(() => {
    if (!desktop) return;
    void run(async () => {
      await load();
      if (settings.skills.migration.status === "pending") {
        const p = await skillsApi.migration();
        setMigration(p);
        if (p.rows.length || p.diagnostics.length) setModal("migration");
      }
    });
  }, [load, run]);
  useEffect(() => {
    const handler = (e: Event) => {
      const request = (e as CustomEvent<{ action: string; id?: string }>)
        .detail;
      if (request.action === "github") {
        setRepository(null);
        setCandidates([]);
        setModal("github");
      }
      if (request.action === "project") {
        setModal("settings");
      }
      if (request.action === "detail" && request.id)
        void run(() => detail(request.id!));
      if (request.action === "sync") {
        const id = useStore.getState().selectedSkillId;
        if (id)
          void run(async () => {
            await detail(id);
            setChoice("central");
            setPreview(
              await skillsApi.preview(
                id,
                syncAgent,
                workspace === "global" ? null : workspace,
                mode,
                allowCopy,
              ),
            );
          });
      }
      if (request.action === "refresh") void run(load);
    };
    window.addEventListener("skills-action", handler);
    return () => window.removeEventListener("skills-action", handler);
  }, [detail, run, load, syncAgent, workspace, mode, allowCopy]);
  useEffect(() => {
    if (!desktop || !settings.skills.autoCheckUpdates) return;
    let cancelled = false;
    const check = () => {
      void skillsApi
        .repositories(true)
        .then((r) => {
          if (!cancelled) setRepos(r);
        })
        .catch((e) => {
          if (!cancelled) setError(String(e));
        });
    };
    check();
    const timer = setInterval(check, 3600000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [settings.skills.autoCheckUpdates]);
  const reloadDetail = async () => {
    await load();
    if (selected) await detail(selected.id);
  };
  const openGit = () => {
    setRepository(null);
    setCandidates([]);
    setPaths([]);
    setModal("github");
  };
  return (
    <div className="page-flow skills-page" aria-busy={busy}>
      <div hidden={!!selected}>
      <div className="page-intro">
        <div>
          <span className="eyebrow">一个内容库，多个工作空间</span>
          <h1>
            Skills<span className="heading-dot">.</span>
          </h1>
          <p>浏览技能，打开详情即可启用或关闭 Agent。</p>
        </div>
        <div className="button-row">
          <button
            className="button"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const p = await chooseFile(true);
                if (p) {
                  await command("import_local_skill", { path: p[0] });
                  await load();
                  setNotice("本地 Skill 已导入");
                  notify({ kind: "success", title: "Skill 已导入", message: "本地 Skill 已导入中央库" });
                }
              })
            }
          >
            <FolderOpen size={16} />
            导入本地
          </button>
          <button className="button" disabled={busy} onClick={openGit}>
            <GitBranch size={16} />
            导入 GitHub
          </button>
          <button
            className="button primary"
            disabled={busy}
            onClick={() => {
              setEditing(null);
              setEditorName("");
              setDocument(
                "---\nname: my-skill\ndescription: 描述这个 Skill 的用途\n---\n\n# 使用说明\n",
              );
              setModal("editor");
            }}
          >
            <Plus size={16} />
            新建 Skill
          </button>
        </div>
      </div>
      {error && (
        <div role="alert" className="notice error">
          <AlertCircle size={16} />
          <span>{error}</span>
        </div>
      )}
      {notice && (
        <div role="status" className="notice">
          <CheckCircle2 size={16} />
          <span>{notice}</span>
        </div>
      )}
      {busy && (
        <p role="status" className="loading-banner">
          正在处理 Skills，原数据保留…
        </p>
      )}
      <div className="skill-library-bar">
        <div>
          <span className="muted">中央库</span>
          <code>{scan?.centralPath ?? "桌面连接后显示绝对路径"}</code>
        </div>
        <button
          className="icon-button"
          aria-label="复制中央库路径"
          disabled={!scan}
          onClick={() =>
            void run(() => navigator.clipboard.writeText(scan!.centralPath))
          }
        >
          <Copy size={16} />
        </button>
        <button
          className="button"
          disabled={busy}
          onClick={() => void run(load)}
        >
          <RefreshCw size={16} />
          扫描
        </button>
        <button
          className="icon-button"
          aria-label="公共仓库设置"
          onClick={() =>
            void run(async () => {
              setOperations(desktop ? await skillsApi.operations() : []);
              setModal("settings");
            })
          }
        >
          <Settings2 size={18} />
        </button>
      </div>
      {scan?.diagnostics.map((d, i) => (
        <p className="inline-warning" key={i}>
          {d}
        </p>
      ))}
      <div className="skills-layout">
        <aside className="skill-filters" aria-label="我的 Skills">
          <strong>我的 Skills</strong>
          {[
            ["all", "全部"],
            ["local", "本地"],
            ["github", "GitHub"],
          ].map(([id, label]) => (
            <button
              key={id}
              aria-pressed={source === id}
              className={source === id ? "active" : ""}
              onClick={() => setSource(id)}
            >
              {label}
              <span>
                {id === "all"
                  ? (scan?.skills.length ?? 0)
                  : (scan?.skills.filter((s) => s.sourceType === id).length ??
                    0)}
              </span>
            </button>
          ))}
          <label>
            工作空间
            <select
              value={workspace}
              onChange={(e) => {
                setWorkspace(e.target.value);
              }}
            >
              <option value="global">全局</option>
              {settings.skills.projects.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </label>
          <button onClick={() => setModal("settings")}>
            <FolderOpen size={16} />
            管理项目
          </button>
        </aside>
        <Panel
          title="技能库"
          subtitle={`中央库与 Agent 安装 · ${workspace === "global" ? "全局工作空间" : "项目工作空间"}`}
          action={
            <button
              className="text-button"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  setRepos(await skillsApi.repositories(true));
                  setNotice("更新检查完成，详情见公共仓库设置");
                })
              }
            >
              检查更新
            </button>
          }
        >
          <div className="skill-search">
            <label>
              <Search size={16} />
              <input
                aria-label="搜索 Skill"
                placeholder="搜索名称或描述"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
            <select
              aria-label="筛选 Skill Agent"
              value={agent}
              onChange={(e) => setAgent(e.target.value)}
            >
              <option value="all">全部 Agent</option>
              {Object.entries(agents)
                .filter(([id]) => id !== "codex-legacy")
                .map(([id, label]) => (
                  <option key={id} value={id}>
                    {label}
                  </option>
                ))}
            </select>
            <select
              aria-label="筛选 Skill 状态"
              value={status}
              onChange={(e) => setStatus(e.target.value)}
            >
              <option value="all">全部状态</option>
              <option value="conflict">有冲突</option>
              <option value="duplicate">重复入口</option>
              <option value="target_only">Agent 独有 / 待导入</option>
              <option value="missing">未安装</option>
              <option value="error">读取失败</option>
              <option value="target_newer">目标已变化</option>
              <option value="updates">有更新</option>
            </select>
          </div>
          <SkillMatrix
            activationQueue={activation}
            skills={scan?.skills ?? []}
            refreshKey={matrixRefreshKey}
            workspace={workspace}
            query={query}
            source={source}
            agent={agent}
            status={status}
            repos={repos}
            allowCopy={settings.skills.allowCopy ?? false}
            onDetail={(id) => void run(() => detail(id))}
            onChanged={load}
            notify={notify}
            onAction={(action, id) => {
              const current = scan?.skills.find((skill) => skill.id === id);
              if (!current) return;
              if (action === "edit") {
                setEditing(current);
                setEditorName(current.name);
                setDocument(current.document);
                setModal("editor");
              } else if (action === "delete") {
                setSelected(current);
                setPermanent(false);
                setDeleteName("");
                setModal("delete");
              } else {
                void run(async () => {
                  const destination = await chooseFile(true);
                  if (!destination) return;
                  const result = await command<{ message: string }>("export_skill", { id, destination: destination[0] });
                  notify({ kind: "success", title: "Skill 已导出", message: result.message });
                });
              }
            }}
          />
        </Panel>
      </div>
      </div>
        {selected && <SkillDetail skill={selected} workspace={workspace} allowCopy={allowCopy} activationQueue={activation} onBack={() => { setSelected(null); useStore.setState({ selectedSkillId: null }); requestAnimationFrame(() => window.scrollTo(0, libraryScroll.current)); }} actions={<>
          <button className="button" disabled={busy || !scan?.skills.some(s => s.id === selected.id)} onClick={() => { setEditing(selected); setEditorName(selected.name); setDocument(selected.document); setModal("editor"); }}>编辑</button>
          <button className="button" disabled={busy || !scan?.skills.some(s => s.id === selected.id)} onClick={() => void run(async () => { const p = await chooseFile(true); if (p) { const r = await command<{ message: string }>("export_skill", { id: selected.id, destination: p[0] }); setNotice(r.message); } })}>导出</button>
          <button className="button danger" disabled={busy || !scan?.skills.some(s => s.id === selected.id)} onClick={() => { setPermanent(false); setDeleteName(""); setModal("delete"); }}>删除</button>
        </>}>
            {error && <p role="alert" className="inline-warning">{error}</p>}
            {notice && <p role="status">{notice}</p>}
            <h3>兼容分发入口</h3>
            {targets.map((t) => (
              <div className="skill-target" key={t.id}>
                <div className="grow">
                  <strong>
                    {agents[t.agentId]} ·{" "}
                    {t.scope === "global" ? "全局" : t.projectPath}
                  </strong>
                  <small>{t.targetPath}</small>
                  <span>
                    {modes[t.mode]} · {t.managed ? "工具管理" : "已有或未分发"}
                  </span>
                  {t.error && <p className="danger-text">{t.error}</p>}
                </div>
                <SkillStatus status={t.status} />
                {t.managed && (
                  <button
                    className="icon-button"
                    disabled={busy}
                    aria-label={`解除 ${agents[t.agentId]} 分发`}
                    onClick={() => {
                      setRemove(t);
                      setModal("remove");
                    }}
                  >
                    <Trash2 size={16} />
                  </button>
                )}
              </div>
            ))}
            <div className="skill-sync-form">
              <label htmlFor="skill-agent">
                Agent
                <select
                  id="skill-agent"
                  value={syncAgent}
                  onChange={(e) => setSyncAgent(e.target.value)}
                >
                  {Object.entries(agents)
                    .filter(([id]) => id !== "codex-legacy")
                    .map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))}
                </select>
              </label>
              <label htmlFor="skill-workspace">
                工作空间
                <select
                  id="skill-workspace"
                  value={workspace}
                  onChange={(e) => setWorkspace(e.target.value)}
                >
                  <option value="global">全局</option>
                  {settings.skills.projects.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
              </label>
              <label htmlFor="skill-mode">
                同步模式
                <select
                  id="skill-mode"
                  value={mode}
                  onChange={(e) => setMode(e.target.value)}
                >
                  {["auto", "symlink", "junction", "copy"].map((m) => (
                    <option key={m} value={m}>
                      {modes[m]}
                    </option>
                  ))}
                </select>
              </label>
              <label className="skill-checkbox">
                <input
                  type="checkbox"
                  checked={allowCopy}
                  onChange={(e) => setAllowCopy(e.target.checked)}
                />
                链接失败时允许复制
              </label>
              <button
                className="button primary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    setChoice("central");
                    setPreview(
                      await skillsApi.preview(
                        selected.id,
                        syncAgent,
                        workspace === "global" ? null : workspace,
                        mode,
                        allowCopy,
                      ),
                    );
                  })
                }
              >
                <Link2 size={16} />
                预览分发
              </button>
            </div>
        </SkillDetail>}
      <SkillOperationToast items={toasts} onDismiss={dismissToast} />
      <SkillActivationDialog preview={activationPreview} onClose={() => setActivationPreview(null)} notify={notify} onApplied={async () => { activation.clear(); await load(); if (selected) await detail(selected.id); }} />
      <Modal
        error={error}
        open={!!preview}
        onClose={() => setPreview(null)}
        title="分发预览"
        description="核对影响范围后执行"
        wide
      >
        {preview && (
          <>
            <dl className="detail-grid">
              <div>
                <dt>中央版本</dt>
                <dd>{preview.sourcePath}</dd>
              </div>
              <div>
                <dt>目标</dt>
                <dd>{preview.targetPath}</dd>
              </div>
              <div>
                <dt>模式</dt>
                <dd>{modes[preview.mode]}</dd>
              </div>
              <div>
                <dt>状态</dt>
                <dd>
                  <SkillStatus status={preview.status} />
                </dd>
              </div>
            </dl>
            {preview.conflict && (
              <p className="inline-warning">
                目标是外部内容或已修改的副本。替换前将保留备份，请逐项查看文件差异。
              </p>
            )}
            <Differences differences={preview.differences} />
            {preview.conflict && (
              <label>
                冲突处理
                <select
                  value={choice}
                  onChange={(e) => setChoice(e.target.value)}
                >
                  <option value="central">保留中央库</option>
                  <option value="target">保留目标并更新中央库</option>
                  <option value="skip">跳过该 Skill</option>
                </select>
              </label>
            )}
            <div className="modal-actions">
              <button className="button" onClick={() => setPreview(null)}>
                取消
              </button>
              <button
                className="button primary"
                disabled={busy}
                onClick={() => {
                  if (choice === "skip") {
                    setPreview(null);
                    return;
                  }
                  void run(async () => {
                    const t = await skillsApi.sync(preview.id, choice);
                    setNotice(`已完成分发 · ${modes[t.mode]}`);
                    notify({ kind: "success", title: "Skill 已分发", message: `已完成分发 · ${modes[t.mode]}` });
                    await reloadDetail();
                    setPreview(null);
                  });
                }}
              >
                确认分发
              </button>
            </div>
          </>
        )}
      </Modal>
      <Modal
        error={error}
        open={modal === "editor"}
        onClose={() => setModal(null)}
        title={editing ? "编辑 Skill" : "新建 Skill"}
        description="名称与 frontmatter name 保持一致"
        wide
      >
        <form
          className="form-stack"
          onSubmit={(e) => {
            e.preventDefault();
            void run(async () => {
              if (editing)
                await command("update_skill", {
                  id: editing.id,
                  document,
                  expectedHash: editing.contentHash,
                });
              else
                await command("create_skill", { name: editorName, document });
              setModal(null);
              await load();
              if (editing) await detail(editing.id);
              setNotice("Skill 已保存");
              notify({ kind: "success", title: "Skill 已保存", message: "中央库内容已更新" });
            });
          }}
        >
          <label htmlFor="skill-name">
            名称
            <input
              id="skill-name"
              required
              disabled={!!editing}
              value={editorName}
              onChange={(e) => setEditorName(e.target.value)}
            />
          </label>
          <label htmlFor="skill-document">
            SKILL.md
            <textarea
              id="skill-document"
              className="skill-editor"
              required
              value={document}
              onChange={(e) => setDocument(e.target.value)}
            />
          </label>
          <button className="button primary" disabled={busy}>
            保存 Skill
          </button>
        </form>
      </Modal>
      <Modal
        error={error}
        open={modal === "github"}
        onClose={() => setModal(null)}
        title="导入 GitHub"
        description="浏览仓库中的 Skills，选择后导入中央库"
        wide
      >
        <form
          className="form-stack"
          onSubmit={(e) => {
            e.preventDefault();
            void run(async () => {
              const r = await skillsApi.checkout(url, reference);
              setRepository(r.repository);
              setCandidates(r.candidates);
              setPaths([]);
            });
          }}
        >
          <label>
            GitHub 仓库 URL
            <input
              type="url"
              required
              placeholder="https://github.com/owner/repo"
              value={url}
              onChange={(e) => {
                setUrl(e.target.value);
                setRepository(null);
                setCandidates([]);
              }}
            />
          </label>
          <label>
            branch / tag / commit
            <input
              required
              value={reference}
              onChange={(e) => {
                setReference(e.target.value);
                setRepository(null);
                setCandidates([]);
              }}
            />
          </label>
          <button className="button" disabled={busy}>
            浏览仓库
          </button>
        </form>
        {repository && (
          <div className="skill-candidates">
            <p className="mono">commit {repository.revision}</p>
            {candidates.map((c) => (
              <label key={c.path} className="skill-candidate">
                <input
                  type="checkbox"
                  checked={paths.includes(c.path)}
                  onChange={(e) =>
                    setPaths(
                      e.target.checked
                        ? [...paths, c.path]
                        : paths.filter((p) => p !== c.path),
                    )
                  }
                />
                <span>
                  <strong>{c.name}</strong>
                  <small>
                    {c.path || "仓库根目录"} · {c.description}
                  </small>
                </span>
              </label>
            ))}
            {!candidates.length && <p>仓库中没有有效的 Skill。</p>}
            <button
              className="button primary"
              disabled={busy || !paths.length}
              onClick={() =>
                void run(async () => {
                  const r = await skillsApi.importRepository(
                    repository.id,
                    paths,
                  );
                  await save({
                    ...settings,
                    skills: {
                      ...settings.skills,
                      repositories: [
                        ...new Set([
                          ...settings.skills.repositories,
                          repository.url,
                        ]),
                      ],
                    },
                  });
                  setModal(null);
                  await load();
                  setNotice(`已导入 ${r.skills.length} 个 Skill`);
                  notify({ kind: "success", title: "Skill 已导入", message: `已导入 ${r.skills.length} 个 Skill` });
                })
              }
            >
              导入所选 {paths.length} 个 Skill
            </button>
          </div>
        )}
      </Modal>
      <Modal
        error={error}
        open={modal === "migration"}
        onClose={() => setModal(null)}
        title="迁移现有公共 Skills"
        description="先比较，再备份并迁移；迁移源保留"
        wide
      >
        {migration && (
          <>
            <p className="mono">
              {migration.sourcePath} → {migration.centralPath}
            </p>
            {!!migration.directoryLinks?.length && (
              <div className="inline-warning">
                <p>
                  以下整个 skills
                  目录指向旧库。确认后将备份目录入口，改为普通目录和单技能入口；需处理全部迁移项。Codex
                  兼容入口清理后，中央库仍直接可用。
                </p>
                {migration.directoryLinks.map((path) => (
                  <p className="mono" key={path}>
                    {path}
                  </p>
                ))}
              </div>
            )}
            <p>
              发现 {migration.rows.length} 个 Skill ·{" "}
              {migration.rows.reduce((n, r) => n + r.links.length, 0)} 个 Agent
              联接 · 内容一致{" "}
              {migration.rows.filter((r) => r.status === "identical").length} 个
              · 冲突{" "}
              {migration.rows.filter((r) => r.status === "conflict").length} 个
            </p>
            {migration.diagnostics.map((d, i) => (
              <p className="inline-warning" key={i}>
                {d}
              </p>
            ))}
            {migration.rows.map((r) => (
              <details key={r.name}>
                <summary>
                  {r.name} · {states[r.status]} · {r.links.length} 个联接
                </summary>
                <p>{r.sourcePath}</p>
                <p>{r.targetPath}</p>
                <Differences differences={r.differences} />
                <label>
                  迁移选择
                  <select
                    value={
                      choices[r.name] ??
                      (r.status === "source_only" ? "target" : "central")
                    }
                    onChange={(e) =>
                      setChoices({ ...choices, [r.name]: e.target.value })
                    }
                  >
                    {r.status === "conflict" && (
                      <option value="">请选择</option>
                    )}
                    <option value="target">保留迁移来源</option>
                    <option value="central">保留中央库</option>
                    <option value="skip">跳过该 Skill</option>
                  </select>
                </label>
              </details>
            ))}
            <div className="modal-actions">
              <button
                className="button"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await save({
                      ...settings,
                      skills: {
                        ...settings.skills,
                        migration: {
                          ...settings.skills.migration,
                          status: "skipped",
                        },
                      },
                    });
                    setModal(null);
                  })
                }
              >
                暂不迁移
              </button>
              <button
                className="button primary"
                disabled={
                  busy ||
                  !migration.rows.length ||
                  !!migration.diagnostics.length
                }
                onClick={() =>
                  void run(async () => {
                    const result = await skillsApi.migrate(
                      migration.id,
                      migration.rows.map((r) => ({
                        name: r.name,
                        choice:
                          choices[r.name] ??
                          (r.status === "source_only" ? "target" : "central"),
                      })),
                    );
                    useStore.setState({ settings: await api.settings() });
                    setModal(null);
                    await load();
                    setNotice(`${result.message} · 备份：${result.backupPath}`);
                    notify({ kind: "success", title: "Skills 迁移完成", message: `${result.message} · 备份：${result.backupPath}` });
                  })
                }
              >
                备份并迁移
              </button>
            </div>
          </>
        )}
      </Modal>
      <Modal
        error={error}
        open={modal === "remove"}
        onClose={() => setModal(null)}
        title="确认解除分发"
        description="只移除本工具管理且未被外部改动的目标"
      >
        {remove && (
          <>
            <p>
              {agents[remove.agentId]} · {remove.projectPath ?? "全局"}
            </p>
            <p className="mono">{remove.targetPath}</p>
            <p>原目标保留备份；中央内容保留。</p>
            <button
              className="button danger"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const r = await skillsApi.remove(remove.id);
                  await reloadDetail();
                  setModal(null);
                  setNotice(r.message);
                  notify({ kind: "success", title: "入口已解除", message: r.message });
                })
              }
            >
              确认解除分发
            </button>
          </>
        )}
      </Modal>
      <Modal
        error={error}
        open={modal === "delete"}
        onClose={() => setModal(null)}
        title="删除 Skill / 解除分发"
        description="默认只解除已管理的分发，永久删除需要再次输入名称"
      >
        {selected && (
          <>
            <p>影响 {targets.filter((t) => t.managed).length} 个受管理目标：</p>
            {targets
              .filter((t) => t.managed)
              .map((t) => (
                <p className="mono" key={t.id}>
                  {t.targetPath}
                </p>
              ))}
            <label className="skill-checkbox">
              <input
                type="checkbox"
                checked={permanent}
                onChange={(e) => setPermanent(e.target.checked)}
              />
              同时将中央内容移出库（备份保留）
            </label>
            {permanent && (
              <label>
                再次确认，输入 {selected.name}
                <input
                  value={deleteName}
                  onChange={(e) => setDeleteName(e.target.value)}
                />
              </label>
            )}
            <div className="modal-actions">
              <button
                className="button danger"
                disabled={busy || (permanent && deleteName !== selected.name)}
                onClick={() =>
                  void run(async () => {
                    const r = await command<{
                      message: string;
                      backupPath: string | null;
                    }>("delete_skill", {
                      id: selected.id,
                      permanent,
                      expectedHash: selected.contentHash,
                    });
                    setModal(null);
                    setNotice(
                      `${r.message}${r.backupPath ? ` · ${r.backupPath}` : ""}`,
                    );
                    notify({ kind: "success", title: permanent ? "Skill 已删除" : "分发已解除", message: `${r.message}${r.backupPath ? ` · ${r.backupPath}` : ""}` });
                    if (permanent) setSelected(null);
                    await load();
                    if (!permanent) await detail(selected.id);
                  })
                }
              >
                {permanent ? "确认删除中央内容" : "确认解除全部分发"}
              </button>
            </div>
          </>
        )}
      </Modal>
      <Modal
        error={error}
        open={modal === "settings"}
        onClose={() => setModal(null)}
        title="公共仓库设置"
        description="中央库、来源仓库、项目与迁移记录"
        wide
      >
        <div className="form-stack">
          <label>
            中央库（固定）
            <input readOnly value={scan?.centralPath ?? "桌面连接后显示"} />
          </label>
          <label>
            高级：详情页默认同步模式（批量使用自动链接）
            <select
              value={settings.skills.defaultSyncMode}
              onChange={(e) =>
                void run(() =>
                  save({
                    ...settings,
                    skills: {
                      ...settings.skills,
                      defaultSyncMode: e.target.value,
                    },
                  }),
                )
              }
            >
              {["auto", "symlink", "junction", "copy"].map((m) => (
                <option key={m} value={m}>
                  {modes[m]}
                </option>
              ))}
            </select>
          </label>
          <label className="skill-checkbox">
            <input
              type="checkbox"
              checked={settings.skills.allowCopy ?? false}
              onChange={(e) =>
                void run(() =>
                  save({
                    ...settings,
                    skills: { ...settings.skills, allowCopy: e.target.checked },
                  }),
                )
              }
            />
            高级：允许自动链接失败后复制（默认关闭）
          </label>
          <label className="skill-checkbox">
            <input
              type="checkbox"
              checked={settings.skills.autoCheckUpdates}
              onChange={(e) =>
                void run(() =>
                  save({
                    ...settings,
                    skills: {
                      ...settings.skills,
                      autoCheckUpdates: e.target.checked,
                    },
                  }),
                )
              }
            />
            Skills 页面打开时自动检查更新（每小时）
          </label>
          <h3>项目工作区</h3>
          {settings.skills.projects.map((p) => (
            <div className="skill-target" key={p}>
              <code className="grow">{p}</code>
              <button
                className="button"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await save({
                      ...settings,
                      skills: {
                        ...settings.skills,
                        projects: settings.skills.projects.filter(
                          (x) => x !== p,
                        ),
                      },
                    });
                    if (workspace === p) {
                      setWorkspace("global");
                    }
                    await load();
                  })
                }
              >
                移除登记
              </button>
            </div>
          ))}
          <button
            className="button"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const p = await chooseFile(true);
                if (p) {
                  await skillsApi.project(p[0]);
                  await save({
                    ...settings,
                    skills: {
                      ...settings.skills,
                      projects: [
                        ...new Set([...settings.skills.projects, p[0]]),
                      ],
                    },
                  });
                  await load();
                }
              })
            }
          >
            <Plus size={16} />
            添加项目
          </button>
          <h3>GitHub 仓库</h3>
          {repos.length ? (
            repos.map((r) => (
              <div className="skill-target" key={r.id}>
                <div>
                  <strong>
                    {r.url} · {r.reference}
                  </strong>
                  <small>
                    {r.updateAvailable
                      ? "远程有更新，中央库未覆盖"
                      : "最后检查：" + r.lastCheckedAt}
                  </small>
                  <small>checkout：{r.checkoutPath}</small>
                  {r.error && <p className="danger-text">{r.error}</p>}
                </div>
              </div>
            ))
          ) : (
            <p className="muted">尚未导入 GitHub 仓库。</p>
          )}
          <button
            className="button"
            disabled={busy}
            onClick={() =>
              void run(async () => setRepos(await skillsApi.repositories(true)))
            }
          >
            检查远程更新
          </button>
          <h3>迁移与备份</h3>
          <p>
            状态：{settings.skills.migration.status} · 备份：
            {settings.skills.migration.backupPath ?? "尚无迁移备份"}
          </p>
          <button
            className="button"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                setMigration(await skillsApi.migration());
                setChoices({});
                setModal("migration");
              })
            }
          >
            重新预览迁移
          </button>
          <h3>最近操作</h3>
          {operations.map((o, i) => (
            <div key={i}>
              <strong>
                {o.operation} · {o.at}
              </strong>
              <p className="mono">{o.path}</p>
              {o.backupPath && <p className="mono">备份：{o.backupPath}</p>}
            </div>
          ))}
        </div>
      </Modal>
    </div>
  );
}
