// Full-window onboarding overlay (virtual_mic_setup.md state machine).

import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { ipc } from "../lib/ipc";
import { strings } from "../lib/strings";
import { usePipelineStore } from "../stores/pipeline";
import { GhostButton, PrimaryButton, Spinner } from "./ui";

const s = strings.onboarding;

type Step = "explain" | "waiting" | "detected";

export function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState<Step>("explain");
  const [scanning, setScanning] = useState(false);
  const rescanDevices = usePipelineStore((p) => p.rescanDevices);

  async function rescan() {
    setScanning(true);
    try {
      const status = await ipc.virtualMicStatus();
      await rescanDevices();
      if (status.status === "installed") {
        setStep("detected");
      }
    } finally {
      setScanning(false);
    }
  }

  function finish() {
    void ipc.onboardingComplete();
    onDone();
  }

  const dots: Step[] = ["explain", "waiting", "detected"];

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-ink">
      <div className="flex w-[480px] flex-col gap-6 text-center">
        <h1 className="text-heading font-bold text-text">{s.title}</h1>

        {step === "explain" ? (
          <>
            <p className="text-body-lg text-text-dim">{s.explain}</p>
            <div className="flex flex-col items-center gap-3">
              <PrimaryButton onClick={() => void openUrl(strings.links.vbCable)}>
                {s.downloadVbCable}
              </PrimaryButton>
              <button
                type="button"
                onClick={() => void openUrl(strings.links.virtualAudioDriver)}
                className="text-body text-text-dim underline-offset-4 hover:underline"
              >
                {s.explainAlt}
              </button>
            </div>
            <PrimaryButton onClick={() => setStep("waiting")}>{s.next}</PrimaryButton>
          </>
        ) : null}

        {step === "waiting" ? (
          <>
            <p className="text-body-lg text-text-dim">{s.waiting}</p>
            <div className="flex items-center justify-center gap-3">
              <PrimaryButton onClick={() => void rescan()} disabled={scanning}>
                {s.rescan}
              </PrimaryButton>
              {scanning ? <Spinner /> : null}
            </div>
          </>
        ) : null}

        {step === "detected" ? (
          <>
            <p className="text-body-lg font-semibold text-live">{s.detected}</p>
            <p className="text-body-lg text-text-dim">{s.detectedBody}</p>
            <p className="rounded-lg bg-surface px-4 py-3 text-body text-text">{s.callAppGuide}</p>
            <p className="text-body text-text-faint">{s.testHint}</p>
            <PrimaryButton onClick={finish}>{s.done}</PrimaryButton>
          </>
        ) : null}

        <div className="flex items-center justify-center gap-2">
          {dots.map((d) => (
            <span
              key={d}
              className={`h-1.5 w-1.5 rounded-full ${d === step ? "bg-mask" : "bg-outline"}`}
            />
          ))}
        </div>
        {step !== "detected" ? (
          <GhostButton className="mx-auto" onClick={finish}>
            {s.skip}
          </GhostButton>
        ) : null}
      </div>
    </div>
  );
}
