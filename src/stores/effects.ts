// Effects store: preset list + active preset + live custom sliders.

import { create } from "zustand";
import { type DspPreset, ipc } from "../lib/ipc";

interface EffectsState {
  presets: DspPreset[];
  activePresetId: string;
  customPitch: number;
  customFormant: number;
  hydrate: () => Promise<void>;
  setPreset: (id: string) => Promise<void>;
  setParams: (pitch: number, formant: number) => void;
  saveCustomAsPreset: (name: string) => Promise<void>;
  deletePreset: (id: string) => Promise<void>;
}

export const useEffectsStore = create<EffectsState>((set, get) => ({
  presets: [],
  activePresetId: "none",
  customPitch: 0,
  customFormant: 0,

  hydrate: async () => {
    const [presets, config] = await Promise.all([ipc.dspListPresets(), ipc.configGet()]);
    const active = presets.find((p) => p.id === config.activePresetId);
    set({
      presets,
      activePresetId: active?.id ?? "none",
      customPitch: active?.pitch ?? 0,
      customFormant: active?.formant ?? 0,
    });
  },

  setPreset: async (id) => {
    await ipc.dspSetPreset(id);
    const preset = get().presets.find((p) => p.id === id);
    set({
      activePresetId: id,
      customPitch: preset?.pitch ?? 0,
      customFormant: preset?.formant ?? 0,
    });
  },

  // Live knobs are fire-and-forget; the backend clamps and smooths.
  setParams: (pitch, formant) => {
    set({ customPitch: pitch, customFormant: formant });
    void ipc.dspSetParams(pitch, formant);
  },

  saveCustomAsPreset: async (name) => {
    const { customPitch, customFormant, activePresetId, presets } = get();
    const base = presets.find((p) => p.id === activePresetId);
    const saved = await ipc.dspSavePreset({
      id: "",
      name,
      pitch: customPitch,
      formant: customFormant,
      effects: base?.effects ?? [],
      isBuiltin: false,
    });
    set({ presets: [...presets, saved], activePresetId: saved.id });
    await ipc.dspSetPreset(saved.id);
  },

  deletePreset: async (id) => {
    await ipc.dspDeletePreset(id);
    const presets = get().presets.filter((p) => p.id !== id);
    const wasActive = get().activePresetId === id;
    set({
      presets,
      // Backend already fell back to Clean (dsp spec edge case).
      activePresetId: wasActive ? "none" : get().activePresetId,
      ...(wasActive ? { customPitch: 0, customFormant: 0 } : {}),
    });
  },
}));
