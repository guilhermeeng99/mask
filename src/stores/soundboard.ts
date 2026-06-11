// Soundboard store: clip library + playing positions (from events).

import { create } from "zustand";
import { ipc, type PlayingClip, type SoundClip } from "../lib/ipc";

interface SoundboardState {
  clips: SoundClip[];
  playing: PlayingClip[];
  hydrate: () => Promise<void>;
  importPaths: (paths: string[]) => Promise<void>;
  play: (id: string) => Promise<void>;
  stop: (id: string) => Promise<void>;
  stopAll: () => Promise<void>;
  update: (clip: SoundClip) => Promise<void>;
  remove: (id: string) => Promise<void>;
  applyPlaying: (playing: PlayingClip[]) => void;
}

export const useSoundboardStore = create<SoundboardState>((set, get) => ({
  clips: [],
  playing: [],

  hydrate: async () => {
    set({ clips: await ipc.soundboardList() });
  },

  importPaths: async (paths) => {
    if (paths.length === 0) return;
    const imported = await ipc.soundboardImport(paths);
    set({ clips: [...get().clips, ...imported] });
  },

  play: async (id) => {
    await ipc.soundboardPlay(id);
  },

  stop: async (id) => {
    await ipc.soundboardStop(id);
  },

  stopAll: async () => {
    await ipc.soundboardStopAll();
  },

  update: async (clip) => {
    await ipc.soundboardUpdate(clip);
    set({ clips: get().clips.map((c) => (c.id === clip.id ? clip : c)) });
  },

  remove: async (id) => {
    await ipc.soundboardDelete(id);
    set({ clips: get().clips.filter((c) => c.id !== id) });
  },

  applyPlaying: (playing) => set({ playing }),
}));
