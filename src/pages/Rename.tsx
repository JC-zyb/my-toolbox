import { useEffect, useMemo, useRef, useState } from "react";
import { FolderOpen, RotateCcw, Search } from "lucide-react";
import { chooseFile, command, desktop } from "../api";
import { Empty, Modal, Panel } from "../components";
import { defaultRenameRules, renameErrors } from "../rename";
import type {
  RenamePlan,
  RenameResult,
  RenameRules,
  RenameUndo,
} from "../types";
import "./rename.css";

const PAGE_SIZE = 100;
export function Rename() {
  const [directory, setDirectory] = useState("");
  const [rules, setRules] = useState<RenameRules>({ ...defaultRenameRules });
  const [plan, setPlan] = useState<RenamePlan | null>(null);
  const [selected, setSelected] = useState(new Set<string>());
  const [undo, setUndo] = useState<RenameUndo | null>(null);
  const [undoPlan, setUndoPlan] = useState<RenamePlan | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [view, setView] = useState("all");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const revision = useRef(0);
  const running = useRef(false);
  const errorRef = useRef<HTMLParagraphElement>(null);

  useEffect(() => {
    let active = true;
    if (desktop)
      void command<RenameUndo | null>("rename_history")
        .then((value) => {
          if (active) setUndo(value);
        })
        .catch((e) => {
          if (active) setError(String(e));
        });
    return () => {
      active = false;
      revision.current++;
    };
  }, []);
  useEffect(() => {
    if (error) errorRef.current?.focus();
  }, [error]);

  function invalidate() {
    revision.current++;
    setPlan(null);
    setConfirm(false);
    setError("");
    setNotice("");
    setPage(0);
  }
  function update<K extends keyof RenameRules>(key: K, value: RenameRules[K]) {
    invalidate();
    setRules((r) => ({ ...r, [key]: value }));
  }
  async function run(label: string, action: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(label);
    setError("");
    try {
      await action();
    } catch (e) {
      setError(String(e));
    } finally {
      running.current = false;
      setBusy("");
    }
  }
  const errors = useMemo(
    () => (plan ? renameErrors(plan, selected) : new Map<string, string>()),
    [plan, selected],
  );
  const changes = useMemo(
    () => plan?.rows.filter((r) => r.from !== r.to) ?? [],
    [plan],
  );
  const chosen = useMemo(
    () => changes.filter((r) => selected.has(r.from)),
    [changes, selected],
  );
  const filtered = useMemo(
    () =>
      plan?.rows.filter(
        (r) =>
          (view !== "changed" || r.from !== r.to) &&
          (view !== "conflict" || errors.has(r.from)) &&
          `${r.fromName}\n${r.toName}`
            .toLowerCase()
            .includes(search.toLowerCase()),
      ) ?? [],
    [plan, view, errors, search],
  );
  const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const currentPage = Math.min(page, pages - 1);
  const numericError =
    rules.mode === "template" &&
    (!Number.isSafeInteger(rules.start) ||
      rules.start < 0 ||
      !Number.isSafeInteger(rules.step) ||
      rules.step < 1 ||
      !Number.isInteger(rules.padding) ||
      rules.padding < 0 ||
      rules.padding > 18);
  const applyDisabled = !!busy || !chosen.length || errors.size > 0;

  return (
    <div className="page-flow rename-page" aria-busy={!!busy}>
      <div className="page-intro">
        <div>
          <span className="eyebrow">先看结果，再改变文件</span>
          <h1>
            批量改名<span className="heading-dot">.</span>
          </h1>
          <p>组合规则，逐项核对新名称，再确认执行。</p>
        </div>
        <button
          className="button"
          disabled={!undo || !!busy}
          onClick={() =>
            void run("检查撤销", async () => {
              setUndoPlan(
                await command<RenamePlan>("rename_undo_preview", {
                  id: undo!.id,
                }),
              );
            })
          }
        >
          <RotateCcw size={16} aria-hidden="true" />
          撤销最近一批{undo ? ` (${undo.count})` : ""}
        </button>
      </div>
      {error && (
        <p
          className="inline-warning rename-message"
          role="alert"
          ref={errorRef}
          tabIndex={-1}
        >
          {error}
        </p>
      )}
      {notice && (
        <p className="rename-message rename-success" role="status">
          {notice}
        </p>
      )}
      <Panel
        title="改名规则"
        subtitle="单个目录的普通文件，不递归、不处理链接、不覆盖批外目标"
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void run("生成预览", async () => {
              const current = ++revision.current;
              setPlan(null);
              setNotice("");
              const result = await command<RenamePlan>("rename_preview", {
                request: { directory, rules },
              });
              if (current === revision.current) {
                setPlan(result);
                setSelected(
                  new Set(
                    result.rows
                      .filter((r) => r.from !== r.to)
                      .map((r) => r.from),
                  ),
                );
                setPage(0);
                setView("all");
              }
            });
          }}
        >
          <fieldset disabled={!!busy} className="rename-fields form-stack">
            <div className="rename-directory">
              <label htmlFor="rename-directory">目标目录</label>
              <div className="input-action">
                <input
                  id="rename-directory"
                  value={directory}
                  onChange={(e) => {
                    invalidate();
                    setDirectory(e.target.value);
                  }}
                  placeholder="输入或选择一个目录"
                  required
                />
                <button
                  type="button"
                  className="button"
                  onClick={() =>
                    void run("选择目录", async () => {
                      const paths = await chooseFile(true);
                      if (paths) {
                        invalidate();
                        setDirectory(paths[0]);
                      }
                    })
                  }
                >
                  <FolderOpen size={16} aria-hidden="true" />
                  选择目录
                </button>
              </div>
            </div>
            <div className="form-grid">
              <label>
                改名方式
                <select
                  value={rules.mode}
                  onChange={(e) =>
                    update("mode", e.target.value as RenameRules["mode"])
                  }
                >
                  <option value="replace">替换文本</option>
                  <option value="template">命名模板与编号</option>
                </select>
              </label>
              <label>
                作用范围
                <select
                  value={rules.scope}
                  onChange={(e) =>
                    update("scope", e.target.value as RenameRules["scope"])
                  }
                >
                  <option value="stem">文件主名（保留扩展名）</option>
                  <option value="full">完整名称（包含扩展名）</option>
                  <option value="extension">仅扩展名（不含点）</option>
                </select>
              </label>
            </div>
            {rules.mode === "replace" ? (
              <>
                <div className="form-grid">
                  <label>
                    查找
                    <input
                      value={rules.find}
                      onChange={(e) => update("find", e.target.value)}
                      placeholder={
                        rules.regex ? "例如 ^IMG_(\\d+)$" : "留空可仅添加前后缀"
                      }
                      aria-describedby="rename-match-help"
                    />
                  </label>
                  <label>
                    替换为
                    <input
                      value={rules.replace}
                      onChange={(e) => update("replace", e.target.value)}
                      placeholder="留空表示删除匹配文本"
                    />
                  </label>
                </div>
                <div className="rename-checks">
                  <label>
                    <input
                      type="checkbox"
                      checked={rules.regex}
                      onChange={(e) => update("regex", e.target.checked)}
                    />
                    使用正则表达式
                  </label>
                  <label>
                    <input
                      type="checkbox"
                      checked={rules.caseSensitive}
                      onChange={(e) =>
                        update("caseSensitive", e.target.checked)
                      }
                    />
                    区分大小写
                  </label>
                  <label>
                    <input
                      type="checkbox"
                      checked={rules.replaceAll}
                      onChange={(e) => update("replaceAll", e.target.checked)}
                    />
                    替换全部匹配
                  </label>
                </div>
                <p className="rename-help" id="rename-match-help">
                  {rules.regex
                    ? "使用 Rust 正则：替换支持 $1、${name} 捕获组，字面 $ 写作 $$；不支持环视和反向引用。"
                    : "按原样匹配文本；取消“替换全部匹配”时，只替换首处。"}
                </p>
              </>
            ) : (
              <>
                <label>
                  命名模板
                  <input
                    value={rules.template}
                    onChange={(e) => update("template", e.target.value)}
                    aria-describedby="rename-template-help"
                    required
                  />
                </label>
                <p className="rename-help" id="rename-template-help">
                  {
                    "{name} 原主名 · {ext} 原扩展名（不含点）· {n} 编号。例如 照片_{n} → 照片_001.jpg。完整名称模式请在模板中自行包含扩展名。"
                  }
                </p>
                <div className="rename-numbering">
                  <label>
                    起始编号
                    <input
                      type="number"
                      min={0}
                      max={Number.MAX_SAFE_INTEGER}
                      step={1}
                      value={rules.start}
                      onChange={(e) => update("start", Number(e.target.value))}
                      aria-describedby={
                        numericError ? "rename-number-error" : undefined
                      }
                      aria-invalid={!!numericError}
                    />
                  </label>
                  <label>
                    编号步长
                    <input
                      type="number"
                      min={1}
                      max={Number.MAX_SAFE_INTEGER}
                      step={1}
                      value={rules.step}
                      onChange={(e) => update("step", Number(e.target.value))}
                      aria-describedby={
                        numericError ? "rename-number-error" : undefined
                      }
                      aria-invalid={!!numericError}
                    />
                  </label>
                  <label>
                    补零位数
                    <input
                      type="number"
                      min={0}
                      max={18}
                      step={1}
                      value={rules.padding}
                      onChange={(e) =>
                        update("padding", Number(e.target.value))
                      }
                      aria-describedby={
                        numericError ? "rename-number-error" : undefined
                      }
                      aria-invalid={!!numericError}
                    />
                  </label>
                </div>
                {numericError && (
                  <p
                    className="danger-text rename-help"
                    id="rename-number-error"
                    role="alert"
                  >
                    起始编号须为非负整数，步长至少为 1，补零位数为 0–18。
                  </p>
                )}
              </>
            )}
            <div className="form-grid">
              <label>
                前缀
                <input
                  value={rules.prefix}
                  onChange={(e) => update("prefix", e.target.value)}
                />
              </label>
              <label>
                后缀
                <input
                  value={rules.suffix}
                  onChange={(e) => update("suffix", e.target.value)}
                />
              </label>
              <label>
                大小写转换
                <select
                  value={rules.letterCase}
                  onChange={(e) =>
                    update(
                      "letterCase",
                      e.target.value as RenameRules["letterCase"],
                    )
                  }
                >
                  <option value="keep">保持原样</option>
                  <option value="lower">转为小写</option>
                  <option value="upper">转为大写</option>
                </select>
              </label>
              <div className="rename-checks">
                <label>
                  <input
                    type="checkbox"
                    checked={rules.trim}
                    onChange={(e) => update("trim", e.target.checked)}
                  />
                  去除首尾空白
                </label>
              </div>
            </div>
            <p className="rename-help">
              处理顺序：替换或模板 → 去首尾空白 → 大小写转换 →
              前后缀。所有规则只作用于所选范围。
            </p>
            <details className="rename-options">
              <summary>筛选与排序</summary>
              <div className="form-grid">
                <label>
                  名称包含
                  <input
                    value={rules.nameFilter}
                    onChange={(e) => update("nameFilter", e.target.value)}
                    placeholder="不区分大小写"
                  />
                </label>
                <label>
                  扩展名筛选
                  <input
                    value={rules.extensions}
                    onChange={(e) => update("extensions", e.target.value)}
                    placeholder="例如 jpg, png, txt；留空为全部"
                  />
                </label>
                <label>
                  排序方式
                  <select
                    value={rules.sort}
                    onChange={(e) =>
                      update("sort", e.target.value as RenameRules["sort"])
                    }
                  >
                    <option value="natural">名称自然排序（2 在 10 前）</option>
                    <option value="name">名称字典排序</option>
                    <option value="modified">修改时间（从早到晚）</option>
                  </select>
                </label>
                <div className="rename-checks">
                  <label>
                    <input
                      type="checkbox"
                      checked={rules.descending}
                      onChange={(e) => update("descending", e.target.checked)}
                    />
                    倒序
                  </label>
                  <label>
                    <input
                      type="checkbox"
                      checked={rules.includeHidden}
                      onChange={(e) =>
                        update("includeHidden", e.target.checked)
                      }
                    />
                    包含隐藏文件
                  </label>
                </div>
              </div>
              <p className="rename-help">
                编号在筛选与排序后生成；取消文件选择不会重新编号。
              </p>
            </details>
            <div className="button-row">
              <button
                className="button primary"
                type="submit"
                disabled={!directory.trim() || !!numericError}
              >
                <Search size={16} aria-hidden="true" />
                {busy === "生成预览" ? "正在生成…" : "生成预览"}
              </button>
              <button
                className="button"
                type="button"
                onClick={() => {
                  invalidate();
                  setRules({ ...defaultRenameRules });
                }}
              >
                重置规则
              </button>
              <span className="muted">预览不会修改文件</span>
            </div>
          </fieldset>
        </form>
      </Panel>
      <Panel
        title="改名预览"
        subtitle={
          plan
            ? `${plan.rows.length} 个文件 · ${changes.length} 个有变化 · 已选 ${chosen.length} 个 · ${errors.size} 个冲突 · 忽略 ${plan.skipped} 项`
            : "设置规则后生成预览；规则变化后需重新预览"
        }
        action={
          <button
            className="button primary"
            disabled={applyDisabled}
            onClick={() => setConfirm(true)}
          >
            应用改名{chosen.length ? ` (${chosen.length})` : ""}
          </button>
        }
      >
        {plan ? (
          <>
            <p className="rename-path mono" title={plan.directory}>
              {plan.directory}
            </p>
            <div className="rename-toolbar">
              <div className="rename-checks">
                <label>
                  <input
                    type="checkbox"
                    ref={(element) => {
                      if (element)
                        element.indeterminate =
                          chosen.length > 0 && chosen.length < changes.length;
                    }}
                    checked={
                      changes.length > 0 && chosen.length === changes.length
                    }
                    disabled={!changes.length || !!busy}
                    onChange={(e) =>
                      setSelected(
                        new Set(
                          e.target.checked ? changes.map((r) => r.from) : [],
                        ),
                      )
                    }
                  />
                  选择全部变化项
                </label>
              </div>
              <label className="sr-only" htmlFor="rename-view">
                预览筛选
              </label>
              <select
                id="rename-view"
                value={view}
                onChange={(e) => {
                  setView(e.target.value);
                  setPage(0);
                }}
              >
                <option value="all">全部文件</option>
                <option value="changed">仅有变化</option>
                <option value="conflict">仅冲突</option>
              </select>
              <label className="sr-only" htmlFor="rename-search">
                搜索预览
              </label>
              <input
                id="rename-search"
                placeholder="搜索原名称或新名称"
                value={search}
                onChange={(e) => {
                  setSearch(e.target.value);
                  setPage(0);
                }}
              />
            </div>
            {errors.size > 0 && (
              <p className="rename-help danger-text" role="status">
                所选文件有 {errors.size}{" "}
                个冲突，请修改规则或取消相关文件的选择。
              </p>
            )}
            {plan.rows.length === 0 ? (
              <Empty
                title="没有符合条件的文件"
                description="检查目录、名称与扩展名筛选；隐藏文件和链接默认不参与。"
              />
            ) : (
              <div className="table-wrap rename-table">
                <table>
                  <thead>
                    <tr>
                      <th>选择</th>
                      <th>原名称</th>
                      <th>新名称</th>
                      <th>检查结果</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filtered
                      .slice(
                        currentPage * PAGE_SIZE,
                        (currentPage + 1) * PAGE_SIZE,
                      )
                      .map((row) => (
                        <tr
                          key={row.from}
                          className={
                            errors.has(row.from) ? "rename-conflict" : ""
                          }
                        >
                          <td>
                            <input
                              type="checkbox"
                              aria-label={`选择 ${row.fromName}`}
                              disabled={row.from === row.to || !!busy}
                              checked={selected.has(row.from)}
                              onChange={(e) =>
                                setSelected((old) => {
                                  const next = new Set(old);
                                  if (e.target.checked) next.add(row.from);
                                  else next.delete(row.from);
                                  return next;
                                })
                              }
                            />
                          </td>
                          <td className="mono" title={row.from}>
                            {row.fromName}
                          </td>
                          <td className="mono" title={row.to}>
                            {row.toName || "（空名称）"}
                          </td>
                          <td
                            className={
                              errors.has(row.from) ? "danger-text" : "muted"
                            }
                          >
                            {errors.get(row.from) ??
                              (row.from === row.to
                                ? "无变化"
                                : selected.has(row.from)
                                  ? "可以改名"
                                  : "未选择")}
                          </td>
                        </tr>
                      ))}
                  </tbody>
                </table>
                {filtered.length === 0 && (
                  <p className="rename-help">没有符合当前预览筛选的文件。</p>
                )}
              </div>
            )}
            <div className="pagination">
              <span>
                {filtered.length} 项 · 第 {currentPage + 1} / {pages} 页
              </span>
              <button
                className="button"
                disabled={currentPage === 0}
                onClick={() => setPage(currentPage - 1)}
              >
                上一页
              </button>
              <button
                className="button"
                disabled={currentPage + 1 >= pages}
                onClick={() => setPage(currentPage + 1)}
              >
                下一页
              </button>
            </div>
          </>
        ) : (
          <Empty
            title="预览你的文件名"
            description="选择目录并设置规则，逐项检查结果后应用。"
          />
        )}
      </Panel>
      <p className="rename-help">
        最近一批改名可在本次应用会话中撤销；退出应用后记录清除。执行前会重新核对文件，失败会尝试恢复并报告结果。
      </p>
      <Modal
        open={confirm}
        onClose={() => {
          if (!busy) setConfirm(false);
        }}
        title="确认应用改名"
        description={`将改变 ${chosen.length} 个文件名。请核对目录和选择范围。`}
      >
        <p className="mono rename-confirm-path">{plan?.directory}</p>
        <p>仅执行已选择的变化项，执行前重新检查文件与目标；失败时尝试回滚。</p>
        {undo && <p>成功后将替换上一批的撤销记录。</p>}
        <ul className="rename-confirm-list">
          {chosen.slice(0, 20).map((r) => (
            <li key={r.from}>
              {r.fromName} → {r.toName}
            </li>
          ))}
        </ul>
        {chosen.length > 20 && (
          <p>另有 {chosen.length - 20} 项，完整列表见改名预览。</p>
        )}
        <div className="modal-actions">
          <button
            className="button"
            disabled={!!busy}
            onClick={() => setConfirm(false)}
          >
            取消
          </button>
          <button
            className="button primary"
            disabled={applyDisabled}
            onClick={() =>
              void run("正在改名", async () => {
                try {
                  const result = await command<RenameResult>("rename_apply", {
                    id: plan!.id,
                    selected: chosen.map((r) => r.from),
                  });
                  setUndo(result.undo);
                  setNotice(
                    `已完成 ${result.count} 个文件改名，可撤销最近一批。`,
                  );
                } finally {
                  setPlan(null);
                  setConfirm(false);
                }
              })
            }
          >
            {busy === "正在改名" ? "正在改名…" : "确认改名"}
          </button>
        </div>
      </Modal>
      <Modal
        open={!!undoPlan}
        onClose={() => {
          if (!busy) setUndoPlan(null);
        }}
        title="确认撤销改名"
        description={`恢复最近一批 ${undoPlan?.rows.length ?? 0} 个文件的原名称。`}
      >
        <p className="mono rename-confirm-path">{undoPlan?.directory}</p>
        <p>文件被修改或原名称被占用时，将拒绝撤销并说明原因。</p>
        <ul className="rename-confirm-list">
          {undoPlan?.rows.slice(0, 20).map((r) => (
            <li key={r.from}>
              {r.fromName} → {r.toName}
            </li>
          ))}
        </ul>
        {undoPlan && undoPlan.rows.length > 20 && (
          <p>
            另有 {undoPlan.rows.length - 20}{" "}
            项，将一并恢复为这一批改名前的名称。
          </p>
        )}
        <div className="modal-actions">
          <button
            className="button"
            disabled={!!busy}
            onClick={() => setUndoPlan(null)}
          >
            取消
          </button>
          <button
            className="button primary"
            disabled={!!busy}
            onClick={() =>
              void run("正在撤销", async () => {
                try {
                  const count = await command<number>("rename_undo", {
                    id: undoPlan!.id,
                  });
                  setUndo(null);
                  setPlan(null);
                  setNotice(`已恢复 ${count} 个文件的原名称。`);
                } finally {
                  setUndoPlan(null);
                }
              })
            }
          >
            {busy === "正在撤销" ? "正在撤销…" : "确认撤销"}
          </button>
        </div>
      </Modal>
    </div>
  );
}
