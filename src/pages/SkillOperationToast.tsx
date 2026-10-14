import { AlertCircle, CheckCircle2, Info, LoaderCircle, X } from "lucide-react";

export type SkillToastKind = "pending" | "success" | "error" | "confirm" | "info";
export interface SkillToastItem {
  id: string;
  kind: SkillToastKind;
  title: string;
  message?: string;
  actionLabel?: string;
  onAction?: () => void;
}

export function SkillOperationToast({ items, onDismiss }: { items: SkillToastItem[]; onDismiss: (id: string) => void }) {
  return <div className="skill-toast-stack" aria-label="Skill 操作通知">
    {items.map(item => {
      const Icon = item.kind === "pending" ? LoaderCircle : item.kind === "success" ? CheckCircle2 : item.kind === "error" ? AlertCircle : Info;
      return <div key={item.id} className={`skill-toast ${item.kind}`} role={item.kind === "error" ? "alert" : "status"}>
        <Icon size={17} aria-hidden="true" className={item.kind === "pending" ? "skill-toast-spin" : undefined} />
        <div className="skill-toast-copy"><strong>{item.title}</strong>{item.message && <span>{item.message}</span>}
          {item.actionLabel && item.onAction && <button className="text-button" onClick={item.onAction}>{item.actionLabel}</button>}
        </div>
        <button className="icon-button" aria-label={`关闭通知：${item.title}`} onClick={() => onDismiss(item.id)}><X size={15} /></button>
      </div>;
    })}
  </div>;
}
