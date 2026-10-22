import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  Activity,
  ArrowUpRight,
  Box,
  Braces,
  Check,
  Command,
  Database,
  FileCode2,
  FolderOpen,
  LayoutDashboard,
  ListFilter,
  Moon,
  Newspaper,
  MoreHorizontal,
  PanelLeftClose,
  Play,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Sun,
  Terminal,
  Trash2,
  Wand2,
  X,
  Zap,
} from "lucide-react";
import { api, chooseFile, command, desktop } from "./api";
import { AgentMark, Empty, Modal, Panel, Status } from "./components";
import { Dashboard } from "./pages/Dashboard";
import { Agents } from "./pages/Agents";
import { Sessions, SessionDetail } from "./pages/Sessions";
import { Skills } from "./pages/Skills";
import { LocalTools } from "./pages/LocalTools";
import { EnvironmentVariables } from "./pages/EnvironmentVariables";
import { AgentNews } from "./pages/AgentNews";
import { NewsRecommendations } from "./pages/NewsRecommendations";
import { useNews } from "./news";
import { skillsApi } from "./skills-api";
import { useStore } from "./store";
import { bounds, dateTime, filterRecords } from "./stats";
import type {
  LaunchEntry,
  Page,
  RenamePlan,
  Script,
  ScriptResult,
  SourceStatus,
  Theme,
  UsageRecord,
} from "./types";

const navigation: {
  group: string;
  items: { id: Page; label: string; icon: typeof Activity; key?: string }[];
}[] = [
  {
    group: "工作台",
    items: [
      { id: "overview", label: "用量总览", icon: LayoutDashboard, key: "1" },
      { id: "agents", label: "Agents", icon: Box, key: "2" },
      { id: "sessions", label: "会话", icon: ListFilter, key: "3" },
      { id: "skills", label: "Skills", icon: Braces },
      { id: "agent-news", label: "Agent 资讯", icon: Newspaper },
    ],
  },
  {
    group: "工具",
    items: [
      { id: "local-tools", label: "本地工具", icon: Terminal },
      { id: "launchers", label: "快速入口", icon: Zap, key: "4" },
      { id: "rename", label: "批量改名", icon: Wand2 },
      { id: "scripts", label: "脚本", icon: FileCode2 },
    ],
  },
  {
    group: "系统",
    items: [
      { id: "environment", label: "环境变量", icon: Braces },
      { id: "settings", label: "设置", icon: Settings2, key: "5" },
      { id: "sources", label: "数据源", icon: Database },
    ],
  },
];
const pages = navigation.flatMap((g) => g.items);
type Perform = (fn: () => Promise<unknown>) => void;

export default function App() {
  useEffect(() => {
    void useNews.getState().initialize();
    if (!desktop) return;
    const refresh = () => void useNews.getState().refresh();
    const timer = setInterval(refresh, 60_000);
    window.addEventListener("agent-news-refresh", refreshNews);
    function refreshNews() { void useNews.getState().refresh(true); }
    return () => { clearInterval(timer); window.removeEventListener("agent-news-refresh", refreshNews); };
  }, []);
  const state = useStore();
  const {
    page,
    range,
    source,
    customStart,
    customEnd,
    settings,
    snapshot,
    loading,
    error,
    notice,
    palette,
    patch,
    setPage,
  } = state;
  const [session, setSession] = useState<UsageRecord | null>(null),
    [selectedSource, setSelectedSource] = useState<SourceStatus | null>(null);
  const [systemDark, setSystemDark] = useState(
    matchMedia("(prefers-color-scheme: dark)").matches,
  );
  const dark =
    settings.theme === "dark" || (settings.theme === "system" && systemDark);
  useEffect(() => {
    void useStore.getState().initialize();
    const media = matchMedia("(prefers-color-scheme: dark)");
    const update = () => setSystemDark(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  useEffect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
  }, [dark]);
  useEffect(() => {
    if (!desktop) return;
    const timer = setInterval(
      () => void useStore.getState().refresh(),
      settings.refreshSeconds * 1000,
    );
    return () => clearInterval(timer);
  }, [settings.refreshSeconds]);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key.toLowerCase() === "k") {
        e.preventDefault();
        useStore.setState((s) => ({ palette: !s.palette }));
      }
      if (e.key.toLowerCase() === "r") {
        e.preventDefault();
        if (useStore.getState().page === "skills")
          window.dispatchEvent(
            new CustomEvent("skills-action", { detail: { action: "refresh" } }),
          );
        else if (useStore.getState().page === "environment") window.dispatchEvent(new Event("environment-refresh"));
        else if (useStore.getState().page === "local-tools") window.dispatchEvent(new Event("local-tools-refresh"));
        else if (useStore.getState().page === "agent-news") window.dispatchEvent(new Event("agent-news-refresh"));
        else void useStore.getState().refresh();
      }
      const page = (
        {
          1: "overview",
          2: "agents",
          3: "sessions",
          4: "launchers",
          5: "settings",
        } as Record<string, Page>
      )[e.key];
      if (page) {
        e.preventDefault();
        useStore.getState().setPage(page);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);
  const perform: Perform = (fn) => {
    void fn().catch((e) => patch({ error: String(e) }));
  };
  const interval = bounds(range, customStart, customEnd);
  const all = snapshot?.records ?? [];
  const records = useMemo(
    () => filterRecords(all, interval.start, interval.end, source),
    [all, interval.start, interval.end, source],
  );
  const previous = useMemo(
    () =>
      filterRecords(all, interval.previousStart, interval.previousEnd, source),
    [all, interval.previousStart, interval.previousEnd, source],
  );
  const shared = {
    records,
    onSource: setSelectedSource,
    onSession: setSession,
    perform,
  };
  const title = pages.find((p) => p.id === page)?.label;
  const healthy = (snapshot?.sources ?? []).filter(
    (s) => s.status === "ready",
  ).length;
  const theme = (value: Theme) => {
    if (desktop) perform(() => state.save({ ...settings, theme: value }));
    else patch({ settings: { ...settings, theme: value } });
  };
  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">
        跳到页面内容
      </a>
      <aside className="sidebar">
        <button
          className="brand"
          onClick={() => setPage("overview")}
          aria-label="个人工具箱首页"
        >
          <span className="brand-icon">
            <Braces size={21} />
          </span>
          <span>
            <strong>个人工具箱</strong>
            <small>PERSONAL WORKSPACE</small>
          </span>
        </button>
        <button
          className="command-search"
          onClick={() => patch({ palette: true })}
        >
          <Search size={16} />
          <span>搜索与命令</span>
          <kbd>Ctrl K</kbd>
        </button>
        <nav aria-label="主导航">
          {navigation.map((g) => (
            <div className="nav-group" key={g.group}>
              <span className="nav-group-label">{g.group}</span>
              {g.items.map(({ id, label, icon: Icon, key }) => (
                <button
                  key={id}
                  className={`nav-item ${page === id ? "selected" : ""}`}
                  aria-label={label}
                  aria-current={page === id ? "page" : undefined}
                  onClick={() => setPage(id)}
                >
                  <Icon size={18} />
                  <span>{label}</span>
                  {key && <span className="nav-key">{key}</span>}
                  {id === "agents" && snapshot && (
                    <span className="nav-count">{snapshot.sources.length}</span>
                  )}
                </button>
              ))}
            </div>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="collection-status">
            <span className={`status-dot ${error ? "warning" : ""}`} />
            <strong>
              {loading
                ? "正在采集"
                : snapshot
                  ? `${healthy} 个来源已更新`
                  : "等待连接数据源"}
            </strong>
            <button
              className="icon-button"
              aria-label="查看数据源"
              onClick={() => setPage("sources")}
            >
              <MoreHorizontal size={16} />
            </button>
          </div>
          <small>最后刷新 {dateTime(snapshot?.collectedAt ?? null)}</small>
          <button
            className="text-button data-folder"
            disabled={!snapshot}
            onClick={() =>
              snapshot && perform(() => api.open(snapshot.dataDirectory))
            }
          >
            <FolderOpen size={14} />
            本地数据目录
            <ArrowUpRight size={12} />
          </button>
          <div className="sidebar-version">
            <span>个人工具箱</span>
            <span className="mono">v0.1.0</span>
          </div>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            <PanelLeftClose size={16} />
            <span>工作台</span>
            <span className="slash">/</span>
            <strong>{title}</strong>
          </div>
          <div className="top-controls">
            {page !== "skills" && page !== "local-tools" && page !== "agent-news" && page !== "environment" && (
              <>
                <div className="segmented range-selector">
                  {[
                    ["1", "今天"],
                    ["7", "7 天"],
                    ["30", "30 天"],
                    ["all", "全部"],
                    ["custom", "自定义"],
                  ].map(([id, label]) => (
                    <button
                      className={range === id ? "active" : ""}
                      aria-pressed={range === id}
                      key={id}
                      onClick={() => patch({ range: id })}
                    >
                      {label}
                    </button>
                  ))}
                </div>
                <select
                  className="source-select"
                  aria-label="筛选 Agent 来源"
                  value={source}
                  onChange={(e) => patch({ source: e.target.value })}
                >
                  <option value="all">全部来源</option>
                  {(settings.sources.length
                    ? settings.sources
                    : (snapshot?.sources ?? [])
                  ).map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name}
                    </option>
                  ))}
                </select>
                <span className="toolbar-divider" />
              </>
            )}
            <button
              className="icon-button"
              aria-label="刷新数据"
              disabled={loading}
              onClick={() =>
                page === "skills"
                  ? window.dispatchEvent(
                      new CustomEvent("skills-action", {
                        detail: { action: "refresh" },
                      }),
                    )
                  : page === "environment" ? window.dispatchEvent(new Event("environment-refresh")) : page === "local-tools" ? window.dispatchEvent(new Event("local-tools-refresh")) : page === "agent-news" ? window.dispatchEvent(new Event("agent-news-refresh")) : void state.refresh()
              }
            >
              <RefreshCw size={17} />
            </button>
            <button
              className="icon-button"
              aria-label={dark ? "切换浅色主题" : "切换深色主题"}
              onClick={() => theme(dark ? "light" : "dark")}
            >
              {dark ? <Sun size={18} /> : <Moon size={18} />}
            </button>
            <button
              className="icon-button"
              aria-label="打开设置"
              onClick={() => setPage("settings")}
            >
              <Settings2 size={18} />
            </button>
          </div>
        </header>
        {range === "custom" && page !== "skills" && page !== "local-tools" && page !== "agent-news" && page !== "environment" && (
          <div className="custom-range">
            <label>
              开始日期
              <input
                type="date"
                value={customStart}
                onChange={(e) => patch({ customStart: e.target.value })}
              />
            </label>
            <span>—</span>
            <label>
              结束日期
              <input
                type="date"
                min={customStart}
                value={customEnd}
                onChange={(e) => patch({ customEnd: e.target.value })}
              />
            </label>
          </div>
        )}
        <main id="main-content" tabIndex={-1}>
          {(notice || error) && (
            <div
              className={`notice ${error ? "error" : ""}`}
              role={error ? "alert" : "status"}
            >
              <Activity size={16} />
              <span>
                {error ?? notice}
                {error && snapshot ? " · 保留上次成功数据" : ""}
              </span>
              <button
                className="icon-button"
                aria-label="关闭提示"
                onClick={() => patch({ error: null, notice: null })}
              >
                <X size={15} />
              </button>
            </div>
          )}
          {loading && (
            <div className="loading-banner" role="status">
              {snapshot
                ? "正在刷新，当前展示上次成功数据…"
                : "正在读取本地用量数据…"}
            </div>
          )}
          {page === "overview" ? (
            <Dashboard
              {...shared}
              previous={previous}
              start={interval.start}
              end={interval.end}
            />
          ) : page === "agents" ? (
            <Agents {...shared} />
          ) : page === "sessions" ? (
            <Sessions {...shared} />
          ) : page === "skills" ? (
            <Skills />
          ) : page === "local-tools" ? (
            <LocalTools />
          ) : page === "environment" ? (
            <EnvironmentVariables />
          ) : page === "agent-news" ? (
            <AgentNews />
          ) : page === "sources" ? (
            <Sources perform={perform} />
          ) : page === "settings" ? (
            <SettingsPage perform={perform} theme={theme} />
          ) : page === "rename" ? (
            <Rename perform={perform} />
          ) : page === "scripts" ? (
            <Scripts perform={perform} />
          ) : (
            <Launchers perform={perform} />
          )}
        </main>
        <footer className="workspace-footer">
          <span>
            <span className="status-dot" />
            数据在本机，工作在手边
          </span>
          <button
            className="text-button"
            onClick={() => patch({ palette: true })}
          >
            <Command size={12} />
            命令面板 <kbd>Ctrl K</kbd>
          </button>
        </footer>
      </div>
      <div className="app-corner-stack">
        <div id="corner-notifications" />
        <NewsRecommendations onBrowse={() => setPage("agent-news")} />
      </div>
      <CommandPalette
        open={palette}
        onClose={() => patch({ palette: false })}
        perform={perform}
      />
      <SessionDetail
        record={session}
        records={all}
        onClose={() => setSession(null)}
      />
      <SourceDetail
        source={selectedSource}
        onClose={() => setSelectedSource(null)}
      />
    </div>
  );
}

function Intro({
  eyebrow,
  title,
  description,
  action,
}: {
  eyebrow: string;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="page-intro">
      <div>
        <span className="eyebrow">{eyebrow}</span>
        <h1>
          {title}
          <span className="heading-dot">.</span>
        </h1>
        <p>{description}</p>
      </div>
      {action}
    </div>
  );
}

function CommandPalette({
  open,
  onClose,
  perform,
}: {
  open: boolean;
  onClose: () => void;
  perform: Perform;
}) {
  const { setPage, refresh, snapshot, skillNames, selectedSkillId } =
    useStore();
  const skillAction = (action: string, id?: string) => {
    setPage("skills");
    // 等页面挂载后传递命令，生产命令仍通过桌面桥执行。
    setTimeout(
      () =>
        window.dispatchEvent(
          new CustomEvent("skills-action", { detail: { action, id } }),
        ),
      100,
    );
  };
  const [query, setQuery] = useState(""),
    [index, setIndex] = useState(0);
  useEffect(() => {
    if (open) {
      setQuery("");
      setIndex(0);
      if (desktop && !useStore.getState().skillNames.length) {
        perform(async () => {
          const result = await skillsApi.scan();
          useStore.setState({
            skillNames: result.skills.map((s) => ({ id: s.id, name: s.name })),
          });
        });
      }
    }
  }, [open]);
  const actions = [
    ...pages.map((p) => ({
      label: `打开${p.label}`,
      icon: p.icon,
      run: () => setPage(p.id),
    })),
    { label: "刷新所有数据源", icon: RefreshCw, run: () => perform(refresh) },
    {
      label: "导入 Skills GitHub 仓库",
      icon: Braces,
      run: () => skillAction("github"),
    },
    {
      label: "打开 Skills 项目工作区",
      icon: FolderOpen,
      run: () => skillAction("project"),
    },
    ...(selectedSkillId
      ? [
          {
            label: "同步当前 Skill",
            icon: RefreshCw,
            run: () => skillAction("sync"),
          },
        ]
      : []),
    ...skillNames.map((s) => ({
      label: `打开 Skill ${s.name}`,
      icon: Braces,
      run: () => skillAction("detail", s.id),
    })),
    ...(snapshot?.sources ?? []).flatMap((s) => [
      {
        label: `启动 ${s.name}`,
        icon: Terminal,
        run: () => perform(() => api.launch(s.id)),
      },
      {
        label: `打开 ${s.name} 数据目录`,
        icon: FolderOpen,
        run: () => perform(() => api.open(s.root)),
      },
    ]),
  ].filter((a) => a.label.toLowerCase().includes(query.toLowerCase()));
  const execute = (i: number) => {
    actions[i]?.run();
    onClose();
  };
  return (
    <Modal
      open={open}
      onClose={onClose}
      title="搜索与命令"
      description="↑ ↓ 选择 · Enter 执行 · Esc 关闭"
    >
      <div className="palette-search">
        <Search size={20} />
        <input
          autoFocus
          aria-label="搜索命令"
          placeholder="输入页面、Agent 或操作…"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setIndex((i) => Math.min(actions.length - 1, i + 1));
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              setIndex((i) => Math.max(0, i - 1));
            }
            if (e.key === "Enter") {
              e.preventDefault();
              execute(index);
            }
          }}
          aria-activedescendant={`command-${index}`}
          role="combobox"
          aria-controls="command-list"
          aria-expanded="true"
        />
      </div>
      <div className="command-list" id="command-list" role="listbox">
        {actions.map(({ label, icon: Icon }, i) => (
          <button
            id={`command-${i}`}
            role="option"
            aria-selected={index === i}
            className={i === index ? "active" : ""}
            key={label}
            onMouseMove={() => setIndex(i)}
            onClick={() => execute(i)}
          >
            <Icon size={17} />
            <span>{label}</span>
            {index === i && <kbd>↵</kbd>}
          </button>
        ))}
        {!actions.length && <p className="muted">没有匹配的命令</p>}
      </div>
    </Modal>
  );
}

function SourceDetail({
  source,
  onClose,
}: {
  source: SourceStatus | null;
  onClose: () => void;
}) {
  return (
    <Modal
      open={!!source}
      onClose={onClose}
      title="来源详情"
      description="采集覆盖范围与诊断"
    >
      {source && (
        <>
          <div className="detail-lead">
            <AgentMark agent={source.id} large />
            <div>
              <h2>{source.name}</h2>
              <Status status={source.status} />
            </div>
          </div>
          <dl className="detail-grid">
            {[
              ["数据目录", source.root],
              ["客户端", source.executable || "未登记"],
              ["版本", source.version ?? "未读取"],
              ["最后成功采集", dateTime(source.lastScan)],
              ["最后活动", dateTime(source.lastActivity)],
              [
                "覆盖范围",
                `${source.files} 个文件；${source.id === "dsh" ? "cost-meter version 1 账本" : "本地 JSONL 会话与归档"}`,
              ],
            ].map(([k, v]) => (
              <div key={k}>
                <dt>{k}</dt>
                <dd>{v}</dd>
              </div>
            ))}
          </dl>
          <h3>来源诊断</h3>
          {source.diagnostics.length ? (
            source.diagnostics.map((d, i) => (
              <p className="inline-warning" key={i}>
                {d}
              </p>
            ))
          ) : (
            <p className="muted">
              最近一次采集未发现解析错误。缺少的价格与字段仍显示未知。
            </p>
          )}
        </>
      )}
    </Modal>
  );
}

function Sources({ perform }: { perform: Perform }) {
  const { settings, save, refresh, snapshot } = useStore();
  const [draft, setDraft] = useState(settings);
  useEffect(() => setDraft(settings), [settings]);
  return (
    <div className="page-flow">
      <Intro
        eyebrow="本地采集设置"
        title="数据源"
        description="登记本地目录，独立控制每一个采集来源。"
        action={
          <button
            className="button primary"
            onClick={() =>
              perform(async () => {
                await save(draft);
                await refresh();
              })
            }
          >
            <Check size={16} />
            保存并采集
          </button>
        }
      />
      {draft.sources.length ? (
        draft.sources.map((s) => (
          <Panel
            key={s.id}
            title={s.name}
            subtitle={
              s.id === "dsh"
                ? "读取 cost-meter/ledger.json；原始会话不推算用量"
                : "只读会话元数据和 Token 记录"
            }
            action={
              <label className="switch-label">
                <input
                  type="checkbox"
                  checked={s.enabled}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      sources: draft.sources.map((x) =>
                        x.id === s.id ? { ...x, enabled: e.target.checked } : x,
                      ),
                    })
                  }
                />
                启用采集
              </label>
            }
          >
            <div className="form-grid">
              <label>
                数据根目录
                <div className="input-action">
                  <input
                    value={s.root}
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        sources: draft.sources.map((x) =>
                          x.id === s.id ? { ...x, root: e.target.value } : x,
                        ),
                      })
                    }
                  />
                  <button
                    className="icon-button"
                    aria-label={`选择 ${s.name} 目录`}
                    onClick={() =>
                      perform(async () => {
                        const paths = await chooseFile(true);
                        if (paths)
                          setDraft({
                            ...draft,
                            sources: draft.sources.map((x) =>
                              x.id === s.id ? { ...x, root: paths[0] } : x,
                            ),
                          });
                      })
                    }
                  >
                    <FolderOpen size={17} />
                  </button>
                </div>
              </label>
              <label>
                客户端启动文件
                <div className="input-action">
                  <input
                    value={s.executable}
                    placeholder="可选：可执行文件或 PowerShell 脚本"
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        sources: draft.sources.map((x) =>
                          x.id === s.id
                            ? { ...x, executable: e.target.value }
                            : x,
                        ),
                      })
                    }
                  />
                  <button
                    className="icon-button"
                    aria-label={`选择 ${s.name} 启动文件`}
                    onClick={() =>
                      perform(async () => {
                        const paths = await chooseFile();
                        if (paths)
                          setDraft({
                            ...draft,
                            sources: draft.sources.map((x) =>
                              x.id === s.id
                                ? { ...x, executable: paths[0] }
                                : x,
                            ),
                          });
                      })
                    }
                  >
                    <FolderOpen size={17} />
                  </button>
                </div>
              </label>
            </div>
            <div className="panel-foot">
              <span>模型价格缺失时显示未知，不自动生成估算金额。</span>
              {snapshot?.sources.find((x) => x.id === s.id) && (
                <Status
                  status={snapshot.sources.find((x) => x.id === s.id)!.status}
                />
              )}
            </div>
          </Panel>
        ))
      ) : (
        <Panel title="桌面来源检测">
          <Empty
            title="本地采集在桌面版中可用"
            description="使用 npm run desktop 启动后，自动检测 Codex、Claude Code 和 DSH 的默认目录。"
          />
        </Panel>
      )}
      <div className="info-strip">
        <Database size={18} />
        SQLite 缓存保留上次成功记录；解析异常会显示来源诊断。
      </div>
    </div>
  );
}

function SettingsPage({
  perform,
  theme,
}: {
  perform: Perform;
  theme: (value: Theme) => void;
}) {
  const { settings, save, snapshot } = useStore();
  return (
    <div className="page-flow">
      <Intro
        eyebrow="让工作台更适合你"
        title="设置"
        description="熟悉的节奏，安静的界面。"
      />
      <Panel title="外观" subtitle="浅色与深色保持相同的信息层级">
        <div className="theme-options">
          {(["light", "dark", "system"] as Theme[]).map((t) => (
            <button
              className={`theme-option ${settings.theme === t ? "selected" : ""}`}
              key={t}
              onClick={() => theme(t)}
              aria-pressed={settings.theme === t}
            >
              <span className={`theme-swatch swatch-${t}`}>
                <i />
                <i />
                <i />
              </span>
              <strong>
                {
                  { light: "暖白工作台", dark: "暖灰深色", system: "跟随系统" }[
                    t
                  ]
                }
              </strong>
              <span>{settings.theme === t ? <Check size={16} /> : null}</span>
            </button>
          ))}
        </div>
      </Panel>
      <Panel title="采集与窗口" subtitle="数据在本机存储，定期读取来源变化">
        <div className="setting-row">
          <div>
            <strong>自动刷新间隔</strong>
            <p>刷新失败保留上次成功数据。</p>
          </div>
          <select
            aria-label="自动刷新间隔"
            value={settings.refreshSeconds}
            onChange={(e) =>
              perform(() =>
                save({ ...settings, refreshSeconds: Number(e.target.value) }),
              )
            }
          >
            {[30, 60, 120, 300, 600].map((v) => (
              <option value={v} key={v}>
                {v < 60 ? `${v} 秒` : `${v / 60} 分钟`}
              </option>
            ))}
          </select>
        </div>
        <div className="setting-row">
          <div>
            <strong>关闭窗口后留在托盘</strong>
            <p>通过托盘菜单打开工作台或完全退出。</p>
          </div>
          <input
            aria-label="关闭后留在托盘"
            type="checkbox"
            checked={settings.closeToTray}
            onChange={(e) =>
              perform(() =>
                save({ ...settings, closeToTray: e.target.checked }),
              )
            }
          />
        </div>
        <div className="setting-row">
          <div>
            <strong>数据目录</strong>
            <p className="mono break-all">
              {snapshot?.dataDirectory ?? "桌面版首次启动后创建"}
            </p>
          </div>
          <button
            className="button"
            disabled={!snapshot}
            onClick={() =>
              snapshot && perform(() => api.open(snapshot.dataDirectory))
            }
          >
            <FolderOpen size={16} />
            打开
          </button>
        </div>
      </Panel>
      <Panel title="键盘快捷键">
        <div className="shortcut-list">
          {[
            ["搜索与命令", "Ctrl / Cmd + K"],
            ["刷新数据", "Ctrl / Cmd + R"],
            ["切换主导航", "Ctrl / Cmd + 1–5"],
            ["关闭浮层", "Esc"],
            ["选择命令", "↑ / ↓，Enter"],
          ].map(([label, key]) => (
            <div key={label}>
              <span>{label}</span>
              <kbd>{key}</kbd>
            </div>
          ))}
        </div>
      </Panel>
    </div>
  );
}

function Launchers({ perform }: { perform: Perform }) {
  const { settings, save, patch } = useStore();
  const [query, setQuery] = useState(""),
    [editing, setEditing] = useState<LaunchEntry | null>(null),
    [recent, setRecent] = useState<string[]>([]);
  const launch = (id: string) =>
    perform(async () => {
      await command("launch_entry", { id });
      setRecent((r) => [id, ...r.filter((x) => x !== id)].slice(0, 6));
      patch({ notice: "启动请求已提交" });
    });
  return (
    <div className="page-flow">
      <Intro
        eyebrow="常用动作，就在手边"
        title="快速入口"
        description="把应用、项目目录和网站放在同一个地方。"
        action={
          <button
            className="button primary"
            onClick={() =>
              setEditing({
                id: crypto.randomUUID(),
                name: "",
                kind: "directory",
                target: "",
                args: [],
                cwd: "",
              })
            }
          >
            <Plus size={16} />
            添加入口
          </button>
        }
      />
      <Panel
        title="常用入口"
        action={
          <label className="search-field">
            <Search size={16} />
            <input
              aria-label="搜索入口"
              placeholder="搜索入口"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
        }
      >
        {settings.entries.length ? (
          <div className="launcher-grid">
            {settings.entries
              .filter((e) => e.name.toLowerCase().includes(query.toLowerCase()))
              .map((e) => (
                <div className="launcher" key={e.id}>
                  <button
                    className="launcher-main"
                    onClick={() => launch(e.id)}
                  >
                    <span className="launcher-icon">
                      {e.kind === "directory" ? (
                        <FolderOpen size={24} />
                      ) : e.kind === "application" ? (
                        <Terminal size={24} />
                      ) : (
                        <ArrowUpRight size={24} />
                      )}
                    </span>
                    <strong>{e.name}</strong>
                    <small title={e.target}>{e.target}</small>
                  </button>
                  <button
                    className="icon-button"
                    aria-label={`编辑 ${e.name}`}
                    onClick={() => setEditing(e)}
                  >
                    <MoreHorizontal size={18} />
                  </button>
                </div>
              ))}
          </div>
        ) : (
          <Empty
            title="把常用入口放在这里"
            description="应用、工作目录、文件和网站都可以登记。"
            action={
              <button
                className="button"
                onClick={() =>
                  setEditing({
                    id: crypto.randomUUID(),
                    name: "",
                    kind: "directory",
                    target: "",
                    args: [],
                    cwd: "",
                  })
                }
              >
                <Plus size={16} />
                添加第一个入口
              </button>
            }
          />
        )}
      </Panel>
      <Panel title="最近运行" subtitle="本次打开工作台期间">
        {recent.length ? (
          <div className="quick-actions">
            {recent.map((id) => {
              const e = settings.entries.find((e) => e.id === id);
              return e ? (
                <button
                  className="quick-action"
                  key={id}
                  onClick={() => launch(id)}
                >
                  <Zap size={17} />
                  <span>{e.name}</span>
                  <ArrowUpRight size={15} />
                </button>
              ) : null;
            })}
          </div>
        ) : (
          <p className="muted padded">尚未运行入口。</p>
        )}
      </Panel>
      <Modal
        open={!!editing}
        onClose={() => setEditing(null)}
        title="编辑快速入口"
        description="系统操作只在点击运行时执行"
      >
        {editing && (
          <form
            className="form-stack"
            onSubmit={(e) => {
              e.preventDefault();
              perform(async () => {
                await save({
                  ...settings,
                  entries: [
                    ...settings.entries.filter((x) => x.id !== editing.id),
                    editing,
                  ],
                });
                setEditing(null);
              });
            }}
          >
            <label>
              名称
              <input
                required
                value={editing.name}
                onChange={(e) =>
                  setEditing({ ...editing, name: e.target.value })
                }
              />
            </label>
            <label>
              类型
              <select
                value={editing.kind}
                onChange={(e) =>
                  setEditing({
                    ...editing,
                    kind: e.target.value as LaunchEntry["kind"],
                  })
                }
              >
                {[
                  ["directory", "目录"],
                  ["application", "应用"],
                  ["file", "文件"],
                  ["url", "网站"],
                ].map(([id, label]) => (
                  <option key={id} value={id}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              目标
              <input
                required
                value={editing.target}
                placeholder={editing.kind === "url" ? "https://…" : "完整路径"}
                onChange={(e) =>
                  setEditing({ ...editing, target: e.target.value })
                }
              />
            </label>
            {editing.kind === "application" && (
              <>
                <label>
                  参数（每行一个）
                  <textarea
                    value={editing.args.join("\n")}
                    onChange={(e) =>
                      setEditing({
                        ...editing,
                        args: e.target.value.split("\n").filter(Boolean),
                      })
                    }
                  />
                </label>
                <label>
                  工作目录
                  <input
                    value={editing.cwd}
                    onChange={(e) =>
                      setEditing({ ...editing, cwd: e.target.value })
                    }
                  />
                </label>
              </>
            )}
            <div className="modal-actions">
              {settings.entries.some((x) => x.id === editing.id) && (
                <button
                  type="button"
                  className="button danger"
                  onClick={() =>
                    perform(async () => {
                      await save({
                        ...settings,
                        entries: settings.entries.filter(
                          (x) => x.id !== editing.id,
                        ),
                      });
                      setEditing(null);
                    })
                  }
                >
                  <Trash2 size={16} />
                  移除入口
                </button>
              )}
              <button className="button primary" type="submit">
                保存入口
              </button>
            </div>
          </form>
        )}
      </Modal>
    </div>
  );
}

function Rename({ perform }: { perform: Perform }) {
  const [directory, setDirectory] = useState(""),
    [find, setFind] = useState(""),
    [replace, setReplace] = useState(""),
    [prefix, setPrefix] = useState(""),
    [suffix, setSuffix] = useState(""),
    [plan, setPlan] = useState<RenamePlan | null>(null),
    [confirm, setConfirm] = useState(false);
  useEffect(() => setPlan(null), [directory, find, replace, prefix, suffix]);
  const changed = plan?.rows.filter((r) => r.from !== r.to) ?? [];
  return (
    <div className="page-flow">
      <Intro
        eyebrow="先看结果，再改变文件"
        title="批量改名"
        description="处理目录中的文件，保留扩展名，自动检查覆盖与重名。"
      />
      <Panel title="改名规则" subtitle="仅处理选定目录下的文件，不递归子目录">
        <div className="form-stack">
          <label>
            目标目录
            <div className="input-action">
              <input
                value={directory}
                onChange={(e) => setDirectory(e.target.value)}
                placeholder="选择一个目录"
              />
              <button
                className="button"
                onClick={() =>
                  perform(async () => {
                    const paths = await chooseFile(true);
                    if (paths) setDirectory(paths[0]);
                  })
                }
              >
                <FolderOpen size={16} />
                选择目录
              </button>
            </div>
          </label>
          <div className="form-grid">
            {[
              ["查找", find, setFind],
              ["替换为", replace, setReplace],
              ["前缀", prefix, setPrefix],
              ["后缀", suffix, setSuffix],
            ].map(([label, value, setter]) => (
              <label key={String(label)}>
                {String(label)}
                <input
                  value={String(value)}
                  onChange={(e) =>
                    (setter as (v: string) => void)(e.target.value)
                  }
                />
              </label>
            ))}
          </div>
          <div className="button-row">
            <button
              className="button primary"
              disabled={!directory}
              onClick={() =>
                perform(async () =>
                  setPlan(
                    await command<RenamePlan>("rename_preview", {
                      directory,
                      find,
                      replace,
                      prefix,
                      suffix,
                    }),
                  ),
                )
              }
            >
              <Search size={16} />
              生成预览
            </button>
            <span className="muted">预览不会修改文件</span>
          </div>
        </div>
      </Panel>
      <Panel
        title="改名预览"
        subtitle={
          plan
            ? `${plan.rows.length} 个文件 · ${changed.length} 个待改变`
            : "规则改变后需要重新预览"
        }
        action={
          <button
            className="button"
            disabled={!changed.length || plan?.rows.some((r) => r.error)}
            onClick={() => setConfirm(true)}
          >
            应用改名
          </button>
        }
      >
        {plan ? (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>原名称</th>
                  <th>新名称</th>
                  <th>检查</th>
                </tr>
              </thead>
              <tbody>
                {plan.rows.map((r) => (
                  <tr key={r.from}>
                    <td>{r.from.split(/[\\/]/).at(-1)}</td>
                    <td>{r.to.split(/[\\/]/).at(-1)}</td>
                    <td className={r.error ? "danger-text" : "muted"}>
                      {r.error ?? (r.from === r.to ? "无变化" : "可以改名")}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <Empty
            title="预览你的文件名"
            description="选择目录并设置规则，检查结果后应用。"
          />
        )}
      </Panel>
      <Modal
        open={confirm}
        onClose={() => setConfirm(false)}
        title="确认应用改名"
        description={`将改变 ${changed.length} 个文件名；请确认上方预览。`}
      >
        <p>执行前会重新检查文件与目标。出现失败时尝试回滚并报告结果。</p>
        <div className="modal-actions">
          <button className="button" onClick={() => setConfirm(false)}>
            取消
          </button>
          <button
            className="button primary"
            onClick={() =>
              perform(async () => {
                const count = await command<number>("rename_apply", {
                  id: plan!.id,
                });
                setPlan(null);
                setConfirm(false);
                useStore.setState({ notice: `已完成 ${count} 个文件改名` });
              })
            }
          >
            确认改名
          </button>
        </div>
      </Modal>
    </div>
  );
}

function Scripts({ perform }: { perform: Perform }) {
  const { settings, save } = useStore();
  const [editing, setEditing] = useState<Script | null>(null),
    [confirm, setConfirm] = useState<Script | null>(null),
    [running, setRunning] = useState<string | null>(null),
    [result, setResult] = useState<ScriptResult | null>(null);
  return (
    <div className="page-flow">
      <Intro
        eyebrow="让重复工作少一点"
        title="脚本"
        description="登记自己的脚本，运行前确认路径与参数。"
        action={
          <button
            className="button primary"
            onClick={() =>
              setEditing({
                id: crypto.randomUUID(),
                name: "",
                path: "",
                interpreter: "powershell.exe",
                cwd: "",
                args: [],
                timeoutSeconds: 60,
              })
            }
          >
            <Plus size={16} />
            添加脚本
          </button>
        }
      />
      <Panel title="已登记脚本" subtitle="结构化传递参数；不拼接 Shell 命令">
        {settings.scripts.length ? (
          <div className="script-list">
            {settings.scripts.map((s) => (
              <div className="script-row" key={s.id}>
                <span className="launcher-icon">
                  <FileCode2 size={24} />
                </span>
                <div className="grow">
                  <strong>{s.name}</strong>
                  <small className="mono" title={s.path}>
                    {s.path}
                  </small>
                </div>
                <span className="muted">{s.timeoutSeconds}s</span>
                <button
                  className="icon-button"
                  aria-label={`编辑 ${s.name}`}
                  onClick={() => setEditing(s)}
                >
                  <Settings2 size={18} />
                </button>
                <button
                  className="button"
                  disabled={!!running}
                  onClick={() => setConfirm(s)}
                >
                  <Play size={15} />
                  {running === s.id ? "运行中…" : "运行"}
                </button>
              </div>
            ))}
          </div>
        ) : (
          <Empty
            title="登记第一个脚本"
            description="支持指定解释器、工作目录、参数和超时。"
          />
        )}
      </Panel>
      <Panel
        title="最近运行结果"
        subtitle={
          result
            ? `${result.status === "success" ? "执行成功" : "执行失败"} · 退出码 ${result.exitCode ?? "未知"} · ${dateTime(result.finishedAt)}`
            : "本次会话中的脚本输出"
        }
      >
        {result ? (
          <pre className="script-output">
            {result.output || "脚本未产生输出。"}
          </pre>
        ) : (
          <p className="muted padded">
            {running ? "脚本正在运行，等待返回结果。" : "尚未运行脚本。"}
          </p>
        )}
      </Panel>
      <Modal
        open={!!editing}
        onClose={() => setEditing(null)}
        title="登记脚本"
        description="选择你信任的本地脚本"
      >
        {editing && (
          <form
            className="form-stack"
            onSubmit={(e) => {
              e.preventDefault();
              perform(async () => {
                await save({
                  ...settings,
                  scripts: [
                    ...settings.scripts.filter((x) => x.id !== editing.id),
                    editing,
                  ],
                });
                setEditing(null);
              });
            }}
          >
            {[
              ["名称", "name"],
              ["脚本路径", "path"],
              ["解释器", "interpreter"],
              ["工作目录", "cwd"],
            ].map(([label, key]) => (
              <label key={key}>
                {label}
                <input
                  required={key !== "cwd"}
                  value={String(editing[key as keyof Script])}
                  onChange={(e) =>
                    setEditing({ ...editing, [key]: e.target.value })
                  }
                />
              </label>
            ))}
            <label>
              参数（每行一个）
              <textarea
                value={editing.args.join("\n")}
                onChange={(e) =>
                  setEditing({
                    ...editing,
                    args: e.target.value.split("\n").filter(Boolean),
                  })
                }
              />
            </label>
            <label>
              超时（秒）
              <input
                type="number"
                min="1"
                max="3600"
                required
                value={editing.timeoutSeconds}
                onChange={(e) =>
                  setEditing({
                    ...editing,
                    timeoutSeconds: Number(e.target.value),
                  })
                }
              />
            </label>
            <div className="modal-actions">
              {settings.scripts.some((x) => x.id === editing.id) && (
                <button
                  type="button"
                  className="button danger"
                  onClick={() =>
                    perform(async () => {
                      await save({
                        ...settings,
                        scripts: settings.scripts.filter(
                          (x) => x.id !== editing.id,
                        ),
                      });
                      setEditing(null);
                    })
                  }
                >
                  <Trash2 size={16} />
                  移除脚本
                </button>
              )}
              <button className="button primary" type="submit">
                保存脚本
              </button>
            </div>
          </form>
        )}
      </Modal>
      <Modal
        open={!!confirm}
        onClose={() => setConfirm(null)}
        title="确认运行脚本"
        description="脚本可以修改本机文件，请核对执行信息"
      >
        {confirm && (
          <>
            <dl className="detail-grid">
              {[
                ["脚本", confirm.path],
                ["解释器", confirm.interpreter],
                ["工作目录", confirm.cwd || "应用目录"],
                ["参数", confirm.args.join(" · ") || "无"],
              ].map(([label, value]) => (
                <div key={label}>
                  <dt>{label}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
            <div className="modal-actions">
              <button className="button" onClick={() => setConfirm(null)}>
                取消
              </button>
              <button
                className="button primary"
                onClick={() => {
                  const id = confirm.id;
                  setConfirm(null);
                  setRunning(id);
                  setResult(null);
                  perform(async () => {
                    try {
                      setResult(
                        await command<ScriptResult>("run_script", { id }),
                      );
                    } finally {
                      setRunning(null);
                    }
                  });
                }}
              >
                <Play size={15} />
                确认运行
              </button>
            </div>
          </>
        )}
      </Modal>
    </div>
  );
}
