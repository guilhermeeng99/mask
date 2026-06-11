// AI voices (phase 2): companion setup, model list, activation, latency knob.
// Clone imports are consent-gated (phase 3, voice_cloning rule 1).

import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { strings } from "../lib/strings";
import { usePipelineStore } from "../stores/pipeline";
import { useToastStore } from "../stores/toasts";
import { useVcStore } from "../stores/vc";
import {
  Card,
  DangerButton,
  Field,
  focusRing,
  GhostButton,
  MonoValue,
  PanelHeader,
  PrimaryButton,
  Slider,
  Spinner,
} from "./ui";

const s = strings.vc;

export function VoicesPanel() {
  const store = useVcStore();
  const pipelineRunning = usePipelineStore((p) => p.status === "running");
  const pushToast = useToastStore((t) => t.push);
  const [importing, setImporting] = useState<"model" | "clone" | null>(null);

  if (!store.backend) return null;

  if (!store.backend.companionsInstalled) {
    return (
      <Card>
        <PanelHeader title={s.panelTitle} action={<BackendBadge />} />
        <p className="text-body text-text-dim">{s.setupHint}</p>
        {store.downloading ? (
          <div className="flex items-center gap-3">
            <Spinner />
            <DownloadProgress />
          </div>
        ) : (
          <PrimaryButton
            className="self-start"
            onClick={() => store.downloadCompanions().catch((e) => pushToast("error", String(e)))}
          >
            {s.setup}
          </PrimaryButton>
        )}
      </Card>
    );
  }

  const activeId = store.status.state === "active" ? store.status.modelId : null;

  return (
    <Card>
      <PanelHeader
        title={s.panelTitle}
        action={
          <div className="flex items-center gap-2">
            <BackendBadge />
            <GhostButton onClick={() => setImporting("model")}>{s.importModel}</GhostButton>
            <GhostButton onClick={() => setImporting("clone")}>{s.importClone}</GhostButton>
          </div>
        }
      />

      {store.status.state === "fallback" ? (
        <p className="text-body text-warn">{s.fallback}</p>
      ) : null}

      {store.models.length === 0 ? (
        <p className="text-body text-text-faint">
          {s.empty}. {s.emptyHint}.
        </p>
      ) : (
        <div className="flex flex-col gap-1.5">
          {store.models.map((model) => {
            const isActive = model.id === activeId;
            const isLoading = store.status.state === "loading";
            return (
              <div
                key={model.id}
                className={`flex items-center gap-3 rounded-lg px-3 py-2 ${
                  isActive ? "bg-mask-soft ring-1 ring-mask" : "bg-raised"
                }`}
              >
                <div className="min-w-0 grow">
                  <p className="truncate text-body font-semibold text-text">{model.name}</p>
                  <p className="truncate text-body text-text-faint" title={model.licenseNote}>
                    {model.licenseNote}
                  </p>
                </div>
                <MonoValue>{model.sampleRate / 1000}k</MonoValue>
                {isActive ? (
                  <GhostButton
                    onClick={() => store.deactivate().catch((e) => pushToast("error", String(e)))}
                  >
                    {s.deactivate}
                  </GhostButton>
                ) : (
                  <GhostButton
                    disabled={!pipelineRunning || isLoading}
                    title={!pipelineRunning ? s.needsPipeline : undefined}
                    onClick={() =>
                      store.activate(model.id).catch((e) => pushToast("error", String(e)))
                    }
                  >
                    {isLoading ? s.loading : s.activate}
                  </GhostButton>
                )}
                <DangerButton
                  label={s.deleteModel}
                  confirmLabel={strings.effects.confirmDelete}
                  onConfirm={() =>
                    store.deleteModel(model.id).catch((e) => pushToast("error", String(e)))
                  }
                />
              </div>
            );
          })}
        </div>
      )}

      <div className="grid grid-cols-2 gap-4">
        <Field label={s.pitchOffset}>
          <div className="flex items-center gap-3">
            <Slider
              min={-24}
              max={24}
              step={1}
              bipolar
              value={store.pitchOffset}
              onChange={store.setPitchOffset}
            />
            <MonoValue>
              {store.pitchOffset > 0 ? "+" : ""}
              {store.pitchOffset} {strings.effects.semitonesUnit}
            </MonoValue>
          </div>
        </Field>
        <Field label={s.chunk}>
          <div className="flex items-center gap-3">
            <Slider
              min={160}
              max={640}
              step={40}
              value={store.chunkMs}
              onChange={store.setChunkMs}
            />
            <MonoValue>{store.chunkMs} ms</MonoValue>
          </div>
        </Field>
      </div>

      {importing ? <ImportDialog kind={importing} onClose={() => setImporting(null)} /> : null}
    </Card>
  );
}

function BackendBadge() {
  const backend = useVcStore((v) => v.backend);
  if (!backend) return null;
  return (
    <span className="rounded-full bg-raised px-2 py-0.5 font-mono text-body text-text-dim">
      {s.backend[backend.backend]}
    </span>
  );
}

function DownloadProgress() {
  const progress = useVcStore((v) => v.downloadProgress);
  if (!progress) return <span className="text-body text-text-dim">{s.downloading}…</span>;
  const pct = progress.total ? Math.round((progress.downloaded / progress.total) * 100) : null;
  return (
    <MonoValue>
      {s.downloading} {progress.file}
      {pct !== null ? ` ${pct}%` : ""}
    </MonoValue>
  );
}

function ImportDialog({ kind, onClose }: { kind: "model" | "clone"; onClose: () => void }) {
  const store = useVcStore();
  const pushToast = useToastStore((t) => t.push);
  const [onnxPath, setOnnxPath] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [consent, setConsent] = useState(false);
  const isClone = kind === "clone";

  async function pickFile() {
    const picked = await open({
      multiple: false,
      filters: [{ name: s.modelFilterName, extensions: ["onnx"] }],
    });
    if (typeof picked === "string") {
      setOnnxPath(picked);
      if (!name) {
        const stem = picked.split(/[\\/]/).pop() ?? "";
        setName(stem.replace(/\.onnx$/i, ""));
      }
    }
  }

  async function confirm() {
    if (!onnxPath || !name.trim() || (isClone && !consent)) return;
    const args = {
      onnx: onnxPath,
      index: null,
      name: name.trim(),
      defaultPitch: 0,
      sampleRate: 40_000,
    };
    try {
      if (isClone) {
        await store.importClone(args);
      } else {
        await store.importModel(args, null);
      }
      onClose();
    } catch (e) {
      pushToast("error", String(e));
    }
  }

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
        className="flex w-96 flex-col gap-4 rounded-2xl bg-surface p-5 shadow-pop"
        role="dialog"
        aria-label={isClone ? s.importClone : s.importModel}
      >
        <h3 className="text-subheading font-semibold text-text">
          {isClone ? s.importClone : s.importModel}
        </h3>

        <GhostButton onClick={() => void pickFile()}>
          {onnxPath ? onnxPath.split("\\").pop() : `${s.modelFilterName} (.onnx)…`}
        </GhostButton>

        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={s.namePrompt}
          className={`rounded-lg bg-raised px-3 py-1.5 text-body-lg text-text ring-1 ring-outline ${focusRing}`}
        />

        {isClone ? (
          <div className="flex flex-col gap-2 rounded-lg bg-raised p-3">
            <p className="text-body font-semibold text-text">{s.consentTitle}</p>
            <p className="text-body text-text-dim">{s.consentBody}</p>
            <label className="flex items-center gap-2 text-body text-text">
              <input
                type="checkbox"
                checked={consent}
                onChange={(e) => setConsent(e.target.checked)}
                className={`h-4 w-4 accent-(--color-mask) ${focusRing}`}
              />
              {s.consentCheckbox}
            </label>
            <button
              type="button"
              onClick={() => void openUrl(strings.links.cloneGuide)}
              className="self-start text-body text-text-dim underline-offset-4 hover:underline"
            >
              {s.cloneGuide}
            </button>
          </div>
        ) : null}

        <div className="flex justify-end gap-2">
          <GhostButton onClick={onClose}>{s.cancel}</GhostButton>
          <PrimaryButton
            onClick={() => void confirm()}
            disabled={!onnxPath || !name.trim() || (isClone && !consent)}
          >
            {s.confirmImport}
          </PrimaryButton>
        </div>
      </div>
    </div>
  );
}
