# Virtual Microphone Setup Spec

Mask needs a virtual audio device so call apps can pick "Mask voice" as their microphone. Windows has no user-mode virtual mic API and signing a kernel driver is not viable for an individual open-source project (EV certificate plus legal entity). Decision: **Mask does not ship a driver**. It detects a user-installed virtual cable and guides installation through onboarding.

## Supported Virtual Cables

| Product | Detection (device name contains) | License | Notes |
|---|---|---|---|
| VB-Audio Virtual Cable | `CABLE Input` / `CABLE Output` | Donationware, not redistributable | Primary recommendation, battle-tested |
| Virtual-Audio-Driver (VirtualDrivers) | `Virtual Audio Device` | MIT, signed releases | Open-source alternative |

The user routes: Mask outputs to `CABLE Input`; the call app selects `CABLE Output` as its microphone.

## Business Rules

1. **Mask never bundles or auto-installs a driver.** Onboarding links to the official download pages and explains the manual install. Licensing is the reason; do not "fix" this with a bundled installer.
2. **Detection is by render-endpoint name heuristic** (`is_virtual_mic` flag in `AudioDevice`, see [audio_pipeline.md](audio_pipeline.md)). The known-name list lives in one Rust const.
3. **Onboarding triggers automatically** on first launch and whenever the pipeline starts with no virtual cable detected. It can be re-opened from settings.
4. **Onboarding steps**: explain why a cable is needed → download links → user installs (Windows may require reboot) → app re-scans devices → success state shows the detected cable and selects it as the pipeline output.
5. **Re-scan is manual (button) plus automatic** via the OS device-change notification; no polling loop.
6. **The final onboarding screen teaches the call-app side**: pick `CABLE Output` as the microphone in Discord/Zoom, with one illustration per app.
7. **The app remains usable without a cable** for trying effects (output to headphones); a persistent banner states that calls will not receive the voice until a cable is installed.
8. **A built-in mic test verifies the route**: while the pipeline runs into the cable, a meter shows signal on the cable's monitor; the success criterion is the user seeing the output meter move while speaking.

## Module Contract

```rust
// src-tauri/src/audio/virtual_mic.rs

enum VirtualMicStatus {
  NotInstalled,
  Installed { device: AudioDevice },
}

fn detect_virtual_mic(devices: &[AudioDevice]) -> VirtualMicStatus;
```

### Tauri commands

```
virtual_mic_status() -> VirtualMicStatus
```

(Device re-scan reuses `audio_list_devices` and the `pipeline://devices` event.)

## Onboarding State Machine (frontend)

```
FirstLaunch ──▶ Explain ──▶ DownloadLinks ──▶ WaitingInstall
WaitingInstall ──device detected (event or re-scan)──▶ Detected ──▶ CallAppGuide ──▶ Done
WaitingInstall ──skip──▶ Done (degraded: banner shown)
Any step ──close──▶ Done (resumable from settings)
```

## Edge Cases

- **Both cables installed** — prefer VB-Cable; offer a picker in settings.
- **Cable installed but disabled in Windows sound settings** — detection sees nothing; onboarding troubleshooting section covers enabling the device.
- **Windows requires reboot after driver install** — WaitingInstall copy mentions it; state persists across app restarts.
- **User renames the device in Windows** — heuristic fails; settings allows manually marking any output device as the virtual mic target.
- **VB-Cable installed at 44.1 kHz default** — pipeline resamples (see audio_pipeline rule 1); troubleshooting recommends setting the cable to 48 kHz for lowest latency.

## Open Questions

- TODO: evaluate shipping a settings deep-link or script that sets the cable's default format to 48 kHz / 16-bit automatically (requires elevation?).
