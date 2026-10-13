import { create } from "zustand";
import { api, desktop } from "./api";
import type { Page, Settings, Snapshot } from "./types";

export const initialSettings: Settings = {
  theme: "system",
  refreshSeconds: 60,
  closeToTray: true,
  sources: [],
  entries: [],
  scripts: [],
  skills: {
    centralPath: "~/.agents/skills",
    defaultSyncMode: "auto",
    allowCopy: false,
    repositories: [],
    projects: [],
    autoCheckUpdates: false,
    migration: {
      sourcePath: "~/.skills-manager/skills",
      status: "pending",
      backupPath: null,
    },
  },
};
interface State {
  page: Page;
  range: string;
  source: string;
  modelFilter: string;
  customStart: string;
  customEnd: string;
  settings: Settings;
  snapshot: Snapshot | null;
  loading: boolean;
  error: string | null;
  notice: string | null;
  palette: boolean;
  skillNames: { id: string; name: string }[];
  selectedSkillId: string | null;
  setPage: (page: Page) => void;
  patch: (value: Partial<State>) => void;
  initialize: () => Promise<void>;
  refresh: () => Promise<void>;
  save: (settings: Settings) => Promise<void>;
}
export const useStore = create<State>((set, get) => ({
  page: "overview",
  range: "7",
  source: "all",
  modelFilter: "all",
  customStart: "",
  customEnd: "",
  settings: initialSettings,
  snapshot: null,
  loading: false,
  error: null,
  notice: null,
  palette: false,
  skillNames: [],
  selectedSkillId: null,
  setPage: (page) => set({ page }),
  patch: (value) => set(value),
  initialize: async () => {
    if (!desktop) {
      set({ notice: "浏览器预览 · 本地数据采集和系统操作在桌面版中可用" });
      return;
    }
    try {
      const loaded = await api.settings();
      const settings = {
        ...loaded,
        skills: loaded.skills ?? initialSettings.skills,
      };
      const snapshot = await api.cached();
      set({
        settings,
        snapshot: snapshot
          ? {
              ...snapshot,
              sources: snapshot.sources.map((s) =>
                s.status === "ready" &&
                s.lastScan &&
                Date.now() - Date.parse(s.lastScan) >
                  settings.refreshSeconds * 3000
                  ? { ...s, status: "stale" }
                  : s,
              ),
            }
          : null,
      });
      await get().refresh();
    } catch (e) {
      set({
        error: String(e),
        loading: false,
        snapshot: get().snapshot
          ? {
              ...get().snapshot!,
              sources: get().snapshot!.sources.map((s) =>
                s.status === "ready" ? { ...s, status: "stale" } : s,
              ),
            }
          : null,
      });
    }
  },
  refresh: async () => {
    if (get().loading) return;
    set({ loading: true, error: null });
    try {
      set({ snapshot: await api.refresh(), loading: false });
    } catch (e) {
      set({
        error: String(e),
        loading: false,
        snapshot: get().snapshot
          ? {
              ...get().snapshot!,
              sources: get().snapshot!.sources.map((s) =>
                s.status === "ready" ? { ...s, status: "stale" } : s,
              ),
            }
          : null,
      });
    }
  },
  save: async (settings) => {
    await api.save(settings);
    set({ settings, notice: "设置已保存" });
  },
}));
