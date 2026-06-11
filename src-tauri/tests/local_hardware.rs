//! Integration tests against the machine's real audio devices and the
//! downloaded companion models. They exercise what unit tests cannot: real
//! WASAPI streams, real ONNX sessions, real GPU execution providers.
//!
//! They are `#[ignore]` because CI runners have neither audio endpoints nor
//! the companion files. Run locally with:
//!
//! ```bash
//! cargo test --test local_hardware -- --ignored --nocapture --test-threads=1
//! ```

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use mask_lib::audio::pipeline::{self, PipelineConfig};
use mask_lib::audio::{list_devices, DeviceKind};
use mask_lib::dsp::{builtin_presets, DspParams};
use mask_lib::soundboard::DecodedClip;
use mask_lib::vc::engine::{detect_backend, RvcSession};

fn default_ids() -> Option<(String, String)> {
    let devices = list_devices();
    let input = devices
        .iter()
        .find(|d| d.kind == DeviceKind::Input && d.is_default)?;
    let output = devices
        .iter()
        .find(|d| d.kind == DeviceKind::Output && d.is_default)?;
    Some((input.id.clone(), output.id.clone()))
}

fn companions_dir() -> std::path::PathBuf {
    let appdata = std::env::var("APPDATA").expect("APPDATA");
    std::path::Path::new(&appdata)
        .join("dev.guilherme.mask")
        .join("models")
        .join("companions")
}

/// Devices enumerate and carry sane metadata.
#[test]
#[ignore]
fn enumerates_real_devices() {
    let devices = list_devices();
    assert!(!devices.is_empty(), "no audio devices on this machine?");
    assert!(devices.iter().any(|d| d.kind == DeviceKind::Input));
    assert!(devices.iter().any(|d| d.kind == DeviceKind::Output));
    for d in &devices {
        assert!(!d.id.is_empty());
        assert!(!d.name.is_empty());
    }
}

/// Full pipeline on real WASAPI streams: starts, processes for 3 s, publishes
/// metrics, survives a live preset switch and a soundboard trigger, stops
/// cleanly. No assertion on input level (the mic may be silent), but output
/// must keep flowing without runaway underruns after warmup.
#[test]
#[ignore]
fn pipeline_runs_on_real_devices() {
    let (input_id, output_id) = default_ids().expect("default input+output devices");
    let config = PipelineConfig {
        input_device_id: input_id,
        output_device_id: output_id,
        monitor_device_id: None,
    };
    let params = DspParams::new(0.0, 0.0);
    let preset = builtin_presets().remove(0);

    let handle = pipeline::start(config, preset, params.clone(), |_| {}).expect("pipeline start");
    assert!(handle.shared.running.load(Ordering::Relaxed));

    // Warm up, then measure underrun growth over a steady window.
    std::thread::sleep(Duration::from_millis(1500));
    let underruns_before = handle.shared.metrics().underruns;

    // Live preset switch (robot) while running.
    let robot = builtin_presets()
        .into_iter()
        .find(|p| p.id == "robot")
        .unwrap();
    handle
        .ctrl
        .send(pipeline::CtrlMsg::SetPreset(Box::new(robot)))
        .unwrap();

    // Soundboard: 3 s of 440 Hz through the mixer (still playing when the
    // metrics are sampled below).
    let samples: Vec<f32> = (0..144_000)
        .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin())
        .collect();
    handle
        .ctrl
        .send(pipeline::CtrlMsg::TriggerClip(DecodedClip {
            id: "test-clip".into(),
            samples: Arc::new(samples),
            volume: 1.0,
        }))
        .unwrap();

    std::thread::sleep(Duration::from_millis(1500));
    let metrics = handle.shared.metrics();
    let underrun_growth = metrics.underruns - underruns_before;

    // The clip alone guarantees output signal regardless of mic silence.
    assert!(
        metrics.output_peak_db > -60.0,
        "no output signal: {metrics:?}"
    );
    assert!(
        metrics.latency_ms > 0.0 && metrics.latency_ms < 200.0,
        "latency above budget: {metrics:?}"
    );
    // Allow scheduling hiccups but not a continuously starving stream.
    assert!(
        underrun_growth < 48_000,
        "output starving: {underrun_growth} underruns in 1.5 s ({metrics:?})"
    );

    let mut handle = handle;
    handle.stop();
    assert!(!handle.shared.running.load(Ordering::Relaxed));
}

/// Same input and output device must be rejected (feedback loop guard).
#[test]
#[ignore]
fn pipeline_rejects_same_device() {
    let (input_id, _) = default_ids().expect("default devices");
    let config = PipelineConfig {
        input_device_id: input_id.clone(),
        output_device_id: input_id,
        monitor_device_id: None,
    };
    let result = pipeline::start(
        config,
        builtin_presets().remove(0),
        DspParams::new(0.0, 0.0),
        |_| {},
    );
    assert!(result.is_err());
}

/// The downloaded companion models load and produce sane shapes on a real
/// inference backend: ContentVec yields 768-dim features at ~50 fps and
/// RMVPE tracks a 220 Hz sine within a quarter tone.
#[test]
#[ignore]
fn companion_models_infer_correctly() {
    let dir = companions_dir();
    let contentvec = dir.join("contentvec.onnx");
    let rmvpe = dir.join("rmvpe.onnx");
    assert!(
        contentvec.exists() && rmvpe.exists(),
        "companions not downloaded at {dir:?}"
    );

    let backend = detect_backend();
    eprintln!("inference backend: {backend:?}");

    // RvcSession needs a voice model too; for the companion smoke test the
    // contentvec graph itself stands in (it is a valid ONNX graph, and the
    // constructor only introspects input names — which will fail). So load
    // sessions individually through the public validate path instead.
    mask_lib::vc::engine::validate_onnx(&contentvec).expect("contentvec session");
    mask_lib::vc::engine::validate_onnx(&rmvpe).expect("rmvpe session");

    // Exercise the full feature/pitch path via the test-only probe.
    let one_second_220hz: Vec<f32> = (0..16_000)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 16_000.0).sin())
        .collect();
    let (feat_dim, frames, f0_hz) =
        RvcSession::probe_companions(&contentvec, &rmvpe, backend, &one_second_220hz)
            .expect("companion inference");
    eprintln!("feat_dim={feat_dim} frames={frames} f0_median={f0_hz:.1} Hz");

    assert_eq!(feat_dim, 768, "ContentVec must emit 768-dim features");
    // 1 s at ~50 fps, doubled to ~100 fps by the engine.
    assert!(
        (80..=120).contains(&frames),
        "expected ~100 feature frames for 1 s, got {frames}"
    );
    // Quarter-tone tolerance around 220 Hz.
    assert!(
        (213.0..=227.0).contains(&f0_hz),
        "RMVPE should track 220 Hz, got {f0_hz}"
    );
}

/// Full RVC conversion with a real community voice model: load all three
/// sessions, stream three 320 ms chunks through `RvcSession::process`, and
/// require finite, non-silent audio of roughly the right duration.
/// Skips (passes with a notice) when the test model is not present.
#[test]
#[ignore]
fn full_rvc_conversion_produces_audio() {
    // A 256-dim community RVC export (drake.onnx from ozada/onnx_rvc),
    // downloaded by the test setup notes in the README.
    let model = std::env::temp_dir().join("drake.onnx");
    if !model.exists() {
        eprintln!("SKIP: no test voice model at {model:?}");
        return;
    }
    let appdata = std::env::var("APPDATA").expect("APPDATA");
    let data_dir = std::path::Path::new(&appdata).join("dev.guilherme.mask");
    let companions = mask_lib::vc::companion_paths(&data_dir);
    let backend = detect_backend();
    let mut session =
        RvcSession::load(&companions, &model, 40_000, 0, backend).expect("load voice model");

    // A 110 Hz sawtooth-ish voiced buzz: harmonically rich like speech.
    let chunk_len = 48_000 * 320 / 1000;
    let mut total_out = 0usize;
    let mut peak = 0.0f32;
    for c in 0..3 {
        let chunk: Vec<f32> = (0..chunk_len)
            .map(|i| {
                let t = (c * chunk_len + i) as f32 / 48_000.0;
                let phase = (110.0 * t).fract();
                0.4 * (2.0 * phase - 1.0)
            })
            .collect();
        let start = std::time::Instant::now();
        let out = session.process(&chunk).expect("rvc process");
        eprintln!(
            "chunk {c}: in {} out {} samples in {:?}",
            chunk.len(),
            out.len(),
            start.elapsed()
        );
        assert!(out.iter().all(|s| s.is_finite()), "non-finite output");
        peak = out.iter().fold(peak, |p, s| p.max(s.abs()));
        total_out += out.len();
    }
    // 3 × 320 ms in → roughly as much out (crossfade tail withheld).
    let expected = 3 * chunk_len;
    assert!(
        total_out > expected / 2 && total_out < expected * 2,
        "implausible output length {total_out} vs input {expected}"
    );
    assert!(peak > 0.001, "converted audio is silent (peak {peak})");
}
