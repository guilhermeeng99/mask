// Tauri event subscriptions → store updates. Called once at app mount.

import { listen } from "@tauri-apps/api/event";
import { usePipelineStore } from "../stores/pipeline";
import { useSoundboardStore } from "../stores/soundboard";
import { useVcStore, type VcStatus } from "../stores/vc";
import type { PipelineMetrics, PlayingClip } from "./ipc";

interface PipelineStateEvent {
  state: "stopped" | "starting" | "running" | "error";
  message?: string;
}

export function subscribeToBackendEvents(): () => void {
  const unsubs: Promise<() => void>[] = [
    listen<PipelineMetrics>("pipeline://metrics", (e) => {
      usePipelineStore.getState().applyMetrics(e.payload);
    }),
    listen<PipelineStateEvent>("pipeline://state", (e) => {
      usePipelineStore.getState().applyStateEvent(e.payload.state, e.payload.message);
    }),
    listen<PlayingClip[]>("soundboard://playing", (e) => {
      useSoundboardStore.getState().applyPlaying(e.payload);
    }),
    listen<VcStatus & { modelId?: string; reason?: string; error?: string }>("vc://state", (e) => {
      useVcStore.getState().applyStatus(e.payload);
    }),
    listen<{ file: string; downloaded: number; total: number | null }>(
      "vc://companion-progress",
      (e) => {
        useVcStore.getState().applyDownloadProgress(e.payload);
      },
    ),
    listen("vc://companions-ready", () => {
      useVcStore.getState().applyCompanionsReady();
    }),
  ];
  return () => {
    for (const unsub of unsubs) {
      void unsub.then((fn) => fn());
    }
  };
}
