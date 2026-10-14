import { useMemo, useRef, useSyncExternalStore } from "react";
import { SkillActivationQueue, type ActivationEvent } from "../skill-activation";
import { skillsApi } from "../skills-api";
import type { SkillActivationPreview } from "../types";
import type { SkillToastItem } from "./SkillOperationToast";

export function useSkillActivation(projectPath: string | null, allowCopy: boolean, notify: (item: Omit<SkillToastItem, "id">) => string, updateToast: (id: string, patch: Partial<SkillToastItem>) => void, onConfirm: (preview: SkillActivationPreview) => void) {
  const callbacks = useRef({ notify, updateToast, onConfirm });
  callbacks.current = { notify, updateToast, onConfirm };
  const queue = useMemo(() => {
    const toasts = new Map<number, string>();
    return new SkillActivationQueue(skillsApi.quickActivation, projectPath, allowCopy, (event: ActivationEvent) => {
      const { kind, name, enabled, message, preview } = event;
      const title = kind === "pending" ? `正在保存 ${name}` : kind === "error" ? `${name} 操作失败` : kind === "confirm" ? `${name} 需要确认` : enabled === null ? `${name} 状态已更新` : `${name} 已${enabled ? "启用" : "关闭"}`;
      const item = { kind, title, message, ...(preview ? { actionLabel: "查看预览", onAction: () => callbacks.current.onConfirm(preview) } : {}) };
      const id = toasts.get(event.operationId);
      if (id) callbacks.current.updateToast(id, item);
      else toasts.set(event.operationId, callbacks.current.notify(item));
      if (kind !== "pending") toasts.delete(event.operationId);
      if (preview) callbacks.current.onConfirm(preview);
    });
  }, [projectPath, allowCopy]);
  useSyncExternalStore(queue.subscribe, queue.snapshot);
  return queue;
}
