import { useEffect, useState } from "react";
import { DevicesPanel } from "./components/DevicesPanel";
import { EffectsPanel } from "./components/EffectsPanel";
import { Onboarding } from "./components/Onboarding";
import { SoundboardPanel } from "./components/SoundboardPanel";
import { Toasts } from "./components/Toasts";
import { MonoValue, StatusDot } from "./components/ui";
import { subscribeToBackendEvents } from "./lib/events";
import { ipc } from "./lib/ipc";
import { strings } from "./lib/strings";
import { useEffectsStore } from "./stores/effects";
import { usePipelineStore } from "./stores/pipeline";
import { useSoundboardStore } from "./stores/soundboard";

export function App() {
  const pipeline = usePipelineStore();
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    const unsubscribe = subscribeToBackendEvents();
    void (async () => {
      const config = await ipc.configGet();
      await Promise.all([
        usePipelineStore.getState().hydrate(),
        useEffectsStore.getState().hydrate(),
        useSoundboardStore.getState().hydrate(),
      ]);
      if (!config.onboardingDone) {
        setShowOnboarding(true);
      }
      setHydrated(true);
    })();
    return unsubscribe;
  }, []);

  const cableMissing =
    hydrated && !pipeline.devices.some((d) => d.kind === "output" && d.isVirtualMic);

  return (
    <div className="flex h-screen flex-col gap-3 p-4">
      <header className="flex items-center gap-3 px-1">
        <span className="text-subheading font-bold text-text">{strings.app.name}</span>
        <StatusDot status={pipeline.status} />
        <span className="text-body text-text-dim">{strings.status[pipeline.status]}</span>
        {pipeline.status === "running" ? (
          <MonoValue>{Math.round(pipeline.metrics.latencyMs)} ms</MonoValue>
        ) : null}
        {cableMissing ? (
          <button
            type="button"
            onClick={() => setShowOnboarding(true)}
            className="ml-auto rounded-full bg-warn/10 px-3 py-1 text-body text-warn ring-1 ring-warn/40 transition hover:ring-warn"
          >
            {strings.devices.cableMissingBanner} {strings.devices.cableMissingAction}
          </button>
        ) : null}
      </header>

      <main className="grid min-h-0 grow grid-cols-[280px_1fr] gap-3">
        <DevicesPanel />
        <div className="flex min-h-0 flex-col gap-3">
          <EffectsPanel />
          <SoundboardPanel />
        </div>
      </main>

      {showOnboarding ? <Onboarding onDone={() => setShowOnboarding(false)} /> : null}
      <Toasts />
    </div>
  );
}
