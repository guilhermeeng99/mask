# Virtual Microphone Setup Spec

Mask needs a virtual audio device so call apps can pick "Mask voice" as their microphone. Windows has no user-mode virtual mic API and signing a kernel driver is not viable for an individual open-source project (EV certificate plus legal entity).

Decision (updated 2026-06-11): **Mask does not bundle a driver, but installs one automatically on request.** The onboarding's primary action downloads the MIT-licensed [Virtual-Audio-Driver](https://github.com/VirtualDrivers/Virtual-Audio-Driver) release (sha256-pinned, fetched from the project's own GitHub releases), then relaunches `mask.exe --install-virtual-driver <inf>` elevated (one UAC prompt) to register the `ROOT\VirtualAudioDriver` devnode via SetupAPI and install the INF (`src-tauri/src/driver_install.rs`). VB-Cable remains a manual fallback only — its license forbids redistribution/automation.

**Known limitation (found 2026-06-11):** the GitHub "Signed" release of Virtual-Audio-Driver is signed by SignPath Foundation (a regular code-signing certificate), **not by Microsoft** (no WHQL/attestation signature — `pnputil` reports `WHCP Version: Unknown`). 64-bit Windows 11 with Secure Boot and/or Memory Integrity (Core Isolation) refuses to load kernel drivers without a Microsoft signature: the devnode installs but stays in Device Manager error state **Code 52** ("Windows cannot verify the digital signature"), and no audio endpoint ever appears. Rebooting does not help ([upstream issue #15](https://github.com/VirtualDrivers/Virtual-Audio-Driver/issues/15), open, no fix). On such machines — the default for modern consumer PCs — the app must detect the blocked devnode and steer the user to VB-Cable instead of suggesting restarts.

## Supported Virtual Cables

| Product | Detection (device name contains) | License | Notes |
|---|---|---|---|
| VB-Audio Virtual Cable | `CABLE Input` / `CABLE Output` | Donationware, not redistributable | Primary recommendation, battle-tested |
| Virtual-Audio-Driver (VirtualDrivers) | `Virtual Audio Device` | MIT, signed releases | Open-source alternative |

The user routes: Mask outputs to `CABLE Input`; the call app selects `CABLE Output` as its microphone.

## Business Rules

1. **Mask never bundles a driver in the installer**, and never auto-installs VB-Cable (license). The one-click path only ever fetches the MIT Virtual-Audio-Driver from its official signed release, verifies the pinned sha256, and asks Windows for elevation once.
2. **Detection is by render-endpoint name heuristic** (`is_virtual_mic` flag in `AudioDevice`, see [audio_pipeline.md](audio_pipeline.md)). The known-name list lives in one Rust const.
3. **Onboarding triggers automatically** on first launch and whenever the pipeline starts with no virtual cable detected. It can be re-opened from settings.
4. **Onboarding steps**: explain why a cable is needed → download links → user installs (Windows may require reboot) → app re-scans devices → success state shows the detected cable and selects it as the pipeline output.
5. **Re-scan is manual (button) in V1.** Automatic OS device-change notification is a TODO (cpal exposes no notification API; would need a WASAPI `IMMNotificationClient` hook).
6. **The final onboarding screen teaches the call-app side**: pick `CABLE Output` as the microphone in Discord/Zoom, with one illustration per app.
7. **The app remains usable without a cable** for trying effects (output to headphones); a persistent banner states that calls will not receive the voice until a cable is installed.
8. **A built-in mic test verifies the route**: while the pipeline runs into the cable, a meter shows signal on the cable's monitor; the success criterion is the user seeing the output meter move while speaking.
9. **Status detection inspects the devnode, not just endpoints.** `virtual_mic_status` first looks for a working virtual render endpoint (→ `Installed`). If none exists but a `ROOT\VirtualAudioDriver` devnode is present with a Device Manager problem code (queried via SetupAPI + `CM_Get_DevNode_Status`), the status is `Blocked { problem_code }` — never `NotInstalled`, and never a "restart your PC" message. Code 52 specifically means the driver signature was rejected (Secure Boot / Memory Integrity); the UI must say restarting will not fix it and point to VB-Cable. Exception: problem code 14 (`CM_PROB_NEED_RESTART`) is the one case a reboot fixes — the UI maps it to the reboot-needed copy, not Blocked.
10. **The install helper never registers a duplicate devnode.** Before `DIF_REGISTERDEVICE`, it enumerates present devices for the `ROOT\VirtualAudioDriver` hardware ID; if one exists, it reuses it and only re-runs the driver update. Each retry must leave at most one devnode on the system.

## Module Contract

```rust
// src-tauri/src/audio/virtual_mic.rs

enum VirtualMicStatus {
  NotInstalled,
  Installed { device: AudioDevice },
  /// Devnode present but Windows refuses to start it (Device Manager problem
  /// code, e.g. 52 = driver signature rejected). Restarting will not fix it.
  Blocked { problem_code: u32 },
}

/// Pure decision over enumerated endpoints + the devnode problem code
/// (queried separately in src-tauri/src/driver_devnode.rs so this stays testable).
fn detect_virtual_mic(devices: &[AudioDevice], driver_problem: Option<u32>) -> VirtualMicStatus;
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
WaitingInstall ──status Blocked (re-scan)──▶ Blocked ──VB-Cable installed (re-scan)──▶ Detected
WaitingInstall ──skip──▶ Done (degraded: banner shown)
Any step ──close──▶ Done (resumable from settings)
```

`Blocked` shows why Windows refused the driver (signature, code 52), states that
restarting will not help, and offers the VB-Cable manual path + re-scan.

## Edge Cases

- **Both cables installed** — prefer VB-Cable; offer a picker in settings.
- **Devnode blocked (code 52) but VB-Cable later installed** — a working endpoint wins over a stale problem code: status is `Installed`, not `Blocked`.
- **One-click install retried after a failed attempt** — the existing devnode is reused (rule 10); no `ROOT\MEDIA\000N` duplicates accumulate.
- **Cable installed but disabled in Windows sound settings** — detection sees nothing; onboarding troubleshooting section covers enabling the device.
- **Windows requires reboot after driver install** — WaitingInstall copy mentions it; state persists across app restarts.
- **User renames the device in Windows** — heuristic fails; settings allows manually marking any output device as the virtual mic target.
- **VB-Cable installed at 44.1 kHz default** — pipeline resamples (see audio_pipeline rule 1); troubleshooting recommends setting the cable to 48 kHz for lowest latency.

## Open Questions

- TODO: evaluate shipping a settings deep-link or script that sets the cable's default format to 48 kHz / 16-bit automatically (requires elevation?).
