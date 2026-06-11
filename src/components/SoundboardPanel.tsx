// Soundboard: clip tile grid, import, per-clip menu, stop all.

import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import type { SoundClip } from "../lib/ipc";
import { strings } from "../lib/strings";
import { usePipelineStore } from "../stores/pipeline";
import { useSoundboardStore } from "../stores/soundboard";
import { useToastStore } from "../stores/toasts";
import {
  Card,
  DangerButton,
  EmptyState,
  focusRing,
  GhostButton,
  MonoValue,
  PanelHeader,
  Slider,
} from "./ui";

const s = strings.soundboard;

function formatDuration(ms: number): string {
  const totalSecs = Math.round(ms / 1000);
  const m = Math.floor(totalSecs / 60);
  const sec = totalSecs % 60;
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

export function SoundboardPanel() {
  const store = useSoundboardStore();
  const pipelineRunning = usePipelineStore((p) => p.status === "running");
  const pushToast = useToastStore((t) => t.push);
  const [editing, setEditing] = useState<SoundClip | null>(null);

  async function importClips() {
    try {
      const picked = await open({
        multiple: true,
        filters: [{ name: s.importFilterName, extensions: ["wav", "mp3", "ogg", "flac"] }],
      });
      const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
      await store.importPaths(paths);
    } catch (err) {
      pushToast("error", String(err));
    }
  }

  function play(id: string) {
    store.play(id).catch((err) => pushToast("error", String(err)));
  }

  const anyPlaying = store.playing.length > 0;

  return (
    <Card className="grow">
      <PanelHeader
        title={s.panelTitle}
        action={
          <div className="flex gap-2">
            <GhostButton onClick={() => void store.stopAll()} disabled={!anyPlaying}>
              {s.stopAll}
            </GhostButton>
            <GhostButton onClick={() => void importClips()}>{s.import}</GhostButton>
          </div>
        }
      />

      {store.clips.length === 0 ? (
        <EmptyState title={s.empty} hint={s.emptyHint} />
      ) : (
        <div className="grid max-h-72 grid-cols-4 gap-2 overflow-y-auto pr-1">
          {store.clips.map((clip) => {
            const playing = store.playing.find((p) => p.id === clip.id);
            const progress = playing ? Math.min(1, playing.positionMs / clip.durationMs) : 0;
            return (
              <button
                key={clip.id}
                type="button"
                onClick={() => play(clip.id)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setEditing(clip);
                }}
                disabled={!pipelineRunning}
                title={pipelineRunning ? clip.name : strings.devices.noInputSelected}
                className={`relative flex min-w-24 flex-col items-start gap-0.5 overflow-hidden rounded-2xl px-3 py-2.5 text-left transition disabled:opacity-40 ${
                  playing ? "bg-mask-soft ring-1 ring-mask" : "bg-raised hover:bg-overlay"
                } ${focusRing}`}
              >
                <span className="w-full truncate text-body font-semibold text-text">
                  {playing ? "■ " : ""}
                  {clip.name}
                </span>
                <MonoValue>{formatDuration(clip.durationMs)}</MonoValue>
                {playing ? (
                  <span
                    className="absolute inset-x-0 bottom-0 h-0.5 bg-live transition-[width]"
                    style={{ width: `${progress * 100}%` }}
                  />
                ) : null}
              </button>
            );
          })}
        </div>
      )}

      {editing ? (
        <ClipEditor
          clip={editing}
          onClose={() => setEditing(null)}
          onSave={(clip) => {
            store.update(clip).catch((err) => pushToast("error", String(err)));
            setEditing(null);
          }}
          onDelete={(id) => {
            store.remove(id).catch((err) => pushToast("error", String(err)));
            setEditing(null);
          }}
        />
      ) : null}
    </Card>
  );
}

function ClipEditor({
  clip,
  onClose,
  onSave,
  onDelete,
}: {
  clip: SoundClip;
  onClose: () => void;
  onSave: (clip: SoundClip) => void;
  onDelete: (id: string) => void;
}) {
  const [name, setName] = useState(clip.name);
  const [volume, setVolume] = useState(clip.volume);
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: scrim click-to-close is a pointer-only affordance; Escape covers keyboard users
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-ink/70"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
      role="presentation"
    >
      <div
        className="flex w-80 flex-col gap-4 rounded-2xl bg-surface p-5 shadow-pop"
        role="dialog"
        aria-label={clip.name}
      >
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          className={`rounded-lg bg-raised px-3 py-1.5 text-body-lg text-text ring-1 ring-outline ${focusRing}`}
        />
        <div>
          <p className="mb-1.5 text-body font-semibold uppercase tracking-wide text-text-dim">
            {s.volume}
          </p>
          <div className="flex items-center gap-3">
            <Slider min={0} max={2} step={0.05} value={volume} onChange={setVolume} />
            <MonoValue>{Math.round(volume * 100)}%</MonoValue>
          </div>
        </div>
        <div className="flex items-center justify-between">
          <DangerButton
            label={s.delete}
            confirmLabel={strings.effects.confirmDelete}
            onConfirm={() => onDelete(clip.id)}
          />
          <GhostButton onClick={() => onSave({ ...clip, name: name.trim() || clip.name, volume })}>
            {s.rename}
          </GhostButton>
        </div>
      </div>
    </div>
  );
}
