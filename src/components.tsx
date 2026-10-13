import type { ReactNode } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Clock3,
  Database,
  Info,
  PauseCircle,
  X,
  ArrowUpRight,
} from "lucide-react";
import * as Dialog from "@radix-ui/react-dialog";
import type { SourceStatus } from "./types";
export const statusText: Record<string, string> = {
  stale: "数据过期",
  ready: "已更新",
  partial: "部分可用",
  error: "解析失败",
  empty: "等待采集",
  missing: "目录缺失",
  paused: "已暂停",
};
export function Status({ status }: { status: string }) {
  const Icon =
    status === "ready"
      ? CheckCircle2
      : status === "paused"
        ? PauseCircle
        : status === "error" || status === "missing"
          ? AlertCircle
          : Clock3;
  return (
    <span className={`status status-${status}`}>
      <Icon size={14} aria-hidden="true" />
      {statusText[status] ?? status}
    </span>
  );
}
export function AgentMark({
  agent,
  large = false,
}: {
  agent: string;
  large?: boolean;
}) {
  return (
    <span
      className={`agent-mark agent-${agent} ${large ? "large" : ""}`}
      aria-hidden="true"
    >
      {agent === "codex" ? (
        <span className="code-glyph">&gt;_</span>
      ) : agent === "claude" ? (
        <span className="claude-glyph">✳</span>
      ) : (
        <span className="dsh-glyph">D</span>
      )}
    </span>
  );
}
export function Panel({
  title,
  subtitle,
  action,
  children,
  className = "",
}: {
  title: string;
  subtitle?: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`panel ${className}`}>
      <div className="panel-heading">
        <div>
          <h2>{title}</h2>
          {subtitle && <p>{subtitle}</p>}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}
export function Empty({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <span className="empty-symbol">
        <Database size={26} aria-hidden="true" />
      </span>
      <h3>{title}</h3>
      <p>{description}</p>
      {action}
    </div>
  );
}
export function Modal({
  title,
  description,
  open,
  onClose,
  children,
  wide = false,
  error,
}: {
  title: string;
  description: string;
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
  error?: string | null;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={(value) => !value && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="overlay" />
        <Dialog.Content className={`modal ${wide ? "wide" : ""}`}>
          <div className="modal-header">
            <div>
              <Dialog.Title>{title}</Dialog.Title>
              <Dialog.Description>{description}</Dialog.Description>
            </div>
            <Dialog.Close className="icon-button" aria-label="关闭">
              <X size={20} />
            </Dialog.Close>
          </div>
          {error && (
            <p className="inline-warning" role="alert">
              {error}
            </p>
          )}
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export function SourceHealth({
  sources,
  onDetail,
}: {
  sources: SourceStatus[];
  onDetail: (source: SourceStatus) => void;
}) {
  return (
    <div className="source-health">
      {sources.map((s) => (
        <button className="source-row" key={s.id} onClick={() => onDetail(s)}>
          <AgentMark agent={s.id} />
          <span className="grow">
            <strong>{s.name}</strong>
            <small>{s.files} 个来源文件</small>
          </span>
          <Status status={s.status} />
          <ArrowUpRight size={15} aria-hidden="true" />
        </button>
      ))}
      {sources.length === 0 && (
        <Empty
          title="尚未连接数据源"
          description="连接本地客户端后，这里会显示采集状态。"
        />
      )}
      <div className="privacy-note">
        <Info size={15} aria-hidden="true" />
        数据保留在本机，不采集对话正文。
      </div>
    </div>
  );
}
