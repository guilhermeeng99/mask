//! The live audio engine: mic capture → DSP → soundboard mix → virtual mic.
//!
//! Threading model (CLAUDE.md real-time rules):
//! - cpal callbacks only move samples through lock-free rings (no alloc/locks).
//! - An engine thread owns the cpal streams (they are !Send) and runs the
//!   block-processing loop: resample to 48 kHz, DSP chain, clip mixing,
//!   limiter, resample to the output rate.
//! - Commands talk to the engine via an mpsc control channel; the UI reads
//!   metrics from atomics published here.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use serde::{Deserialize, Serialize};

use crate::dsp::{DspChain, DspParams, DspPreset};
use crate::soundboard::{DecodedClip, MAX_VOICES};

use super::{find_device, AudioError, DeviceKind};

pub const INTERNAL_RATE: u32 = 48_000;
/// 10 ms blocks at 48 kHz.
pub const BLOCK: usize = 480;
/// Ring capacity: ~85 ms of slack per side absorbs clock drift.
const RING_SECONDS: f32 = 0.085;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineConfig {
    pub input_device_id: String,
    pub output_device_id: String,
    pub monitor_device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineMetrics {
    pub input_peak_db: f32,
    pub output_peak_db: f32,
    pub latency_ms: f32,
    pub underruns: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayingClip {
    pub id: String,
    pub position_ms: u64,
}

pub enum CtrlMsg {
    Stop,
    SetPreset(Box<DspPreset>),
    /// Route voice through the AI inference thread (Some) or back to the
    /// DSP chain (None). See `crate::vc::engine`.
    SetVcLink(Option<Box<crate::vc::engine::VcLink>>),
    TriggerClip(DecodedClip),
    StopClip(String),
    StopAllClips,
}

/// Events the engine reports back to the app (emitted as Tauri events).
pub enum EngineEvent {
    /// AI inference could not keep up; voice fell back to the DSP chain
    /// (ai_voice_conversion rule 7).
    VcFallback(String),
}

/// State shared between the engine thread and the rest of the app.
pub struct Shared {
    pub input_peak_bits: AtomicU32,
    pub output_peak_bits: AtomicU32,
    pub latency_ms_bits: AtomicU32,
    pub underruns: AtomicU64,
    pub running: AtomicBool,
    pub playing: parking_lot::Mutex<Vec<PlayingClip>>,
}

impl Shared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            input_peak_bits: AtomicU32::new(0f32.to_bits()),
            output_peak_bits: AtomicU32::new(0f32.to_bits()),
            latency_ms_bits: AtomicU32::new(0f32.to_bits()),
            underruns: AtomicU64::new(0),
            running: AtomicBool::new(false),
            playing: parking_lot::Mutex::new(Vec::new()),
        })
    }

    pub fn metrics(&self) -> PipelineMetrics {
        PipelineMetrics {
            input_peak_db: to_db(f32::from_bits(self.input_peak_bits.load(Ordering::Relaxed))),
            output_peak_db: to_db(f32::from_bits(
                self.output_peak_bits.load(Ordering::Relaxed),
            )),
            latency_ms: f32::from_bits(self.latency_ms_bits.load(Ordering::Relaxed)),
            underruns: self.underruns.load(Ordering::Relaxed),
        }
    }
}

fn to_db(peak: f32) -> f32 {
    if peak <= 1e-6 {
        -120.0
    } else {
        20.0 * peak.log10()
    }
}

/// Handle owned by the Tauri state. Dropping it stops the engine.
pub struct PipelineHandle {
    pub ctrl: Sender<CtrlMsg>,
    pub shared: Arc<Shared>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl PipelineHandle {
    pub fn stop(&mut self) {
        let _ = self.ctrl.send(CtrlMsg::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for PipelineHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start the engine. Validates devices, then spawns the engine thread which
/// opens the streams and reports back through `ready_rx` so failures surface
/// as a command error instead of a zombie thread.
pub fn start(
    config: PipelineConfig,
    preset: DspPreset,
    params: Arc<DspParams>,
    on_event: impl Fn(EngineEvent) + Send + 'static,
) -> Result<PipelineHandle, AudioError> {
    if config.input_device_id == config.output_device_id {
        return Err(AudioError::SameDevice);
    }
    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel::<CtrlMsg>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), AudioError>>();
    let shared = Shared::new();
    let shared_engine = shared.clone();

    let join = std::thread::Builder::new()
        .name("mask-audio-engine".into())
        .spawn(move || {
            engine_main(
                config,
                preset,
                params,
                ctrl_rx,
                ready_tx,
                shared_engine,
                on_event,
            );
        })
        .map_err(|e| AudioError::Stream(e.to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(PipelineHandle {
            ctrl: ctrl_tx,
            shared,
            join: Some(join),
        }),
        Ok(Err(e)) => {
            let _ = join.join();
            Err(e)
        }
        Err(_) => {
            let _ = join.join();
            Err(AudioError::Stream(
                "engine thread died during startup".into(),
            ))
        }
    }
}

struct EndpointStream {
    _stream: cpal::Stream,
    rate: u32,
}

fn open_input(
    id: &str,
    mut producer: HeapProd<f32>,
    peak_bits: Arc<AtomicU32>,
) -> Result<EndpointStream, AudioError> {
    let device = find_device(id, DeviceKind::Input)?;
    let default = device
        .default_input_config()
        .map_err(|e| AudioError::Stream(e.to_string()))?;
    let channels = default.channels() as usize;
    let rate = default.sample_rate();
    let stream_config: StreamConfig = default.config();

    // Downmix interleaved frames to mono and push. No alloc, no locks.
    macro_rules! build {
        ($t:ty, $to_f32:expr) => {
            device.build_input_stream(
                stream_config.clone(),
                move |data: &[$t], _| {
                    let mut peak = 0.0f32;
                    for frame in data.chunks_exact(channels) {
                        let mut acc = 0.0f32;
                        for s in frame {
                            #[allow(clippy::redundant_closure_call)]
                            {
                                acc += $to_f32(*s);
                            }
                        }
                        let mono = acc / channels as f32;
                        peak = peak.max(mono.abs());
                        let _ = producer.try_push(mono);
                    }
                    peak_bits.store(peak.to_bits(), Ordering::Relaxed);
                },
                |_err| {},
                None,
            )
        };
    }

    let stream = match default.sample_format() {
        SampleFormat::F32 => build!(f32, |s: f32| s),
        SampleFormat::I16 => build!(i16, |s: i16| s as f32 / i16::MAX as f32),
        SampleFormat::U16 => build!(u16, |s: u16| (s as f32 - 32_768.0) / 32_768.0),
        other => {
            return Err(AudioError::Stream(format!(
                "unsupported input sample format: {other:?}"
            )))
        }
    }
    .map_err(|e| AudioError::Stream(e.to_string()))?;
    stream
        .play()
        .map_err(|e| AudioError::Stream(e.to_string()))?;
    Ok(EndpointStream {
        _stream: stream,
        rate,
    })
}

fn open_output(
    id: &str,
    mut consumer: HeapCons<f32>,
    underruns: Option<Arc<AtomicU64>>,
) -> Result<EndpointStream, AudioError> {
    let device = find_device(id, DeviceKind::Output)?;
    let default = device
        .default_output_config()
        .map_err(|e| AudioError::Stream(e.to_string()))?;
    let channels = default.channels() as usize;
    let rate = default.sample_rate();
    let stream_config: StreamConfig = default.config();

    // Upmix mono to every channel; silence + counter on underrun (rule 3).
    let stream = device
        .build_output_stream(
            stream_config,
            move |data: &mut [f32], _| {
                for frame in data.chunks_exact_mut(channels) {
                    let sample = match consumer.try_pop() {
                        Some(s) => s,
                        None => {
                            if let Some(u) = &underruns {
                                u.fetch_add(1, Ordering::Relaxed);
                            }
                            0.0
                        }
                    };
                    for out in frame {
                        *out = sample;
                    }
                }
            },
            |_err| {},
            None,
        )
        .map_err(|e| AudioError::Stream(e.to_string()))?;
    stream
        .play()
        .map_err(|e| AudioError::Stream(e.to_string()))?;
    Ok(EndpointStream {
        _stream: stream,
        rate,
    })
}

struct Voice {
    id: String,
    samples: Arc<Vec<f32>>,
    pos: usize,
    gain: f32,
    seq: u64,
}

#[allow(clippy::too_many_arguments)]
fn engine_main(
    config: PipelineConfig,
    preset: DspPreset,
    params: Arc<DspParams>,
    ctrl: Receiver<CtrlMsg>,
    ready: Sender<Result<(), AudioError>>,
    shared: Arc<Shared>,
    on_event: impl Fn(EngineEvent),
) {
    // --- Open endpoints ---------------------------------------------------
    let in_peak = Arc::new(AtomicU32::new(0));
    let underruns = Arc::new(AtomicU64::new(0));

    let in_ring = HeapRb::<f32>::new((INTERNAL_RATE as f32 * RING_SECONDS) as usize * 4);
    let (in_prod, mut in_cons) = in_ring.split();
    let input = match open_input(&config.input_device_id, in_prod, in_peak.clone()) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };

    let out_ring = HeapRb::<f32>::new((INTERNAL_RATE as f32 * RING_SECONDS) as usize * 4);
    let (mut out_prod, out_cons) = out_ring.split();
    let output = match open_output(&config.output_device_id, out_cons, Some(underruns.clone())) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };

    let mut monitor: Option<(EndpointStream, HeapProd<f32>)> = None;
    if let Some(mon_id) = &config.monitor_device_id {
        let mon_ring = HeapRb::<f32>::new((INTERNAL_RATE as f32 * RING_SECONDS) as usize * 4);
        let (mon_prod, mon_cons) = mon_ring.split();
        match open_output(mon_id, mon_cons, None) {
            Ok(s) => monitor = Some((s, mon_prod)),
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        }
    }

    let _ = ready.send(Ok(()));
    shared.running.store(true, Ordering::Relaxed);

    // --- Processing state ---------------------------------------------------
    let mut chain = DspChain::new(INTERNAL_RATE, &preset, params.clone());
    let mut fade_out_chain: Option<DspChain> = None;

    let mut in_resampler = StreamResampler::new(input.rate, INTERNAL_RATE);
    let mut out_resampler = StreamResampler::new(INTERNAL_RATE, output.rate);

    let mut staging: Vec<f32> = Vec::with_capacity(BLOCK * 8);
    let mut fifo: Vec<f32> = Vec::with_capacity(BLOCK * 16);
    let mut block_in = [0.0f32; BLOCK];
    let mut block_out = [0.0f32; BLOCK];
    let mut block_fade = [0.0f32; BLOCK];
    let mut out_staging: Vec<f32> = Vec::with_capacity(BLOCK * 8);

    let mut voices: Vec<Voice> = Vec::with_capacity(MAX_VOICES);
    let mut voice_seq: u64 = 0;
    let mut blocks_since_publish: u32 = 0;

    // AI voice conversion routing (phase 2). `vc_warm` flips once the
    // inference thread has produced audio; until then the DSP chain keeps
    // running so mode switches have no silent gap.
    let mut vc_link: Option<Box<crate::vc::engine::VcLink>> = None;
    let mut vc_warm = false;
    let mut vc_starved_blocks: u32 = 0;
    // 2 s of continuous starvation triggers the passthrough fallback.
    const VC_STARVE_LIMIT: u32 = 200;

    'engine: loop {
        // Drain control messages (engine thread may allocate; cpal callbacks may not).
        loop {
            match ctrl.try_recv() {
                Ok(CtrlMsg::Stop) => break 'engine,
                Ok(CtrlMsg::SetPreset(p)) => {
                    let new_chain = DspChain::new(INTERNAL_RATE, &p, params.clone());
                    fade_out_chain = Some(std::mem::replace(&mut chain, new_chain));
                }
                Ok(CtrlMsg::SetVcLink(link)) => {
                    vc_link = link;
                    vc_warm = false;
                    vc_starved_blocks = 0;
                }
                Ok(CtrlMsg::TriggerClip(clip)) => {
                    voice_seq += 1;
                    if let Some(v) = voices.iter_mut().find(|v| v.id == clip.id) {
                        v.pos = 0; // retrigger restarts (rule 8)
                        v.gain = clip.volume;
                        v.samples = clip.samples;
                        v.seq = voice_seq;
                    } else {
                        if voices.len() >= MAX_VOICES {
                            // Drop the oldest playing voice (rule 7).
                            if let Some(idx) = voices
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, v)| v.seq)
                                .map(|(i, _)| i)
                            {
                                voices.swap_remove(idx);
                            }
                        }
                        voices.push(Voice {
                            id: clip.id,
                            samples: clip.samples,
                            pos: 0,
                            gain: clip.volume,
                            seq: voice_seq,
                        });
                    }
                }
                Ok(CtrlMsg::StopClip(id)) => voices.retain(|v| v.id != id),
                Ok(CtrlMsg::StopAllClips) => voices.clear(),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'engine,
            }
        }

        // Pull captured samples and resample into the 48 kHz fifo.
        staging.clear();
        while staging.len() < BLOCK * 4 {
            match in_cons.try_pop() {
                Some(s) => staging.push(s),
                None => break,
            }
        }
        if !staging.is_empty() {
            in_resampler.process(&staging, &mut fifo);
        }

        if fifo.len() < BLOCK {
            // Not enough input yet; yield briefly instead of spinning.
            std::thread::sleep(std::time::Duration::from_millis(2));
            continue;
        }

        // Take one block.
        block_in.copy_from_slice(&fifo[..BLOCK]);
        fifo.drain(..BLOCK);

        // Voice processing (+ one-block crossfade after a preset switch, rule 2).
        chain.process(&block_in, &mut block_out);
        if let Some(mut old) = fade_out_chain.take() {
            old.process(&block_in, &mut block_fade);
            for (i, out) in block_out.iter_mut().enumerate() {
                let t = i as f32 / BLOCK as f32;
                *out = block_fade[i] * (1.0 - t) + *out * t;
            }
        }

        // AI voice conversion: feed the inference thread and prefer its
        // output once it is warm. Starvation falls back to the DSP output
        // (and permanently after VC_STARVE_LIMIT, ai_voice_conversion rule 7).
        if let Some(link) = &mut vc_link {
            for s in &block_in {
                let _ = link.to_vc.try_push(*s);
            }
            if link.from_vc.occupied_len() >= BLOCK {
                for out in block_out.iter_mut() {
                    *out = link.from_vc.try_pop().unwrap_or(0.0);
                }
                vc_warm = true;
                vc_starved_blocks = 0;
            } else if vc_warm {
                vc_starved_blocks += 1;
                if vc_starved_blocks >= VC_STARVE_LIMIT {
                    vc_link = None;
                    on_event(EngineEvent::VcFallback(
                        "inference cannot keep up with real time".into(),
                    ));
                }
            }
        }

        // Soundboard mix, post-voice-processing (rule 4).
        for voice in &mut voices {
            let remaining = voice.samples.len() - voice.pos;
            let n = remaining.min(BLOCK);
            for (out, sample) in block_out[..n]
                .iter_mut()
                .zip(&voice.samples[voice.pos..voice.pos + n])
            {
                *out += sample * voice.gain;
            }
            voice.pos += n;
        }
        voices.retain(|v| v.pos < v.samples.len());

        // Soft limiter (rule 5) + output peak.
        let mut peak = 0.0f32;
        for s in block_out.iter_mut() {
            *s = soft_clip(*s);
            peak = peak.max(s.abs());
        }
        shared
            .output_peak_bits
            .store(peak.to_bits(), Ordering::Relaxed);
        shared
            .input_peak_bits
            .store(in_peak.load(Ordering::Relaxed), Ordering::Relaxed);
        shared
            .underruns
            .store(underruns.load(Ordering::Relaxed), Ordering::Relaxed);

        // Resample to the device rate and ship it.
        out_staging.clear();
        out_resampler.process(&block_out, &mut out_staging);
        for s in &out_staging {
            let _ = out_prod.try_push(*s);
        }
        if let Some((_, mon_prod)) = &mut monitor {
            for s in &out_staging {
                let _ = mon_prod.try_push(*s);
            }
        }

        // Latency estimate: fifo backlog + one block + output ring occupancy.
        let latency_ms = (fifo.len() + BLOCK) as f32 / INTERNAL_RATE as f32 * 1000.0
            + out_prod.occupied_len() as f32 / output.rate as f32 * 1000.0
            + 27.0; // signalsmith-stretch internal latency at 1024/256
        shared
            .latency_ms_bits
            .store(latency_ms.to_bits(), Ordering::Relaxed);

        // Publish playing positions every ~100 ms without blocking.
        blocks_since_publish += 1;
        if blocks_since_publish >= 10 {
            blocks_since_publish = 0;
            if let Some(mut playing) = shared.playing.try_lock() {
                playing.clear();
                playing.extend(voices.iter().map(|v| PlayingClip {
                    id: v.id.clone(),
                    position_ms: (v.pos as u64 * 1000) / INTERNAL_RATE as u64,
                }));
            }
        }
    }

    shared.running.store(false, Ordering::Relaxed);
    if let Some(mut playing) = shared.playing.try_lock() {
        playing.clear();
    }
    drop(input);
    drop(output);
    drop(monitor);
}

/// Soft clip: transparent below 0.95, tanh knee above (keeps `Clean` preset
/// bit-exact for normal levels, spec rule 8).
fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.95;
    if x.abs() <= KNEE {
        x
    } else {
        x.signum() * (KNEE + (1.0 - KNEE) * ((x.abs() - KNEE) / (1.0 - KNEE)).tanh())
    }
}

/// Stateful linear resampler for the live path. Keeps a fractional read
/// position across calls so blocks stay continuous.
struct StreamResampler {
    ratio: f64,
    pos: f64,
    prev: f32,
    has_prev: bool,
    passthrough: bool,
}

impl StreamResampler {
    fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            ratio: from_rate as f64 / to_rate as f64,
            pos: 0.0,
            prev: 0.0,
            has_prev: false,
            passthrough: from_rate == to_rate,
        }
    }

    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        // Virtual stream: prev + input. pos is relative to prev at index 0.
        let first = if self.has_prev { self.prev } else { 0.0 };
        let get = |idx: usize| -> f32 {
            if idx == 0 {
                first
            } else {
                input[(idx - 1).min(input.len() - 1)]
            }
        };
        let len = input.len() + usize::from(self.has_prev);
        if len < 2 {
            if !input.is_empty() {
                self.prev = input[input.len() - 1];
                self.has_prev = true;
            }
            return;
        }
        while self.pos + 1.0 < len as f64 {
            let idx = self.pos as usize;
            let frac = (self.pos - idx as f64) as f32;
            let a = get(idx);
            let b = get(idx + 1);
            out.push(a + (b - a) * frac);
            self.pos += self.ratio;
        }
        // Rebase position onto the last sample, which becomes `prev`.
        self.pos -= (len - 1) as f64;
        self.prev = input[input.len() - 1];
        self.has_prev = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_clip_is_transparent_below_knee() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert_eq!(soft_clip(-0.9), -0.9);
    }

    #[test]
    fn soft_clip_bounds_loud_input() {
        assert!(soft_clip(10.0) <= 1.0);
        assert!(soft_clip(-10.0) >= -1.0);
        assert!(soft_clip(10.0) > 0.95);
    }

    #[test]
    fn stream_resampler_rate_conversion_holds_over_blocks() {
        let mut rs = StreamResampler::new(44_100, 48_000);
        let input: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.001).sin()).collect();
        let mut out = Vec::new();
        for chunk in input.chunks(441) {
            rs.process(chunk, &mut out);
        }
        // 1 second in → ~1 second out at the new rate.
        assert!((out.len() as i64 - 48_000).unsigned_abs() < 100);
    }

    #[test]
    fn stream_resampler_passthrough_is_exact() {
        let mut rs = StreamResampler::new(48_000, 48_000);
        let input: Vec<f32> = (0..480).map(|i| i as f32).collect();
        let mut out = Vec::new();
        rs.process(&input, &mut out);
        assert_eq!(out, input);
    }
}
