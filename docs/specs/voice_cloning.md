# Voice Cloning Spec (Phase 3)

Train an RVC model on a friend's voice locally, then use it in the [ai_voice_conversion.md](ai_voice_conversion.md) pipeline. For fun between consenting friends; ethics rules are part of the product.

Status: planned, lowest priority. Direction is fixed; details are TODO until phase 2 ships.

## Approach

RVC training on the user's NVIDIA GPU: 10–30 min of clean recorded audio, roughly 1–3 hours of training on an RTX-class card, producing a `.pth` checkpoint that is exported to ONNX and imported into the model list.

Training will NOT be reimplemented in Rust. Decision pending between:

- **(a) Companion training tool**: separate downloadable package (Python, PyTorch) that Mask shells out to and monitors. Keeps the main app lean.
- **(b) Guide-only V1**: in-app guide for training with the existing RVC WebUI, plus a first-class "import and ONNX-export" flow in Mask.

TODO: decide after phase 2; (b) is the cheaper first step.

## Business Rules

1. **Consent gate**: creating a clone requires checking an explicit confirmation that the voice's owner agreed. The clone's `license_note` records "personal clone, consent confirmed on <date>".
2. **Everything is local.** No audio or model leaves the machine; no cloud training.
3. **Dataset quality guidance in-app**: minimum 10 min of clean speech, no music/noise, one speaker. The importer runs basic checks (duration, clipping, silence ratio) and warns before training time is wasted.
4. **Training requires an NVIDIA GPU** (CUDA). Detected up front; unsupported hardware gets the guide-only path pointing to cloud notebooks (user's own choice, outside the app).
5. **Output of any path is a standard `VcModel`** in the regular model list, no special casing downstream.
6. **The README and app docs state the misuse policy**: clones of non-consenting people, impersonation, and fraud are against the project's terms; the feature is for entertainment between friends.

## Cloning Flow (target UX, option (a))

```
Record/collect audio ──▶ dataset check ──▶ preprocess ──▶ train (progress, ETA, pausable)
  ──▶ test phrase preview ──▶ export ONNX ──▶ appears in VC model list
```

## Edge Cases

- TODO: define after the training-tool decision (interrupted training resume, disk space, multi-GPU, dataset too small).

## Open Questions

- TODO: companion tool vs guide-only for V1 of this phase.
- TODO: can RVC training be driven headlessly with stable CLI flags from a Tauri sidecar, and what is the distribution size?
- TODO: zero-shot alternatives with MIT-compatible licenses (Seed-VC is GPLv3, excluded) — re-survey the landscape when this phase starts.
