// Pipeline state store: hydrated from config/devices at startup, then driven
// by pipeline://* events (app_shell.md). Components never own server state.

import { create } from "zustand";
import { type AudioDevice, ipc, type PipelineMetrics } from "../lib/ipc";

export type PipelineStatus = "stopped" | "starting" | "running" | "error";

interface PipelineState {
  status: PipelineStatus;
  errorMessage?: string;
  devices: AudioDevice[];
  selectedInputId?: string;
  selectedOutputId?: string;
  monitorDeviceId?: string;
  monitorEnabled: boolean;
  metrics: PipelineMetrics;
  hydrate: () => Promise<void>;
  rescanDevices: () => Promise<void>;
  selectInput: (id: string) => void;
  selectOutput: (id: string) => void;
  selectMonitor: (id: string | undefined) => void;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  applyStateEvent: (state: PipelineStatus, message?: string) => void;
  applyMetrics: (metrics: PipelineMetrics) => void;
}

const idleMetrics: PipelineMetrics = {
  inputPeakDb: -120,
  outputPeakDb: -120,
  latencyMs: 0,
  underruns: 0,
};

export const usePipelineStore = create<PipelineState>((set, get) => ({
  status: "stopped",
  devices: [],
  monitorEnabled: false,
  metrics: idleMetrics,

  hydrate: async () => {
    const [config, devices] = await Promise.all([ipc.configGet(), ipc.audioListDevices()]);
    const exists = (id: string | null, kind: "input" | "output") =>
      id && devices.some((d) => d.id === id && d.kind === kind) ? id : undefined;
    const virtualMic = devices.find((d) => d.isVirtualMic);
    set({
      devices,
      selectedInputId:
        exists(config.inputDeviceId, "input") ??
        devices.find((d) => d.kind === "input" && d.isDefault)?.id,
      // Saved device missing → stay stopped with no silent substitution
      // (audio_pipeline rule 9); a fresh config defaults to the virtual mic.
      selectedOutputId:
        exists(config.outputDeviceId, "output") ??
        (config.outputDeviceId ? undefined : virtualMic?.id),
      monitorDeviceId: exists(config.monitorDeviceId, "output"),
      monitorEnabled: config.monitorEnabled,
    });
  },

  rescanDevices: async () => {
    const devices = await ipc.audioListDevices();
    set({ devices });
  },

  selectInput: (id) => set({ selectedInputId: id }),
  selectOutput: (id) => set({ selectedOutputId: id }),
  selectMonitor: (id) => set({ monitorDeviceId: id, monitorEnabled: id !== undefined }),

  start: async () => {
    const { selectedInputId, selectedOutputId, monitorDeviceId, monitorEnabled } = get();
    if (!selectedInputId || !selectedOutputId) return;
    set({ status: "starting", errorMessage: undefined });
    try {
      await ipc.pipelineStart({
        inputDeviceId: selectedInputId,
        outputDeviceId: selectedOutputId,
        monitorDeviceId: monitorEnabled && monitorDeviceId ? monitorDeviceId : null,
      });
    } catch (err) {
      set({ status: "error", errorMessage: String(err) });
      throw err;
    }
  },

  stop: async () => {
    await ipc.pipelineStop();
    set({ metrics: idleMetrics });
  },

  applyStateEvent: (status, message) =>
    set({ status, errorMessage: status === "error" ? message : undefined }),
  applyMetrics: (metrics) => set({ metrics }),
}));
