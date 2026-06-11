# Mask

Real-time voice changer for Windows, open source (MIT). Modify your microphone live (pitch, formants, robot, and more), play soundboard clips into the call, and route everything into Discord, Zoom, or any app through a virtual microphone.

Built with Tauri 2: all audio runs natively in Rust; the UI is React.

## Features

- **Voice effects in real time**: deep voice, high voice, feminine, masculine, robot, chipmunk, cave, old radio, plus custom pitch/formant sliders. DSP path latency stays under 50 ms.
- **Soundboard**: import any number of wav/mp3/ogg/flac clips and play them into the call with one click (up to 4 at once).
- **Virtual microphone routing**: outputs to VB-Cable or Virtual-Audio-Driver; the app guides the one-time install.
- **Monitoring**: optionally hear your own processed voice.
- Planned: AI voice conversion (RVC over ONNX, GPU-accelerated) and consent-gated voice cloning. See [docs/roadmap.md](docs/roadmap.md).

## How the routing works

```
Your mic → Mask (effects + soundboard) → CABLE Input (virtual cable)
Discord/Zoom microphone = CABLE Output
```

Mask does not bundle an audio driver (Windows requires Microsoft-signed kernel drivers). Install one of:

- [VB-Cable](https://vb-audio.com/Cable/) (donationware, the usual choice)
- [Virtual-Audio-Driver](https://github.com/VirtualDrivers/Virtual-Audio-Driver/releases) (MIT, open source)

The app detects the cable and walks you through setup on first launch.

## Development

Requirements: Rust (MSVC), Bun, and the [Tauri 2 Windows prerequisites](https://tauri.app/start/prerequisites/).

```bash
bun install
bun run tauri dev       # run the app
bun run tauri build     # build the installer

bun run lint            # Biome
cd src-tauri
cargo test              # Rust tests
cargo clippy --all-targets -- -D warnings
```

Project conventions live in [CLAUDE.md](CLAUDE.md); feature specs in [docs/specs/](docs/specs/).

## License

[MIT](LICENSE). Voice cloning features are intended for fun between consenting friends; impersonating people without consent is against this project's terms of use.
