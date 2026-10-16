import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowUpRight,
  Check,
  Download,
  FolderOpen,
  Plus,
  RefreshCw,
  Search,
  Terminal,
} from "lucide-react";
import { api, chooseFile, desktop } from "../api";
import { Empty, Modal, Panel } from "../components";
import { dateTime } from "../stats";
import {
  localToolsApi,
  toolActionText,
  toolStatusText,
} from "../local-tools-api";
import type {
  ProjectToolConfig,
  ToolAction,
  ToolCandidate,
  ToolDefinition,
  ToolInstallation,
  ToolOperation,
  ToolOperationPlan,
  ToolSnapshot,
  ToolVersions,
} from "../local-tools-api";
import "./local-tools.css";

type Tab = "installed" | "catalog" | "updates" | "operations";
type DetailTab = "overview" | "versions" | "project";

export function LocalTools() {
  const [snapshot, setSnapshot] = useState<ToolSnapshot | null>(null);
  const [operations, setOperations] = useState<ToolOperation[]>([]);
  const [tab, setTab] = useState<Tab>("installed");
  const [detail, setDetail] = useState<string | null>(null);
  const [detailTab, setDetailTab] = useState<DetailTab>("overview");
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("all");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [candidates, setCandidates] = useState<ToolCandidate[]>([]);
  const [searchDiagnostics, setSearchDiagnostics] = useState<string[]>([]);
  const [versions, setVersions] = useState<ToolVersions | null>(null);
  const [version, setVersion] = useState("");
  const [provider, setProvider] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [project, setProject] = useState<ProjectToolConfig | null>(null);
  const [plan, setPlan] = useState<ToolOperationPlan | null>(null);
  const [selectedOperation, setSelectedOperation] = useState<string | null>(
    null,
  );
  const [directoriesOpen, setDirectoriesOpen] = useState(false);
  const [updateDiagnostics, setUpdateDiagnostics] = useState<string[]>([]);
  const mounted = useRef(true);
  const busyRef = useRef(false);
  const detailRef = useRef(detail);
  detailRef.current = detail;
  const operationsRef = useRef(operations);
  operationsRef.current = operations;

  const run = useCallback(async (label: string, task: () => Promise<void>) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(label);
    setError(null);
    try {
      await task();
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      busyRef.current = false;
      if (mounted.current) setBusy(null);
    }
  }, []);
  const refresh = useCallback(
    () =>
      run("扫描本机", async () => {
        const next = await localToolsApi.scan();
        if (mounted.current) setSnapshot(next);
      }),
    [run],
  );
  useEffect(() => {
    mounted.current = true;
    if (desktop)
      void run("读取工具", async () => {
        const cached = await localToolsApi.list();
        if (!mounted.current) return;
        setSnapshot(cached);
        setOperations(await localToolsApi.operations());
        if (!cached.scannedAt) {
          const next = await localToolsApi.scan();
          if (mounted.current) setSnapshot(next);
        }
      });
    const onAction = () => {
      void refresh();
    };
    window.addEventListener("local-tools-refresh", onAction);
    return () => {
      mounted.current = false;
      window.removeEventListener("local-tools-refresh", onAction);
    };
  }, [run, refresh]);
  useEffect(() => {
    if (!desktop) return;
    let live = true,
      polling = false;
    const timer = setInterval(async () => {
      if (polling) return;
      polling = true;
      try {
        const next = await localToolsApi.operations();
        if (!live) return;
        setOperations(next);
        const changed = next.some(
          (o) =>
            !["queued", "running"].includes(o.status) &&
            operationsRef.current.some(
              (old) =>
                old.id === o.id && ["queued", "running"].includes(old.status),
            ),
        );
        if (changed) {
          const data = await localToolsApi.list();
          if (live) setSnapshot(data);
          if (detailRef.current) {
            const tool = data.definitions.find(
              (t) => t.id === detailRef.current,
            );
            if (tool?.runtimeProvider) {
              const values = await localToolsApi.versions(tool.id);
              if (live && detailRef.current === tool.id) setVersions(values);
            }
          }
        }
      } catch (e) {
        if (live) setError(`读取操作状态失败：${String(e)}`);
      } finally {
        polling = false;
      }
    }, 1500);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);

  const tool = snapshot?.definitions.find((t) => t.id === detail) ?? null;
  const installations =
    snapshot?.installations.filter((i) => i.toolId === detail) ?? [];
  const metadata = snapshot?.metadata.find((m) => m.toolId === detail);
  const currentOperation = operations.find((o) => o.id === selectedOperation);
  const availableProviders = tool
    ? [
        ...(tool.runtimeProvider ? [tool.runtimeProvider] : []),
        ...tool.packages.map((p) => p.provider),
      ].filter(
        (id, index, all) =>
          all.indexOf(id) === index &&
          snapshot?.providers.some((p) => p.id === id && p.available),
      )
    : [];

  function selectTool(value: ToolDefinition) {
    setDetail(value.id);
    setDetailTab("overview");
    setVersions(null);
    setVersion(
      snapshot?.updates.find((u) => u.toolId === value.id)?.targetVersion || "",
    );
    setProject(null);
    setProjectPath("");
    setError(null);
    setProvider(
      value.runtimeProvider &&
        snapshot?.providers.some(
          (p) => p.id === value.runtimeProvider && p.available,
        )
        ? value.runtimeProvider
        : (value.packages.find((p) =>
            snapshot?.providers.some((s) => s.id === p.provider && s.available),
          )?.provider ?? ""),
    );
    const known = snapshot?.metadata.find((m) => m.toolId === value.id);
    if (
      !known?.fetchedAt ||
      Date.now() - new Date(known.fetchedAt).getTime() > 86400000
    ) {
      void run("补全工具资料", async () => {
        const next = await localToolsApi.metadata(value.id);
        if (!mounted.current) return;
        setSnapshot(
          (old) =>
            old && {
              ...old,
              metadata: [
                ...old.metadata.filter((m) => m.toolId !== value.id),
                next,
              ],
            },
        );
      });
    }
  }
  function loadVersions(remote: boolean) {
    if (!tool) return;
    const id = tool.id;
    void run(remote ? "查询可用版本" : "读取已安装版本", async () => {
      const result = await localToolsApi.versions(id, remote);
      if (detailRef.current === id) setVersions(result);
    });
  }
  function preview(
    action: ToolAction,
    installation: ToolInstallation | null = null,
    targetVersion = version,
  ) {
    if (!tool) return;
    void run("生成操作预览", async () =>
      setPlan(
        await localToolsApi.preview({
          toolId: tool.id,
          action,
          installationId: installation?.id ?? null,
          provider:
            installation?.provider ??
            (action === "pin-project" ? null : provider || null),
          version: targetVersion.trim() || null,
          projectPath: action === "pin-project" ? projectPath : null,
        }),
      ),
    );
  }
  async function open(target: string) {
    await run("打开目标", () => api.open(target));
  }
  const disabled = !!busy;

  if (!desktop)
    return (
      <div className="local-tools">
        <Empty
          title="本地工具需要桌面环境"
          description="使用桌面版扫描工具、查询安装来源和管理版本。浏览器预览不会读取本机数据或执行系统操作。"
        />
      </div>
    );

  return (
    <div className="local-tools">
      <div className="local-tools-heading">
        <div>
          {tool ? (
            <button
              className="text-button"
              onClick={() => {
                setDetail(null);
                setError(null);
              }}
            >
              <ArrowLeft size={16} />
              返回工具列表
            </button>
          ) : (
            <p className="local-tools-caption">
              发现本机工具，按实际来源管理安装与版本。
            </p>
          )}
          <small>最后扫描 {dateTime(snapshot?.scannedAt ?? null)}</small>
        </div>
        <div className="local-tool-actions">
          <button
            className="button"
            disabled={disabled}
            onClick={() => setDirectoriesOpen(true)}
          >
            <FolderOpen size={16} />
            扫描目录
          </button>
          <button
            className="button"
            disabled={disabled}
            onClick={() => void refresh()}
          >
            <RefreshCw
              size={16}
              className={busy === "扫描本机" ? "spinning" : ""}
            />
            扫描本机
          </button>
          <button
            className="button primary"
            disabled={disabled}
            onClick={() => {
              setAdding(true);
              setCandidates([]);
              setSearchDiagnostics([]);
            }}
          >
            <Plus size={16} />
            添加工具
          </button>
        </div>
      </div>
      {busy && (
        <p role="status" className="local-tools-progress">
          <RefreshCw size={14} className="spinning" />
          {busy}…
        </p>
      )}
      {error && (
        <p role="alert" className="inline-warning">
          {error}
          <button className="text-button" onClick={() => setError(null)}>
            关闭
          </button>
        </p>
      )}
      {notice && (
        <p role="status" className="local-tools-notice">
          {notice}
          <button className="text-button" onClick={() => setNotice(null)}>
            关闭
          </button>
        </p>
      )}

      {!tool && tab === "updates" && (
        <div className="local-tools-update-check">
          <div className="local-tool-actions">
            <button
              className="button"
              disabled={disabled}
              onClick={() =>
                void run("检查管理器更新", async () => {
                  const result = await localToolsApi.updates();
                  setUpdateDiagnostics(result.diagnostics);
                  setSnapshot(await localToolsApi.list());
                })
              }
            >
              <RefreshCw size={16} />
              检查更新
            </button>
            <small>
              最后检查 {dateTime(snapshot?.updatesCheckedAt ?? null)}
            </small>
          </div>
          <p className="local-tools-caption">
            运行时默认只提示同一主版本系列内的升级；跨主版本可在详情中查询后选择。
          </p>
          {updateDiagnostics.map((d, i) => (
            <p className="inline-warning" key={i}>
              {d}
            </p>
          ))}
        </div>
      )}
      {tool ? (
        <>
          <Panel
            title={tool.name}
            subtitle={metadata?.description || tool.description}
            action={
              <button
                className="text-button"
                disabled={disabled}
                onClick={() =>
                  void run("更新资料", async () => {
                    await localToolsApi.metadata(tool.id);
                    setSnapshot(await localToolsApi.list());
                  })
                }
              >
                <RefreshCw size={14} />
                更新资料
              </button>
            }
          >
            <nav className="segmented local-tools-tabs" aria-label="工具详情">
              {(
                [
                  ["overview", "概览"],
                  ["versions", "安装与版本"],
                  ["project", "项目配置"],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  disabled={disabled}
                  aria-pressed={detailTab === id}
                  className={detailTab === id ? "active" : ""}
                  onClick={() => {
                    setDetailTab(id);
                    if (id === "versions" && tool.runtimeProvider && !versions)
                      loadVersions(false);
                  }}
                >
                  {label}
                </button>
              ))}
            </nav>
            {detailTab === "overview" && (
              <div className="local-tool-overview">
                <dl>
                  <dt>官网</dt>
                  <dd>
                    <button
                      className="text-button local-tool-path"
                      onClick={() =>
                        void open(metadata?.homepage || tool.homepage)
                      }
                    >
                      {metadata?.homepage || tool.homepage}
                      <ArrowUpRight size={14} />
                    </button>
                  </dd>
                  <dt>GitHub</dt>
                  <dd>
                    <button
                      className="text-button local-tool-path"
                      onClick={() => void open(tool.repository)}
                    >
                      {tool.repository}
                      <ArrowUpRight size={14} />
                    </button>
                  </dd>
                  <dt>许可证</dt>
                  <dd>{metadata?.license || "未读取"}</dd>
                  <dt>最新发布</dt>
                  <dd>
                    {metadata?.latestRelease || "未提供 Release"}
                    {metadata?.releaseUrl && (
                      <button
                        className="text-button"
                        onClick={() => void open(metadata.releaseUrl!)}
                      >
                        查看发布
                      </button>
                    )}
                  </dd>
                  <dt>资料时间</dt>
                  <dd>{dateTime(metadata?.fetchedAt || null)}</dd>
                  <dt>版本管理器</dt>
                  <dd>{tool.runtimeProvider || "按工具本体安装来源管理"}</dd>
                </dl>
                <p className="local-tools-caption">
                  GitHub
                  最新发布是资料信息；安装版本以对应管理器的版本目录为准。
                </p>
                {metadata?.error && (
                  <p className="inline-warning" role="alert">
                    资料更新失败，保留已有缓存：{metadata.error}
                  </p>
                )}
                <h3>本机安装实例</h3>
                {renderInstallations()}
              </div>
            )}
            {detailTab === "versions" && (
              <div className="local-tool-version-panel">
                <div className="local-tool-form-row">
                  <label>
                    安装管理器
                    <select
                      value={provider}
                      disabled={disabled}
                      onChange={(e) => setProvider(e.target.value)}
                    >
                      <option value="">请选择</option>
                      {availableProviders.map((id) => (
                        <option key={id} value={id}>
                          {id}
                          {id === tool.runtimeProvider
                            ? " · 多版本管理"
                            : " · 工具本体"}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    目标版本
                    <input
                      value={version}
                      onChange={(e) => setVersion(e.target.value)}
                      placeholder={
                        provider === tool.runtimeProvider
                          ? "填写精确版本或选择下方版本"
                          : "可留空，使用源默认版本"
                      }
                    />
                  </label>
                  <button
                    className="button primary"
                    disabled={
                      disabled ||
                      !provider ||
                      (provider === tool.runtimeProvider && !version.trim())
                    }
                    onClick={() => preview("install")}
                  >
                    <Download size={16} />
                    预览安装
                  </button>
                </div>
                {tool.runtimeProvider && (
                  <>
                    <div className="local-tool-actions">
                      <button
                        className="button"
                        disabled={disabled}
                        onClick={() => loadVersions(false)}
                      >
                        读取已安装版本
                      </button>
                      <button
                        className="button"
                        disabled={disabled}
                        onClick={() => loadVersions(true)}
                      >
                        查询可用版本
                      </button>
                    </div>
                    <p>
                      管理器默认：
                      <strong>
                        {versions?.defaultVersion || "未配置 / 未读取"}
                      </strong>
                      　工具箱当前解析：
                      <strong>
                        {installations.find((i) => i.resolved)?.version ||
                          "未找到"}
                      </strong>
                    </p>
                    {versions?.diagnostic && (
                      <p className="local-tools-caption">
                        {versions.diagnostic}
                      </p>
                    )}
                    <div className="local-tool-version-list">
                      {versions?.versions.map((v, index) => (
                        <div
                          className="local-tool-version"
                          key={`${v.version}-${index}`}
                        >
                          <span className="mono">{v.version}</span>
                          <span>
                            {v.installed ? "已安装" : "可安装"}
                            {v.default ? " · 默认" : ""}
                          </span>
                          <button
                            className="text-button"
                            disabled={disabled}
                            onClick={() => {
                              setVersion(v.version);
                              setProvider(tool.runtimeProvider!);
                            }}
                          >
                            选择版本
                          </button>
                          {v.installed && (
                            <button
                              className="text-button"
                              disabled={disabled}
                              onClick={() => {
                                setProvider(tool.runtimeProvider!);
                                void run("生成操作预览", async () =>
                                  setPlan(
                                    await localToolsApi.preview({
                                      toolId: tool.id,
                                      action: "set-default",
                                      provider: tool.runtimeProvider,
                                      version: v.version,
                                      installationId: null,
                                      projectPath: null,
                                    }),
                                  ),
                                );
                              }}
                            >
                              设为默认…
                            </button>
                          )}
                        </div>
                      ))}
                    </div>
                  </>
                )}
                <h3>已发现的安装</h3>
                {renderInstallations()}
                <p className="local-tools-caption">
                  升级、卸载按所选安装实例的真实来源执行。安装新运行时版本后，可另行设为默认；既有安装不会自动迁移。
                </p>
                {renderProviders()}
              </div>
            )}
            {detailTab === "project" && (
              <div className="local-tool-version-panel">
                {!tool.runtimeProvider ? (
                  <p>此工具不支持项目版本固定。</p>
                ) : (
                  <>
                    <div className="local-tool-form-row">
                      <label>
                        项目目录
                        <input
                          list="local-tool-projects"
                          value={projectPath}
                          onChange={(e) => {
                            setProjectPath(e.target.value);
                            setProject(null);
                          }}
                          placeholder="选择或输入项目目录"
                        />
                      </label>
                      <datalist id="local-tool-projects">
                        {snapshot?.projects.map((p) => (
                          <option key={p} value={p} />
                        ))}
                      </datalist>
                      <button
                        className="button"
                        disabled={disabled}
                        onClick={() =>
                          void run("选择项目", async () => {
                            const paths = await chooseFile(true);
                            if (paths?.[0]) {
                              setProjectPath(paths[0]);
                              setProject(
                                await localToolsApi.project(tool.id, paths[0]),
                              );
                            }
                          })
                        }
                      >
                        <FolderOpen size={16} />
                        选择目录
                      </button>
                      <button
                        className="button"
                        disabled={disabled || !projectPath}
                        onClick={() =>
                          void run("读取项目配置", async () =>
                            setProject(
                              await localToolsApi.project(tool.id, projectPath),
                            ),
                          )
                        }
                      >
                        读取配置
                      </button>
                    </div>
                    <div className="local-tool-form-row">
                      <label>
                        固定版本
                        <input
                          value={version}
                          onChange={(e) => setVersion(e.target.value)}
                          placeholder="例如 24.16.0 / temurin-25 / stable"
                        />
                      </label>
                      <button
                        className="button primary"
                        disabled={disabled || !projectPath || !version.trim()}
                        onClick={() => preview("pin-project")}
                      >
                        预览项目修改
                      </button>
                    </div>
                    {project && (
                      <>
                        <p>
                          项目声明：{project.selectedVersion || "未配置"}
                          　实际解析：{project.effectiveVersion || "尚未核实"}
                        </p>
                        {project.diagnostics.map((d, i) => (
                          <p className="local-tools-caption" key={i}>
                            {d}
                          </p>
                        ))}
                        {project.files.map((f) => (
                          <div key={f.path}>
                            <strong className="local-tool-path">
                              {f.path}
                            </strong>
                            <pre>{f.before}</pre>
                          </div>
                        ))}
                      </>
                    )}
                  </>
                )}
                <p className="local-tools-caption">
                  不会自动修改 PATH 或 shell
                  启动配置。请按管理器官方指引初始化终端，项目固定不会改变已经打开的其他终端。
                </p>
                {renderProviders()}
              </div>
            )}
          </Panel>
        </>
      ) : (
        <>
          <div className="local-tools-toolbar">
            <label className="local-tools-search">
              <Search size={17} />
              <input
                aria-label="筛选本地工具"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="搜索名称、命令或仓库…"
              />
            </label>
            <select
              aria-label="筛选工具来源"
              value={source}
              onChange={(e) => setSource(e.target.value)}
            >
              <option value="all">全部来源</option>
              <option value="unknown">来源待核实</option>
              {[
                ...new Set(
                  snapshot?.installations
                    .map((i) => i.provider)
                    .filter((p): p is string => !!p) ?? [],
                ),
              ].map((id) => (
                <option key={id} value={id}>
                  {id}
                </option>
              ))}
            </select>
          </div>
          <nav className="segmented local-tools-tabs" aria-label="本地工具分类">
            {(
              [
                ["installed", "已安装"],
                ["catalog", "工具目录"],
                ["updates", "可更新"],
                ["operations", "操作记录"],
              ] as const
            ).map(([id, label]) => (
              <button
                className={tab === id ? "active" : ""}
                aria-pressed={tab === id}
                key={id}
                onClick={() => setTab(id)}
              >
                {label}
                {id === "operations" &&
                operations.some((o) => ["queued", "running"].includes(o.status))
                  ? " · 执行中"
                  : ""}
              </button>
            ))}
          </nav>
          {tab === "operations" ? (
            <Panel
              title="最近操作"
              subtitle="关闭页面后任务继续运行；失败和中断保留实际记录。"
            >
              <div className="local-tool-operation-list">
                {operations.map((o) => (
                  <button
                    key={o.id}
                    className="local-tool-operation"
                    onClick={() => setSelectedOperation(o.id)}
                  >
                    <span>
                      {snapshot?.definitions.find((t) => t.id === o.toolId)
                        ?.name || o.toolId}{" "}
                      · {toolActionText[o.action] || o.action}
                      <small>
                        {o.provider} · {dateTime(o.startedAt)}
                      </small>
                    </span>
                    <span className={`local-tool-state ${o.status}`}>
                      {toolStatusText[o.status] || o.status}
                    </span>
                  </button>
                ))}
                {operations.length === 0 && (
                  <p className="local-tools-caption">暂无操作记录。</p>
                )}
              </div>
            </Panel>
          ) : (
            <Panel
              title={
                tab === "catalog"
                  ? "工具目录"
                  : tab === "updates"
                    ? "更新信息"
                    : "本机工具"
              }
              subtitle={
                tab === "updates"
                  ? "仅显示管理器核实的可更新项；GitHub 最新发布不会直接判为可安装更新。"
                  : `${snapshot?.installations.length ?? 0} 个安装入口 · 可查看同一工具的多份安装`
              }
            >
              <div
                className="local-tool-table"
                role="table"
                aria-label="本地工具列表"
              >
                <div className="local-tool-table-head" role="row">
                  <span role="columnheader">工具</span>
                  <span role="columnheader">版本</span>
                  <span role="columnheader">管理来源</span>
                  <span role="columnheader">状态</span>
                </div>
                {filteredTools().map((value) => {
                  const items =
                    snapshot?.installations.filter(
                      (i) => i.toolId === value.id,
                    ) ?? [];
                  const resolved = items.find((i) => i.resolved) ?? items[0];
                  return (
                    <button
                      className="local-tool-row"
                      key={value.id}
                      onClick={() => selectTool(value)}
                      aria-label={`查看 ${value.name}`}
                    >
                      <span>
                        <strong>{value.name}</strong>
                        <small>{value.description}</small>
                      </span>
                      <span className="mono">
                        {resolved?.version || "未安装"}
                        {items.length > 1 && (
                          <small>{items.length} 个入口</small>
                        )}
                      </span>
                      <span>
                        {[
                          ...new Set(items.map((i) => i.provider || "待核实")),
                        ].join(" / ") ||
                          value.runtimeProvider ||
                          "—"}
                      </span>
                      <span
                        className={`local-tool-state ${items.some((i) => i.diagnostic) ? "failed" : ""}`}
                      >
                        {items.some((i) => i.diagnostic)
                          ? "探测异常"
                          : items.some((i) => i.provider)
                            ? "可管理"
                            : items.length
                              ? "来源待核实"
                              : value.registered
                                ? "已添加资料"
                                : "目录工具"}
                      </span>
                    </button>
                  );
                })}
              </div>
              {filteredTools().length === 0 && (
                <Empty
                  title={
                    tab === "updates"
                      ? "尚无已核实的更新"
                      : "没有符合条件的工具"
                  }
                  description={
                    tab === "updates"
                      ? "在工具详情查询管理器可用版本；工具资料中的最新发布仅作参考。"
                      : "可扫描本机、更改筛选，或搜索添加工具资料。"
                  }
                />
              )}
            </Panel>
          )}
          {snapshot?.diagnostics.length ? (
            <details className="local-tools-diagnostics">
              <summary>扫描诊断 · {snapshot.diagnostics.length}</summary>
              {snapshot.diagnostics.map((d, i) => (
                <p key={i}>{d}</p>
              ))}
            </details>
          ) : null}
        </>
      )}

      <Modal
        title="搜索并添加工具"
        description="先匹配工具目录，再搜索 GitHub 与本机已有包管理器。添加资料不会安装软件。"
        open={adding}
        onClose={() => setAdding(false)}
        wide
      >
        <form
          className="local-tool-form-row"
          onSubmit={(e) => {
            e.preventDefault();
            void run("搜索工具", async () => {
              const result = await localToolsApi.search(searchQuery);
              setCandidates(result.candidates);
              setSearchDiagnostics(result.diagnostics);
            });
          }}
        >
          <label>
            工具名称或 GitHub 地址
            <input
              autoFocus
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="例如 uv 或 https://github.com/astral-sh/uv"
            />
          </label>
          <button
            className="button primary"
            disabled={disabled || !searchQuery.trim()}
            type="submit"
          >
            <Search size={16} />
            搜索
          </button>
        </form>
        {busy && <p role="status">{busy}…</p>}
        {error && (
          <p role="alert" className="inline-warning">
            {error}
          </p>
        )}
        {searchDiagnostics.map((d, i) => (
          <p className="inline-warning" key={i}>
            {d}
          </p>
        ))}
        <div className="local-tool-candidates">
          {candidates.map((candidate, index) => (
            <article
              key={`${candidate.repository}-${candidate.provider}-${index}`}
            >
              <div>
                <strong>{candidate.name}</strong>
                <small>
                  {candidate.provider}
                  {candidate.package ? ` · ${candidate.package}` : ""}
                </small>
                <p>{candidate.description}</p>
                <span className="local-tool-path">{candidate.repository}</span>
              </div>
              <button
                className="button"
                disabled={disabled}
                onClick={() =>
                  void run("添加工具资料", async () => {
                    setSnapshot(await localToolsApi.add(candidate));
                    setNotice(`已添加 ${candidate.name} 的资料，未安装软件。`);
                    setAdding(false);
                    setTab("catalog");
                  })
                }
              >
                <Plus size={14} />
                添加资料
              </button>
            </article>
          ))}
        </div>
      </Modal>
      <Modal
        title="确认工具操作"
        description="核对目标、来源和影响后执行。每个预览只可使用一次。"
        open={!!plan}
        onClose={() => setPlan(null)}
        wide
      >
        {plan && (
          <>
            <h3>
              {toolActionText[plan.request.action]} ·{" "}
              {
                snapshot?.definitions.find((t) => t.id === plan.request.toolId)
                  ?.name
              }
            </h3>
            {plan.impact.map((text, i) => (
              <p key={i}>{text}</p>
            ))}
            {plan.executable && (
              <pre className="local-tool-command">
                {plan.executable}
                {"\n"}
                {plan.args.map((a) => JSON.stringify(a)).join(" ")}
              </pre>
            )}
            {plan.files.map((file) => (
              <div className="local-tool-diff" key={file.path}>
                <strong className="local-tool-path">{file.path}</strong>
                <div>
                  <section>
                    <small>修改前</small>
                    <pre>{file.before ?? "（新文件）"}</pre>
                  </section>
                  <section>
                    <small>修改后</small>
                    <pre>{file.after}</pre>
                  </section>
                </div>
              </div>
            ))}
            {error && (
              <p role="alert" className="inline-warning">
                {error}
              </p>
            )}
            <div className="modal-actions">
              <button
                className="button"
                disabled={disabled}
                onClick={() => setPlan(null)}
              >
                取消
              </button>
              <button
                className="button primary"
                disabled={disabled}
                onClick={() =>
                  void run("提交操作", async () => {
                    const operation = await localToolsApi.execute(plan.id);
                    setOperations((old) => [operation, ...old]);
                    setSelectedOperation(operation.id);
                    setPlan(null);
                  })
                }
              >
                <Check size={16} />
                确认执行
              </button>
            </div>
          </>
        )}
      </Modal>
      <Modal
        title="工具操作详情"
        description="状态由后端任务记录提供；输出限制为标准输出和错误各 1 MiB。"
        open={!!currentOperation}
        onClose={() => setSelectedOperation(null)}
        wide
      >
        {currentOperation && (
          <>
            <p>
              <strong className={`local-tool-state ${currentOperation.status}`}>
                {toolStatusText[currentOperation.status]}
              </strong>
              　{currentOperation.provider} ·{" "}
              {toolActionText[currentOperation.action]}　退出码：
              {currentOperation.exitCode ?? "—"}
            </p>
            <pre className="local-tool-output" role="log" aria-live="polite">
              {currentOperation.output}
            </pre>
            {currentOperation.backupPaths.map((p) => (
              <p className="local-tool-path" key={p}>
                备份：{p}
              </p>
            ))}
          </>
        )}
      </Modal>
      <Modal
        title="额外扫描目录"
        description="只检查这些目录中的已知工具命令，不递归扫描磁盘，也不执行未知程序。"
        open={directoriesOpen}
        onClose={() => setDirectoriesOpen(false)}
      >
        {snapshot?.directories.map((path) => (
          <div className="local-tool-directory" key={path}>
            <span className="local-tool-path">{path}</span>
            <button
              className="text-button"
              disabled={disabled}
              onClick={() =>
                void run("更新扫描目录", async () => {
                  await localToolsApi.directories(
                    snapshot.directories.filter((p) => p !== path),
                  );
                  setSnapshot(await localToolsApi.list());
                })
              }
            >
              移除
            </button>
          </div>
        ))}
        <button
          className="button"
          disabled={disabled}
          onClick={() =>
            void run("添加扫描目录", async () => {
              const paths = await chooseFile(true, true);
              if (!paths) return;
              await localToolsApi.directories([
                ...new Set([...(snapshot?.directories ?? []), ...paths]),
              ]);
              setSnapshot(await localToolsApi.list());
            })
          }
        >
          <FolderOpen size={16} />
          选择目录
        </button>
        {error && (
          <p className="inline-warning" role="alert">
            {error}
          </p>
        )}
      </Modal>
    </div>
  );

  function filteredTools() {
    return (snapshot?.definitions ?? []).filter((value) => {
      const items =
        snapshot?.installations.filter((i) => i.toolId === value.id) ?? [];
      if (tab === "installed" && !items.length) return false;
      // 更新由后端管理器核实；不把 GitHub release tag 与本机版本字符串直接比较。
      if (
        tab === "updates" &&
        !snapshot?.updates.some(
          (u) =>
            u.toolId === value.id &&
            items.some((i) => i.id === u.installationId),
        )
      )
        return false;
      if (
        source !== "all" &&
        !items.some((i) =>
          source === "unknown" ? !i.provider : i.provider === source,
        )
      )
        return false;
      return `${value.name} ${value.id} ${value.description} ${value.commands.join(" ")} ${value.repository}`
        .toLowerCase()
        .includes(query.toLowerCase());
    });
  }
  function renderInstallations() {
    return (
      <div className="local-tool-installations">
        {installations.length === 0 ? (
          <p className="local-tools-caption">
            尚未发现本机安装。可选择已有管理器安装，或添加扫描目录。
          </p>
        ) : (
          installations.map((item) => (
            <article key={item.id}>
              <div className="local-tool-installation-heading">
                <strong className="mono">{item.version || "版本未知"}</strong>
                <span>
                  {item.provider || "来源待核实"}
                  {item.resolved ? " · 工具箱当前解析" : ""}
                </span>
              </div>
              <button
                className="text-button local-tool-path"
                onClick={() =>
                  void open(item.path.replace(/[\\/][^\\/]+$/, ""))
                }
              >
                <FolderOpen size={14} />
                {item.path}
              </button>
              <p className="local-tools-caption">{item.evidence}</p>
              {item.diagnostic && (
                <p className="inline-warning">{item.diagnostic}</p>
              )}
              <div className="local-tool-actions">
                {item.provider && (
                  <>
                    <button
                      className="button"
                      disabled={
                        disabled ||
                        item.status === "error" ||
                        (item.provider === tool?.runtimeProvider &&
                          !version.trim())
                      }
                      onClick={() =>
                        preview(
                          "upgrade",
                          item,
                          item.provider === tool?.runtimeProvider
                            ? version
                            : "",
                        )
                      }
                    >
                      预览升级
                    </button>
                    {item.provider !== "uv-self" && (
                      <button
                        className="text-button danger"
                        disabled={disabled}
                        onClick={() => preview("uninstall", item, "")}
                      >
                        卸载…
                      </button>
                    )}
                  </>
                )}
                {!item.provider && (
                  <small>来源未核实，仅提供资料与目录查看。</small>
                )}
              </div>
            </article>
          ))
        )}
      </div>
    );
  }
  function renderProviders() {
    return (
      <details className="local-tools-diagnostics">
        <summary>
          <Terminal size={14} />
          管理器状态与终端初始化
        </summary>
        {snapshot?.providers
          .filter(
            (p) =>
              p.id === tool?.runtimeProvider ||
              tool?.packages.some((m) => m.provider === p.id),
          )
          .map((p) => (
            <div className="local-tool-provider" key={p.id}>
              <strong>
                {p.id} · {p.available ? p.version : "不可用"}
              </strong>
              <span className="local-tool-path">{p.executable}</span>
              {p.diagnostic && <p>{p.diagnostic}</p>}
              <button
                className="text-button"
                onClick={() => void open(p.guide)}
              >
                安装与初始化指引
                <ArrowUpRight size={14} />
              </button>
              {!p.available &&
                snapshot?.definitions.find((t) => t.id === p.id) && (
                  <button
                    className="text-button"
                    disabled={disabled}
                    onClick={() =>
                      selectTool(
                        snapshot.definitions.find((t) => t.id === p.id)!,
                      )
                    }
                  >
                    管理 {p.id} 本体
                  </button>
                )}
            </div>
          ))}
      </details>
    );
  }
}
