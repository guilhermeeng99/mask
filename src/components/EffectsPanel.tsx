// Effects panel: preset tile grid + custom pitch/formant sliders.

import { useState } from "react";
import { strings } from "../lib/strings";
import { useEffectsStore } from "../stores/effects";
import { useToastStore } from "../stores/toasts";
import {
  Card,
  DangerButton,
  Field,
  focusRing,
  GhostButton,
  MonoValue,
  PanelHeader,
  Slider,
} from "./ui";

const s = strings.effects;

const PRESET_ICONS: Record<string, string> = {
  none: "🎤",
  "deep-voice": "🐻",
  "deeper-voice": "🌑",
  "high-voice": "🐦",
  woman: "👩",
  man: "👨",
  robot: "🤖",
  chipmunk: "🐿️",
  cave: "🕳️",
  radio: "📻",
};

export function EffectsPanel() {
  const store = useEffectsStore();
  const pushToast = useToastStore((t) => t.push);
  const [saving, setSaving] = useState(false);
  const [name, setName] = useState("");
  const active = store.presets.find((p) => p.id === store.activePresetId);
  const modified =
    active !== undefined &&
    (store.customPitch !== active.pitch || store.customFormant !== active.formant);

  async function selectPreset(id: string) {
    try {
      await store.setPreset(id);
    } catch (err) {
      pushToast("error", String(err));
    }
  }

  async function savePreset() {
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      await store.saveCustomAsPreset(trimmed);
      setSaving(false);
      setName("");
    } catch (err) {
      pushToast("error", String(err));
    }
  }

  return (
    <Card>
      <PanelHeader title={s.panelTitle} />
      <div className="grid grid-cols-5 gap-2">
        {store.presets.map((preset) => {
          const isActive = preset.id === store.activePresetId;
          return (
            <button
              key={preset.id}
              type="button"
              onClick={() => void selectPreset(preset.id)}
              className={`flex min-w-0 flex-col items-center gap-1 rounded-2xl px-2 py-3 transition ${
                isActive ? "bg-mask-soft ring-1 ring-mask" : "bg-raised hover:bg-overlay"
              } ${focusRing}`}
            >
              <span className="text-heading leading-none">{PRESET_ICONS[preset.id] ?? "🎭"}</span>
              <span
                className={`w-full truncate text-body font-semibold ${
                  isActive ? "text-mask-strong" : "text-text-dim"
                }`}
                title={preset.name}
              >
                {preset.name}
              </span>
            </button>
          );
        })}
      </div>

      <div className="grid grid-cols-2 gap-4">
        <Field label={s.pitch}>
          <div className="flex items-center gap-3">
            <Slider
              min={-12}
              max={12}
              step={0.5}
              bipolar
              value={store.customPitch}
              onChange={(v) => store.setParams(v, store.customFormant)}
            />
            <MonoValue>
              {store.customPitch > 0 ? "+" : ""}
              {store.customPitch} {s.semitonesUnit}
            </MonoValue>
          </div>
        </Field>
        <Field label={s.formant}>
          <div className="flex items-center gap-3">
            <Slider
              min={-12}
              max={12}
              step={0.5}
              bipolar
              value={store.customFormant}
              onChange={(v) => store.setParams(store.customPitch, v)}
            />
            <MonoValue>
              {store.customFormant > 0 ? "+" : ""}
              {store.customFormant} {s.semitonesUnit}
            </MonoValue>
          </div>
        </Field>
      </div>

      <div className="flex items-center gap-2">
        {saving ? (
          <>
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void savePreset();
                if (e.key === "Escape") setSaving(false);
              }}
              placeholder={active ? `${active.name} (custom)` : ""}
              className={`rounded-lg bg-raised px-3 py-1.5 text-body text-text ring-1 ring-outline ${focusRing}`}
            />
            <GhostButton onClick={() => void savePreset()}>{s.saveAsPreset}</GhostButton>
          </>
        ) : (
          <GhostButton onClick={() => setSaving(true)} disabled={!modified}>
            {s.saveAsPreset}
          </GhostButton>
        )}
        {active && !active.isBuiltin ? (
          <DangerButton
            label={s.deletePreset}
            confirmLabel={s.confirmDelete}
            onConfirm={() => {
              store.deletePreset(active.id).catch((err) => pushToast("error", String(err)));
            }}
          />
        ) : null}
      </div>
    </Card>
  );
}
