import { useCallback, useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Info, LockKeyhole, Pencil, Plus, RefreshCw, Search, SlidersHorizontal, Trash2, UserRound, UsersRound } from "lucide-react";
import { desktop } from "../api";
import { Empty, Modal, Panel } from "../components";
import { environmentApi } from "../environment-api";
import type { EnvironmentChange, EnvironmentScope, EnvironmentSnapshot, EnvironmentValueKind, EnvironmentVariable } from "../environment-api";
import "./environment-variables.css";

const scopeName = { user: "用户变量", system: "系统变量" };
const kindName = { string: "字符串", expandString: "可展开字符串", unsupported: "不支持编辑" };
type Editor = { scope: EnvironmentScope; variable: EnvironmentVariable | null; deleting: boolean };

function VariableText({ value, expandable }: { value: string; expandable: boolean }) {
  if (!expandable) return <>{value}</>;
  return <>{value.split(/(%[^%\r\n]+%)/g).map((part, index) =>
    index % 2 === 1
      ? <span className="environment-reference" key={index}>{part}</span>
      : part,
  )}</>;
}

function VariableValue({ variable }: { variable: EnvironmentVariable }) {
  const { name, value, kind } = variable;
  const expandable = kind === "expandString";
  if (!value) return <span className="environment-empty-value">（空字符串）</span>;
  if (name.toLowerCase() === "path" && kind !== "unsupported") {
    const paths = value.split(";");
    const items = (values: string[]) => values.map((path, index) => <li key={index}>
      <span className="environment-path-text">{path
        ? <VariableText value={path} expandable={expandable} />
        : <span className="environment-empty-value">（空条目）</span>}
      </span>
    </li>);
    return <div className="environment-path-value">
      <ol className="environment-path-list">{items(paths.slice(0, 3))}</ol>
      {paths.length > 3 && <details className="environment-disclosure">
        <summary><ChevronDown size={14} aria-hidden="true" /><span className="when-closed">展开其余 {paths.length - 3} 项</span><span className="when-open">收起路径</span></summary>
        <ol className="environment-path-list" start={4}>{items(paths.slice(3))}</ol>
      </details>}
    </div>;
  }
  const long = value.length > 180 || value.split(/\r\n|\n|\r/).length > 3;
  return <div className="environment-value">
    <div className={`environment-value-text ${long ? "environment-value-preview" : ""}`}><VariableText value={value} expandable={expandable} /></div>
    {long && <details className="environment-disclosure">
      <summary><ChevronDown size={14} aria-hidden="true" /><span className="when-closed">查看完整值 · {value.length.toLocaleString()} 字符</span><span className="when-open">收起完整值</span></summary>
      <div className="environment-value-text"><VariableText value={value} expandable={expandable} /></div>
    </details>}
  </div>;
}

export function EnvironmentVariables() {
  const [snapshot, setSnapshot] = useState<EnvironmentSnapshot | null>(null);
  const [scope, setScope] = useState<EnvironmentScope>("user");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const mounted = useRef(false);
  const locked = useRef(false);
  const editorRef = useRef(editor);
  editorRef.current = editor;

  const refresh = useCallback(async () => {
    if (!desktop || locked.current || editorRef.current) return;
    locked.current = true;
    setBusy(true);
    setError(null);
    try {
      const next = await environmentApi.list();
      if (mounted.current) setSnapshot(old => ({ ...next, scopes: next.scopes.map(item => ({
        ...item,
        variables: item.error ? old?.scopes.find(previous => previous.scope === item.scope)?.variables ?? [] : item.variables,
      })) }));
    } catch (e) {
      if (mounted.current) setError(`读取失败：${String(e)}。可重试刷新。`);
    } finally {
      locked.current = false;
      if (mounted.current) setBusy(false);
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void refresh();
    const handler = () => void refresh();
    window.addEventListener("environment-refresh", handler);
    return () => { mounted.current = false; window.removeEventListener("environment-refresh", handler); };
  }, [refresh]);

  const current = snapshot?.scopes.find(item => item.scope === scope);
  const writable = !!current?.writable && !current.error && !error;
  const variables = current?.variables.filter(item => `${item.name}\n${item.value}`.toLocaleLowerCase().includes(query.toLocaleLowerCase())) ?? [];
  function openEditor(variable: EnvironmentVariable | null, deleting = false) {
    setFormError(null);
    setNotice(null);
    setEditor({ scope, variable, deleting });
  }
  async function save(change: EnvironmentChange) {
    if (locked.current) return;
    locked.current = true;
    setBusy(true);
    setFormError(null);
    try {
      const result = await environmentApi.change(change);
      if (!mounted.current) return;
      setSnapshot(old => old && ({ ...old, scopes: old.scopes.map(item => {
        if (item.scope !== change.scope) return item;
        const remaining = item.variables.filter(variable => variable.name.toLocaleLowerCase() !== change.name.toLocaleLowerCase());
        if (result.variable) remaining.push(result.variable);
        remaining.sort((a, b) => a.name.localeCompare(b.name));
        return { ...item, variables: remaining };
      }) }));
      setNotice(result.warning ?? `已${change.value === null ? "删除" : "保存"} ${scopeName[change.scope]} ${change.name}。请重新启动需要使用新变量的程序。`);
      setEditor(null);
    } catch (e) {
      if (mounted.current) setFormError(String(e));
    } finally {
      locked.current = false;
      if (mounted.current) setBusy(false);
    }
  }

  if (!desktop) return <Empty title="环境变量需要桌面环境" description="使用 Windows 桌面版查看和编辑本机环境变量。浏览器预览不会读取或修改电脑环境。" />;
  if (snapshot && !snapshot.supported) return <Empty title="当前系统暂不支持" description="环境变量编辑目前仅支持 Windows；尚未接入 macOS 或 Linux 的 Shell 配置文件。" />;

  return <div className="environment-page" aria-busy={busy}>
    <div className="environment-heading">
      <div className="environment-title"><span className="environment-title-icon"><SlidersHorizontal size={22} aria-hidden="true" /></span><div><h1>环境变量</h1><p>为开发工具配置路径与变量</p></div></div>
      <div className="environment-actions">
        <button className="button" disabled={busy || !!editor} onClick={() => void refresh()}><RefreshCw size={15} className={busy ? "environment-refreshing" : ""} aria-hidden="true" />刷新变量</button>
        <button className="button primary" disabled={busy || !writable} onClick={() => openEditor(null)}><Plus size={15} aria-hidden="true" />新增变量</button>
      </div>
    </div>
    {error && <p className="inline-warning" role="alert">{error}{snapshot ? " 保留上次读取结果，编辑已暂停。" : ""}</p>}
    {notice && <p className="environment-notice" role="status"><Check size={16} aria-hidden="true" />{notice}</p>}
    <div className="environment-toolbar">
      <div className="segmented" aria-label="变量作用域">
        {(["user", "system"] as const).map(id => {
          const count = snapshot?.scopes.find(item => item.scope === id)?.variables.length;
          const Icon = id === "user" ? UserRound : UsersRound;
          return <button key={id} className={scope === id ? "active" : ""} aria-label={scopeName[id]} aria-pressed={scope === id} disabled={busy} onClick={() => setScope(id)}><Icon size={16} aria-hidden="true" />{scopeName[id]}{count !== undefined && <span className="environment-scope-count" aria-hidden="true">{count}</span>}</button>;
        })}
      </div>
      <label className="environment-search"><Search size={16} aria-hidden="true" /><input aria-label="搜索环境变量" placeholder="搜索变量名或值…" value={query} onChange={e => setQuery(e.target.value)} /></label>
    </div>
    {current?.error && <p className="inline-warning" role="alert">{current.error}；当前作用域编辑已暂停，请刷新重试。</p>}
    {current && !current.writable && !current.error && <p className="environment-caption"><LockKeyhole size={15} aria-hidden="true" />{scope === "system" ? "系统变量为只读。如需编辑，请关闭工具箱后以管理员身份重新启动。" : "当前用户变量没有写入权限，请检查注册表访问权限。"}</p>}
    <Panel className="environment-panel" title={scopeName[scope]} subtitle={scope === "user" ? "仅影响当前用户的应用与终端" : "此电脑所有用户共享的环境配置"} action={<span className={`environment-access ${writable ? "writable" : ""}`}>{writable ? <Pencil size={13} aria-hidden="true" /> : <LockKeyhole size={13} aria-hidden="true" />}{writable ? "可编辑" : "只读"}</span>}>
      {busy && !snapshot ? <p className="environment-caption" role="status">正在读取环境变量…</p> : !current ? <Empty title="尚未读取变量" description="点击刷新重试读取本机环境变量。" /> : variables.length === 0 ? <Empty title={query ? "没有匹配的变量" : current.error ? "变量读取失败" : "暂无环境变量"} description={query ? "尝试其他变量名或值。" : current.error ? "修复读取错误后刷新重试。" : "可为当前作用域新增变量。"} /> : <div className="environment-table-wrap">
        <table className="environment-table"><thead><tr><th scope="col">变量名 / 类型</th><th scope="col">变量值</th><th scope="col"><span className="environment-result-count">{query ? `${variables.length} 个匹配` : `${variables.length} 个变量`}</span></th></tr></thead><tbody>
          {variables.map(variable => <tr key={variable.name}>
            <th scope="row"><span className="environment-variable-name">{variable.name || "（未命名值）"}</span><span className={`environment-kind ${variable.kind}`}>{kindName[variable.kind]}</span>{variable.name.toLowerCase() === "path" && variable.kind !== "unsupported" && <span className="environment-path-count">{variable.value.split(";").length} 个条目</span>}</th>
            <td><VariableValue variable={variable} /></td>
            <td><div className="environment-row-actions"><button className="environment-edit" aria-label={`编辑 ${variable.name}`} disabled={busy || !writable || variable.kind === "unsupported"} onClick={() => openEditor(variable)}><Pencil size={14} aria-hidden="true" /><span>编辑</span></button><button className="icon-button environment-delete" aria-label={`删除 ${variable.name}`} title={`删除 ${variable.name}`} disabled={busy || !writable || variable.kind === "unsupported"} onClick={() => openEditor(variable, true)}><Trash2 size={15} /></button></div></td>
          </tr>)}
        </tbody></table>
      </div>}
    </Panel>
    <p className="environment-caption environment-footnote"><Info size={16} aria-hidden="true" /><span>保存后，请重新启动需要使用新变量的终端或应用。<span>变量引用保留原样，Path 按原始顺序展示。</span></span></p>
    {editor && <VariableEditor editor={editor} busy={busy} error={formError} onClose={() => { if (!locked.current) setEditor(null); }} onSave={change => void save(change)} />}
  </div>;
}

function VariableEditor({ editor, busy, error, onClose, onSave }: {
  editor: Editor; busy: boolean; error: string | null; onClose: () => void; onSave: (change: EnvironmentChange) => void;
}) {
  const variable = editor.variable;
  const [name, setName] = useState(variable?.name ?? "");
  const [value, setValue] = useState(variable?.value ?? "");
  const [kind, setKind] = useState<EnvironmentValueKind>(variable?.kind ?? "expandString");
  const [lines, setLines] = useState(variable?.name.toLowerCase() === "path" && !/[\r\n]/.test(variable.value));
  const [validation, setValidation] = useState<string | null>(null);
  const isPath = name.toLowerCase() === "path";
  const canShowLines = !/[\r\n]/.test(value);
  function submit() {
    if (!name.trim() || /[=\x00-\x1f\x7f]/.test(name)) { setValidation("变量名不能为空，也不能包含等号或控制字符。"); return; }
    if (value.includes("\0")) { setValidation("变量值不能包含 NUL 字符。"); return; }
    if (!editor.deleting && name.length + value.length + 2 > 32767) { setValidation("变量名与值的总长度不能超过 32767 个 UTF-16 单元。"); return; }
    setValidation(null);
    onSave({ scope: editor.scope, name, value: editor.deleting ? null : value, kind, expectedRevision: variable?.revision ?? null });
  }
  return <Modal open title={editor.deleting ? `删除变量 ${name}` : variable ? `编辑变量 ${name}` : "新增环境变量"} description={`${scopeName[editor.scope]} · ${editor.deleting ? "删除后将无法由此变量读取原值，请核对后确认。" : "确认名称、原始值与类型后保存。已有变量名不可修改。"}`} error={validation ?? error} onClose={onClose} wide>
    <form className="form-stack environment-form" onSubmit={e => { e.preventDefault(); submit(); }}>
      <label>变量名<input className="environment-name-input" autoFocus={!variable} aria-label="变量名" value={name} readOnly={!!variable} disabled={busy} onChange={e => { setName(e.target.value); setLines(false); }} /></label>
      {isPath && !editor.deleting && <div className="segmented environment-value-mode" aria-label="Path 编辑方式"><button type="button" className={lines ? "active" : ""} aria-pressed={lines} disabled={busy || !canShowLines} onClick={() => setLines(true)}>每行一个路径</button><button type="button" className={!lines ? "active" : ""} aria-pressed={!lines} disabled={busy} onClick={() => setLines(false)}>原始值</button></div>}
      <label><span className="environment-field-heading">{lines ? "路径列表（保留顺序与空条目）" : "变量值"}<span>{lines ? `${value.split(";").length} 个条目` : `${value.length.toLocaleString()} 字符`}</span></span><textarea aria-label="变量值" className="mono" spellCheck={false} rows={lines ? 9 : 5} autoFocus={!!variable && !editor.deleting} readOnly={editor.deleting} disabled={busy} value={lines ? value.split(";").join("\n") : value} onChange={e => setValue(lines ? e.target.value.replace(/\r\n|\n|\r/g, ";") : e.target.value)} /></label>
      {!editor.deleting && <label>值类型<select aria-label="值类型" value={kind} disabled={busy} onChange={e => setKind(e.target.value as EnvironmentValueKind)}><option value="string">字符串（REG_SZ）</option><option value="expandString">可展开字符串（REG_EXPAND_SZ）</option></select><span>使用 %JAVA_HOME% 等变量引用时，选择可展开字符串。空值会保存为空字符串。</span></label>}
      {editor.scope === "system" && <p className="inline-warning">此次操作影响此电脑所有用户的环境变量。</p>}
      <div className="modal-actions"><button type="button" className="button" disabled={busy} onClick={onClose}>取消</button><button className={`button ${editor.deleting ? "danger" : "primary"}`} disabled={busy}>{busy ? "正在保存…" : editor.deleting ? "确认删除" : "保存修改"}</button></div>
    </form>
  </Modal>;
}
