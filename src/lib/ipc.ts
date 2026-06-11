// Typed wrappers for every Tauri command (CLAUDE.md rule: components never
// call invoke directly). Types mirror the Rust serde shapes.

import { invoke } from "@tauri-apps/api/core";

export type DeviceKind = "input" | "output";

export interface AudioDevice {
  id: string;
  name: string;
  kind: DeviceKind;
  isDefault: boolean;
  isVirtualMic: boolean;
}

export type VirtualMicStatus =
  | { status: "notInstalled" }
  | { status: "installed"; device: AudioDevice }
  // Driver devnode exists but Windows refuses to start it (Device Manager
  // problem code, e.g. 52 = signature rejected). Restarting won't fix it.
  | { status: "blocked"; problemCode: number };

export interface PipelineConfig {
  inputDeviceId: string;
  outputDeviceId: string;
  monitorDeviceId: string | null;
}

export interface PipelineMetrics {
  inputPeakDb: number;
  outputPeakDb: number;
  latencyMs: number;
  underruns: number;
}

export type ExtraEffect =
  | { kind: "ringMod"; freqHz: number }
  | { kind: "distortion"; drive: number }
  | { kind: "reverb"; mix: number; decay: number }
  | { kind: "highPass"; cutoffHz: number }
  | { kind: "lowPass"; cutoffHz: number };

export interface DspPreset {
  id: string;
  name: string;
  pitch: number;
  formant: number;
  effects: ExtraEffect[];
  isBuiltin: boolean;
}

export interface SoundClip {
  id: string;
  name: string;
  sourcePath: string;
  durationMs: number;
  volume: number;
  sortIndex: number;
  createdAt: string;
}

export interface PlayingClip {
  id: string;
  positionMs: number;
}

export interface AppConfig {
  version: number;
  inputDeviceId: string | null;
  outputDeviceId: string | null;
  monitorDeviceId: string | null;
  monitorEnabled: boolean;
  activePresetId: string;
  userPresets: DspPreset[];
  onboardingDone: boolean;
}

export interface VcModel {
  id: string;
  name: string;
  onnxPath: string;
  indexPath: string | null;
  defaultPitch: number;
  sampleRate: number;
  licenseNote: string;
  createdAt: string;
}

export interface VcSettings {
  modelId: string;
  pitchOffset: number;
  chunkMs: number;
}

export type InferenceBackend = "directMl" | "cuda" | "cpu";

export interface VcBackendInfo {
  backend: InferenceBackend;
  companionsInstalled: boolean;
}

export interface VcImportArgs {
  onnx: string;
  index: string | null;
  name: string;
  defaultPitch: number;
  sampleRate: number;
}

export const ipc = {
  audioListDevices: () => invoke<AudioDevice[]>("audio_list_devices"),
  virtualMicStatus: () => invoke<VirtualMicStatus>("virtual_mic_status"),
  pipelineStart: (config: PipelineConfig) => invoke<void>("pipeline_start", { config }),
  pipelineStop: () => invoke<void>("pipeline_stop"),

  dspListPresets: () => invoke<DspPreset[]>("dsp_list_presets"),
  dspSetPreset: (id: string) => invoke<void>("dsp_set_preset", { id }),
  dspSetParams: (pitch: number, formant: number) =>
    invoke<void>("dsp_set_params", { pitch, formant }),
  dspSavePreset: (preset: DspPreset) => invoke<DspPreset>("dsp_save_preset", { preset }),
  dspDeletePreset: (id: string) => invoke<void>("dsp_delete_preset", { id }),

  soundboardList: () => invoke<SoundClip[]>("soundboard_list"),
  soundboardImport: (paths: string[]) => invoke<SoundClip[]>("soundboard_import", { paths }),
  soundboardPlay: (id: string) => invoke<void>("soundboard_play", { id }),
  soundboardStop: (id: string) => invoke<void>("soundboard_stop", { id }),
  soundboardStopAll: () => invoke<void>("soundboard_stop_all"),
  soundboardUpdate: (clip: SoundClip) => invoke<void>("soundboard_update", { clip }),
  soundboardDelete: (id: string) => invoke<void>("soundboard_delete", { id }),

  virtualMicInstall: () => invoke<void>("virtual_mic_install"),
  configGet: () => invoke<AppConfig>("config_get"),
  onboardingComplete: () => invoke<void>("onboarding_complete"),

  vcBackendInfo: () => invoke<VcBackendInfo>("vc_backend_info"),
  vcListModels: () => invoke<VcModel[]>("vc_list_models"),
  vcImportModel: (args: VcImportArgs & { licenseNote: string | null }) =>
    invoke<VcModel>("vc_import_model", { ...args }),
  vcImportClonedVoice: (args: VcImportArgs & { consentConfirmed: boolean }) =>
    invoke<VcModel>("vc_import_cloned_voice", { ...args }),
  vcDeleteModel: (id: string) => invoke<void>("vc_delete_model", { id }),
  vcDownloadCompanions: () => invoke<void>("vc_download_companions"),
  vcActivate: (settings: VcSettings) => invoke<void>("vc_activate", { settings }),
  vcDeactivate: () => invoke<void>("vc_deactivate"),
};
