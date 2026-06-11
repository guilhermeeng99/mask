// Full-window onboarding overlay (virtual_mic_setup.md state machine).
// Primary path: one-click driver install (signed MIT Virtual-Audio-Driver,
// downloaded + installed by the backend with a single UAC prompt). Manual
// VB-Cable install remains as the fallback.

import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { strings } from "../lib/strings";
import { usePipelineStore } from "../stores/pipeline";
import { GhostButton, PrimaryButton, Spinner } from "./ui";

const s = strings.onboarding;

type Step = "explain" | "installing" | "manualWait" | "rebootNeeded" | "detected";

interface InstallEvent {
  state: "downloading" | "done" | "error";
  detail: { rebootRequired?: boolean } | string | null;
}

export function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState<Step>("explain");
  const [installPhase, setInstallPhase] = useState<string>(s.installDownloading);
  const [error, setError] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const rescanDevices = usePipelineStore((p) => p.rescanDevices);

  async function checkDetected(): Promise<boolean> {
    const status = await ipc.virtualMicStatus();
    await rescanDevices();
    if (status.status === "installed") {
      setStep("detected");
      return true;
    }
    return false;
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: subscribe once on mount
  useEffect(() => {
    const unlisten = listen<InstallEvent>("vmic://install", (e) => {
      const { state, detail } = e.payload;
      if (state === "downloading") {
        // The UAC prompt follows right after the short download.
        setInstallPhase(s.installDownloading);
        setTimeout(() => setInstallPhase(s.installElevating), 2500);
      } else if (state === "done") {
        const reboot = typeof detail === "object" && detail !== null && detail.rebootRequired;
        if (reboot) {
          setStep("rebootNeeded");
        } else {
          // Give Windows a moment to enumerate the new endpoints.
          setTimeout(() => {
            void checkDetected().then((found) => {
              if (!found) setStep("rebootNeeded");
            });
          }, 1500);
        }
      } else if (state === "error") {
        setError(typeof detail === "string" ? detail : String(detail));
        setStep("explain");
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  function startInstall() {
    setError(null);
    setInstallPhase(s.installDownloading);
    setStep("installing");
    ipc.virtualMicInstall().catch((e) => {
      setError(String(e));
      setStep("explain");
    });
  }

  async function rescan() {
    setScanning(true);
    try {
      await checkDetected();
    } finally {
      setScanning(false);
    }
  }

  function finish() {
    void ipc.onboardingComplete();
    onDone();
  }

  const dots: Step[] = ["explain", "installing", "detected"];
  const dotStep =
    step === "manualWait" ? "installing" : step === "rebootNeeded" ? "installing" : step;

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-ground">
      <div className="flex w-[480px] flex-col gap-6 text-center">
        <h1 className="text-heading font-medium tracking-tight text-text">{s.title}</h1>

        {step === "explain" ? (
          <>
            <p className="text-body-lg text-text-dim">{s.explain}</p>
            {error ? (
              <p className="text-body text-danger">
                {s.installErrorPrefix} {error}
              </p>
            ) : null}
            <PrimaryButton className="mx-auto" onClick={startInstall}>
              {error ? s.retry : s.installAuto}
            </PrimaryButton>
            <div className="flex flex-col items-center gap-1">
              <p className="text-body text-text-faint">{s.manualHint}</p>
              <button
                type="button"
                onClick={() => {
                  void openUrl(strings.links.vbCable);
                  setStep("manualWait");
                }}
                className="text-body text-text-dim underline-offset-4 hover:underline"
              >
                {s.downloadVbCable}
              </button>
            </div>
          </>
        ) : null}

        {step === "installing" ? (
          <div className="flex items-center justify-center gap-3">
            <Spinner />
            <p className="text-body-lg text-text-dim">{installPhase}</p>
          </div>
        ) : null}

        {step === "manualWait" ? (
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

        {step === "rebootNeeded" ? (
          <>
            <p className="text-body-lg text-text-dim">{s.installReboot}</p>
            <div className="flex items-center justify-center gap-3">
              <GhostButton onClick={() => void rescan()} disabled={scanning}>
                {s.rescan}
              </GhostButton>
              {scanning ? <Spinner /> : null}
            </div>
          </>
        ) : null}

        {step === "detected" ? (
          <>
            <p className="text-body-lg font-medium text-live">{s.detected}</p>
            <p className="text-body-lg text-text-dim">{s.detectedBody}</p>
            <p className="rounded-2xl bg-surface px-4 py-3 text-body text-text ring-1 ring-outline">
              {s.callAppGuide}
            </p>
            <p className="text-body text-text-faint">{s.testHint}</p>
            <PrimaryButton className="mx-auto" onClick={finish}>
              {s.done}
            </PrimaryButton>
          </>
        ) : null}

        <div className="flex items-center justify-center gap-2">
          {dots.map((d) => (
            <span
              key={d}
              className={`h-1.5 w-1.5 rounded-full ${d === dotStep ? "bg-mask" : "bg-outline"}`}
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
