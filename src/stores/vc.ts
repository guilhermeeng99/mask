// AI voice conversion store: model library + activation state + companion
// download progress, driven by vc://* events.

import { create } from "zustand";
import { ipc, type VcBackendInfo, type VcImportArgs, type VcModel } from "../lib/ipc";

export interface CompanionProgress {
  file: string;
  fileIndex: number;
  fileCount: number;
  downloaded: number;
  total: number | null;
}

export type VcStatus =
  | { state: "inactive"; error?: string }
  | { state: "loading" }
  | { state: "active"; modelId: string }
  | { state: "fallback"; reason: string };

interface VcState {
  models: VcModel[];
  backend?: VcBackendInfo;
  status: VcStatus;
  pitchOffset: number;
  chunkMs: number;
  downloading: boolean;
  downloadProgress?: CompanionProgress;
  hydrate: () => Promise<void>;
  importModel: (args: VcImportArgs, licenseNote: string | null) => Promise<void>;
  importClone: (args: VcImportArgs) => Promise<void>;
  deleteModel: (id: string) => Promise<void>;
  downloadCompanions: () => Promise<void>;
  activate: (modelId: string) => Promise<void>;
  deactivate: () => Promise<void>;
  setPitchOffset: (v: number) => void;
  setChunkMs: (v: number) => void;
  applyStatus: (status: VcStatus) => void;
  applyDownloadProgress: (p: CompanionProgress) => void;
  applyCompanionsReady: () => void;
}

export const useVcStore = create<VcState>((set, get) => ({
  models: [],
  status: { state: "inactive" },
  pitchOffset: 0,
  chunkMs: 320,
  downloading: false,

  hydrate: async () => {
    const [models, backend] = await Promise.all([ipc.vcListModels(), ipc.vcBackendInfo()]);
    set({ models, backend });
  },

  importModel: async (args, licenseNote) => {
    const model = await ipc.vcImportModel({ ...args, licenseNote });
    set({ models: [...get().models, model] });
  },

  importClone: async (args) => {
    // The consent checkbox is enforced again backend-side (voice_cloning rule 1).
    const model = await ipc.vcImportClonedVoice({ ...args, consentConfirmed: true });
    set({ models: [...get().models, model] });
  },

  deleteModel: async (id) => {
    await ipc.vcDeleteModel(id);
    const status = get().status;
    set({
      models: get().models.filter((m) => m.id !== id),
      status: status.state === "active" && status.modelId === id ? { state: "inactive" } : status,
    });
  },

  downloadCompanions: async () => {
    set({ downloading: true });
    await ipc.vcDownloadCompanions();
  },

  activate: async (modelId) => {
    const { pitchOffset, chunkMs } = get();
    await ipc.vcActivate({ modelId, pitchOffset, chunkMs });
  },

  deactivate: async () => {
    await ipc.vcDeactivate();
  },

  setPitchOffset: (v) => set({ pitchOffset: v }),
  setChunkMs: (v) => set({ chunkMs: v }),

  applyStatus: (status) => set({ status }),
  applyDownloadProgress: (p) => set({ downloadProgress: p, downloading: true }),
  applyCompanionsReady: () => {
    set({ downloading: false, downloadProgress: undefined });
    void ipc.vcBackendInfo().then((backend) => set({ backend }));
  },
}));
