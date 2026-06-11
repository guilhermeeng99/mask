// Left column: device pickers, start/stop, meters (app_shell.md layout).

import { strings } from "../lib/strings";
import { usePipelineStore } from "../stores/pipeline";
import { useToastStore } from "../stores/toasts";
import {
  Card,
  EmptyState,
  Field,
  GhostButton,
  Meter,
  PanelHeader,
  PrimaryButton,
  Select,
} from "./ui";

const s = strings.devices;

export function DevicesPanel() {
  const store = usePipelineStore();
  const pushToast = useToastStore((t) => t.push);
  const inputs = store.devices.filter((d) => d.kind === "input");
  const outputs = store.devices.filter((d) => d.kind === "output");
  const running = store.status === "running" || store.status === "starting";

  async function toggle() {
    try {
      if (running) {
        await store.stop();
      } else {
        await store.start();
      }
    } catch (err) {
      pushToast("error", `${strings.errors.pipelineStart} ${String(err)}`);
    }
  }

  if (store.devices.length === 0) {
    return (
      <Card className="h-full">
        <PanelHeader title={s.panelTitle} />
        <EmptyState
          title={s.noDevices}
          hint=""
          action={<GhostButton onClick={() => void store.rescanDevices()}>{s.rescan}</GhostButton>}
        />
      </Card>
    );
  }

  return (
    <Card className="h-full">
      <PanelHeader
        title={s.panelTitle}
        action={
          <GhostButton onClick={() => void store.rescanDevices()} disabled={running}>
            {s.rescan}
          </GhostButton>
        }
      />

      <Field label={s.microphone}>
        <Select value={store.selectedInputId ?? ""} onChange={store.selectInput} disabled={running}>
          <option value="" disabled>
            …
          </option>
          {inputs.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
            </option>
          ))}
        </Select>
      </Field>

      <Field label={s.output}>
        <Select
          value={store.selectedOutputId ?? ""}
          onChange={store.selectOutput}
          disabled={running}
        >
          <option value="" disabled>
            …
          </option>
          {outputs.map((d) => (
            <option key={d.id} value={d.id}>
              {d.isVirtualMic ? `● ${d.name}` : d.name}
            </option>
          ))}
        </Select>
      </Field>

      <Field label={s.monitor}>
        <Select
          value={store.monitorEnabled && store.monitorDeviceId ? store.monitorDeviceId : ""}
          onChange={(v) => store.selectMonitor(v === "" ? undefined : v)}
          disabled={running}
        >
          <option value="">{s.monitorOff}</option>
          {outputs
            .filter((d) => !d.isVirtualMic)
            .map((d) => (
              <option key={d.id} value={d.id}>
                {d.name}
              </option>
            ))}
        </Select>
      </Field>

      <div className="mt-auto flex flex-col gap-3">
        <Meter db={store.metrics.inputPeakDb} label={s.inputMeter} />
        <Meter db={store.metrics.outputPeakDb} label={s.outputMeter} />
        <PrimaryButton
          onClick={() => void toggle()}
          disabled={!store.selectedInputId || !store.selectedOutputId}
          title={!store.selectedInputId ? s.noInputSelected : undefined}
        >
          {running ? s.stop : s.start}
        </PrimaryButton>
        {store.status === "error" && store.errorMessage ? (
          <p className="text-body text-danger">{store.errorMessage}</p>
        ) : null}
      </div>
    </Card>
  );
}
