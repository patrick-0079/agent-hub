/** 全局状态：路由、扫描快照、设置、任务日志。 */

import { create } from "zustand";
import { api, describeError, onScanProgress } from "./api";
import type { AppSettings, HostInfo, Progress, Route, ScanSnapshot } from "./types";

const MAX_LOGS = 400;

interface AppState {
  route: Route;
  ready: boolean;
  scanning: boolean;
  host: HostInfo | null;
  settings: AppSettings | null;
  snapshot: ScanSnapshot | null;
  logs: Progress[];
  phase: string;
  banner: string | null;

  init: () => Promise<void>;
  navigate: (route: Route) => void;
  scan: () => Promise<void>;
  patchSettings: (patch: Partial<AppSettings>) => Promise<void>;
  pushLog: (p: Progress) => void;
  clearLogs: () => void;
  setBanner: (message: string | null) => void;
}

let initStarted = false;

export const useApp = create<AppState>((set, get) => ({
  route: "dashboard",
  ready: false,
  scanning: false,
  host: null,
  settings: null,
  snapshot: null,
  logs: [],
  phase: "",
  banner: null,

  init: async () => {
    if (initStarted) return;
    initStarted = true;

    onScanProgress((p) => get().pushLog(p));

    try {
      const [host, settings, snapshot] = await Promise.all([
        api.appInfo(),
        api.getSettings(),
        api.lastSnapshot(),
      ]);
      set({ host, settings, snapshot, ready: true });
      if (settings.autoScanOnStart && !snapshot) {
        void get().scan();
      }
    } catch (error) {
      set({ ready: true, banner: describeError(error) });
    }
  },

  navigate: (route) => set({ route }),

  scan: async () => {
    if (get().scanning) return;
    set({ scanning: true, phase: "准备中", logs: [], banner: null });
    try {
      const snapshot = await api.runScan();
      set({ snapshot, scanning: false, phase: "" });
    } catch (error) {
      set({ scanning: false, phase: "", banner: describeError(error) });
    }
  },

  patchSettings: async (patch) => {
    const current = get().settings;
    if (!current) return;
    const next = { ...current, ...patch };
    set({ settings: next });
    try {
      const saved = await api.saveSettings(next);
      set({ settings: saved });
    } catch (error) {
      set({ settings: current, banner: describeError(error) });
    }
  },

  pushLog: (p) =>
    set((state) => {
      const logs = [...state.logs, p];
      if (logs.length > MAX_LOGS) logs.splice(0, logs.length - MAX_LOGS);
      return { logs, phase: p.level === "done" ? "" : p.phase };
    }),

  clearLogs: () => set({ logs: [] }),
  setBanner: (banner) => set({ banner }),
}));