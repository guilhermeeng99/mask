//! RVC streaming inference engine.
//!
//! Pipeline (ai_voice_conversion.md): 48 kHz voice chunks → resample 16 kHz →
//! ContentVec features + RMVPE pitch (both ONNX) → RVC model (ONNX) →
//! resample back to 48 kHz, crossfaded at chunk seams. Runs on a dedicated
//! inference thread fed by lock-free rings; the audio engine only moves
//! samples (CLAUDE.md real-time rules).
//!
//! ONNX layouts follow the RVC-Project ONNX export / w-okada conventions:
//! the graphs are introspected by input name at load and unknown layouts are
//! rejected with a clear error instead of garbage audio.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ort::execution_providers::ExecutionProvider;
use ort::session::Session;
use ort::value::Tensor;
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use crate::audio::pipeline::INTERNAL_RATE;
use crate::soundboard::decode::resample_linear;

use super::mel::{decode_salience, MelExtractor};
use super::{InferenceBackend, VcError};

const FEATURE_RATE: u32 = 16_000;
/// Crossfade between consecutive chunks, in 48 kHz samples (10 ms).
const FADE: usize = 480;

/// Detect the best available execution provider (spec rule 1).
pub fn detect_backend() -> InferenceBackend {
    if ort::execution_providers::CUDAExecutionProvider::default()
        .is_available()
        .unwrap_or(false)
    {
        InferenceBackend::Cuda
    } else if ort::execution_providers::DirectMLExecutionProvider::default()
        .is_available()
        .unwrap_or(false)
    {
        InferenceBackend::DirectMl
    } else {
        InferenceBackend::Cpu
    }
}

fn build_session(path: &Path, backend: InferenceBackend) -> Result<Session, VcError> {
    let builder = Session::builder().map_err(|e| VcError::Runtime(e.to_string()))?;
    let mut builder = match backend {
        InferenceBackend::Cuda => builder
            .with_execution_providers([
                ort::execution_providers::CUDAExecutionProvider::default().build(),
                ort::execution_providers::DirectMLExecutionProvider::default().build(),
            ])
            .map_err(|e| VcError::Runtime(e.to_string()))?,
        InferenceBackend::DirectMl => builder
            .with_execution_providers([
                ort::execution_providers::DirectMLExecutionProvider::default().build(),
            ])
            .map_err(|e| VcError::Runtime(e.to_string()))?,
        InferenceBackend::Cpu => builder,
    };
    builder
        .commit_from_file(path)
        .map_err(|e| VcError::InvalidModel(e.to_string()))
}

/// Validate an ONNX file by building a CPU session for it (used at import).
pub fn validate_onnx(path: &Path) -> Result<(), VcError> {
    build_session(path, InferenceBackend::Cpu).map(|_| ())
}

/// The three sessions plus the streaming state for one active voice.
pub struct RvcSession {
    contentvec: Session,
    rmvpe: Session,
    mel: MelExtractor,
    model: Session,
    model_sr: u32,
    pitch_shift: f32,
    has_pitch_inputs: bool,
    has_rnd_input: bool,
    feat_dim: usize,
    prev_tail: Vec<f32>,
    noise_state: u64,
}

impl RvcSession {
    pub fn load(
        companions: &crate::vc::CompanionPaths,
        model_path: &Path,
        model_sr: u32,
        pitch_semitones: i32,
        backend: InferenceBackend,
    ) -> Result<Self, VcError> {
        let model = build_session(model_path, backend)?;

        let input_names: Vec<String> = model
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        let has_phone = input_names.iter().any(|n| n == "phone" || n == "feats");
        if !has_phone {
            return Err(VcError::InvalidModel(format!(
                "unrecognized RVC graph inputs: {input_names:?} (expected phone/feats)"
            )));
        }
        let has_pitch_inputs = input_names.iter().any(|n| n == "pitch");
        let has_rnd_input = input_names.iter().any(|n| n == "rnd");

        // The phone input's last dim tells which encoder family the model
        // was trained on (768 = vec-768-layer-12, 256 = vec-256-layer-9);
        // pick the matching companion encoder.
        let phone_dim = model
            .inputs()
            .iter()
            .find(|i| i.name() == "phone" || i.name() == "feats")
            .and_then(|i| match i.dtype() {
                ort::value::ValueType::Tensor { shape, .. } => {
                    shape.last().copied().filter(|d| *d > 0).map(|d| d as usize)
                }
                _ => None,
            })
            .unwrap_or(768);
        let encoder_path = if phone_dim == 256 {
            &companions.contentvec256
        } else {
            &companions.contentvec
        };
        if !encoder_path.exists() {
            return Err(VcError::Runtime(format!(
                "this voice expects a {phone_dim}-dim encoder which is not installed; \
                 run the AI components download again"
            )));
        }
        let contentvec = build_session(encoder_path, backend)?;
        let rmvpe = build_session(&companions.rmvpe, backend)?;

        let mut session = Self {
            has_rnd_input,
            contentvec,
            rmvpe,
            mel: MelExtractor::new(),
            model,
            model_sr,
            pitch_shift: 2f32.powf(pitch_semitones as f32 / 12.0),
            has_pitch_inputs,
            feat_dim: phone_dim,
            prev_tail: Vec::new(),
            noise_state: 0x9e37_79b9_7f4a_7c15,
        };

        // Dry-run the voice model once: some RVC graphs crash on DirectML
        // (attention Reshape), and a CUDA registration can silently fall back
        // to DML. If the GPU path dies, rebuild the voice model on CPU —
        // companions stay on the GPU where they are fine.
        let expected_dim = session.feat_dim;
        let dry_feats = vec![0.0f32; 100 * expected_dim];
        let dry_f0 = vec![220.0f32; 100];
        if let Err(first) = session.run_model(&dry_feats, 100, &dry_f0) {
            session.model = build_session(model_path, InferenceBackend::Cpu)?;
            session
                .run_model(&dry_feats, 100, &dry_f0)
                .map_err(|second| {
                    VcError::Runtime(format!(
                        "voice model failed on {backend:?} ({first}) and on CPU ({second})"
                    ))
                })?;
        }
        session.prev_tail.clear();
        Ok(session)
    }

    /// Convert one 48 kHz mono chunk. Returns 48 kHz samples, crossfaded
    /// against the previous chunk's tail.
    pub fn process(&mut self, chunk_48k: &[f32]) -> Result<Vec<f32>, VcError> {
        let audio_16k = resample_linear(chunk_48k, INTERNAL_RATE, FEATURE_RATE);

        let feats = self.extract_features(&audio_16k)?;
        let frames = feats.len() / self.feat_dim;
        let f0 = self.extract_pitch(&audio_16k, frames)?;

        let out_model = self.run_model(&feats, frames, &f0)?;
        let mut out = resample_linear(&out_model, self.model_sr, INTERNAL_RATE);

        // Crossfade against the previous chunk tail (spec: chunked streaming
        // with crossfaded seams).
        if !self.prev_tail.is_empty() {
            let n = self.prev_tail.len().min(out.len());
            for (i, (sample, tail)) in out[..n].iter_mut().zip(&self.prev_tail).enumerate() {
                let t = i as f32 / n as f32;
                *sample = tail * (1.0 - t) + *sample * t;
            }
        }
        if out.len() > FADE {
            self.prev_tail = out.split_off(out.len() - FADE);
        } else {
            self.prev_tail.clear();
        }
        Ok(out)
    }

    fn extract_features(&mut self, audio_16k: &[f32]) -> Result<Vec<f32>, VcError> {
        let (encoder_dim, feats) = run_contentvec(&mut self.contentvec, audio_16k)?;
        if encoder_dim != self.feat_dim {
            return Err(VcError::InvalidModel(format!(
                "this voice model expects {}-dim features but the encoder produces {}-dim \
                 (wrong ContentVec variant for this model)",
                self.feat_dim, encoder_dim
            )));
        }
        Ok(feats)
    }

    fn extract_pitch(&mut self, audio_16k: &[f32], frames: usize) -> Result<Vec<f32>, VcError> {
        run_rmvpe(
            &mut self.rmvpe,
            &self.mel,
            audio_16k,
            frames,
            self.pitch_shift,
        )
    }

    /// Load only the two companion models and run them on raw audio.
    /// Used by the local-hardware integration test to validate the real
    /// downloaded files end-to-end without needing a voice model.
    /// Returns (feature dim, feature frames, median voiced f0 in Hz).
    pub fn probe_companions(
        contentvec_path: &Path,
        rmvpe_path: &Path,
        backend: InferenceBackend,
        audio_16k: &[f32],
    ) -> Result<(usize, usize, f32), VcError> {
        let mut contentvec = build_session(contentvec_path, backend)?;
        let mut rmvpe = build_session(rmvpe_path, backend)?;
        let mel = MelExtractor::new();
        let (feat_dim, feats) = run_contentvec(&mut contentvec, audio_16k)?;
        let frames = feats.len() / feat_dim;
        let f0 = run_rmvpe(&mut rmvpe, &mel, audio_16k, frames, 1.0)?;
        let mut voiced: Vec<f32> = f0.into_iter().filter(|hz| *hz > 0.0).collect();
        voiced.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = voiced.get(voiced.len() / 2).copied().unwrap_or(0.0);
        Ok((feat_dim, frames, median))
    }

    /// Gaussian noise for the model's `rnd` latent input (RVC's
    /// onnx_inference feeds `np.random.randn(1, 192, T)`).
    fn gaussian_noise(&mut self, len: usize) -> Vec<f32> {
        let mut next = || {
            // xorshift64* — quality is irrelevant here, the model just needs
            // non-degenerate noise.
            self.noise_state ^= self.noise_state << 13;
            self.noise_state ^= self.noise_state >> 7;
            self.noise_state ^= self.noise_state << 17;
            (self.noise_state >> 11) as f32 / (1u64 << 53) as f32
        };
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            // Box-Muller.
            let u1 = next().max(1e-12);
            let u2 = next();
            let r = (-2.0 * u1.ln()).sqrt();
            let theta = 2.0 * std::f32::consts::PI * u2;
            out.push(r * theta.cos());
            if out.len() < len {
                out.push(r * theta.sin());
            }
        }
        out
    }

    fn run_model(&mut self, feats: &[f32], frames: usize, f0: &[f32]) -> Result<Vec<f32>, VcError> {
        let feat_dim = self.feat_dim;
        let err = |e: ort::Error| VcError::Runtime(e.to_string());
        let phone = Tensor::from_array((
            [1usize, frames, feat_dim],
            feats.to_vec().into_boxed_slice(),
        ))
        .map_err(err)?;
        let phone_lengths =
            Tensor::from_array(([1usize], vec![frames as i64].into_boxed_slice())).map_err(err)?;
        let ds = Tensor::from_array(([1usize], vec![0i64].into_boxed_slice())).map_err(err)?;

        // Build the input set dynamically: graphs vary in whether they take
        // pitch inputs (no-f0 models) and the `rnd` latent.
        let mut inputs: Vec<(&str, ort::session::SessionInputValue<'_>)> = vec![
            ("phone", phone.into()),
            ("phone_lengths", phone_lengths.into()),
        ];
        if self.has_pitch_inputs {
            let coarse: Vec<i64> = f0.iter().map(|hz| coarse_pitch(*hz)).collect();
            let pitch =
                Tensor::from_array(([1usize, frames], coarse.into_boxed_slice())).map_err(err)?;
            let pitchf = Tensor::from_array(([1usize, frames], f0.to_vec().into_boxed_slice()))
                .map_err(err)?;
            inputs.push(("pitch", pitch.into()));
            inputs.push(("pitchf", pitchf.into()));
        }
        inputs.push(("ds", ds.into()));
        if self.has_rnd_input {
            let noise = self.gaussian_noise(192 * frames);
            let rnd = Tensor::from_array(([1usize, 192, frames], noise.into_boxed_slice()))
                .map_err(err)?;
            inputs.push(("rnd", rnd.into()));
        }

        let outputs = self.model.run(inputs).map_err(err)?;
        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        Ok(data.to_vec())
    }
}

/// Rank of the first graph input (audio models vary between [1,N] and [1,1,N]).
fn first_input_rank(session: &Session) -> usize {
    match session.inputs()[0].dtype() {
        ort::value::ValueType::Tensor { shape, .. } => shape.len(),
        _ => 2,
    }
}

/// Run a single-audio-input ONNX graph, adapting the tensor rank to what the
/// graph declares (rank 2 → [1,N], rank 3 → [1,1,N]).
fn run_audio_graph<'s>(
    session: &'s mut Session,
    audio_16k: &[f32],
) -> Result<ort::session::SessionOutputs<'s>, VcError> {
    let name = session.inputs()[0].name().to_string();
    let rank = first_input_rank(session);
    let data = audio_16k.to_vec().into_boxed_slice();
    let input = match rank {
        3 => Tensor::from_array(([1usize, 1, audio_16k.len()], data)),
        _ => Tensor::from_array(([1usize, audio_16k.len()], data)),
    }
    .map_err(|e| VcError::Runtime(e.to_string()))?;
    session
        .run(ort::inputs![name.as_str() => input])
        .map_err(|e| VcError::Runtime(e.to_string()))
}

/// ContentVec ONNX (community export): 16 kHz audio in →
/// [1, T, 768] features at 50 fps. RVC consumes 100 fps, so rows repeat 2x.
fn run_contentvec(session: &mut Session, audio_16k: &[f32]) -> Result<(usize, Vec<f32>), VcError> {
    let outputs = run_audio_graph(session, audio_16k)?;
    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| VcError::Runtime(e.to_string()))?;
    let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
    let feat_dim = *dims
        .last()
        .ok_or_else(|| VcError::Runtime("contentvec returned a scalar output".into()))?;
    let frames = data.len() / feat_dim;
    let mut doubled = Vec::with_capacity(data.len() * 2);
    for f in 0..frames {
        let row = &data[f * feat_dim..(f + 1) * feat_dim];
        doubled.extend_from_slice(row);
        doubled.extend_from_slice(row);
    }
    Ok((feat_dim, doubled))
}

/// True when the graph expects a [1, 128, T] mel spectrogram (the official
/// RVC rmvpe.onnx export) rather than raw audio.
fn rmvpe_wants_mel(session: &Session) -> bool {
    match session.inputs()[0].dtype() {
        ort::value::ValueType::Tensor { shape, .. } => shape.len() == 3 && shape[1] == 128,
        _ => false,
    }
}

/// RMVPE: 16 kHz audio → f0 in Hz per 10 ms frame. Mel-input exports get the
/// full mel + salience-decode path; raw-audio exports are fed directly. The
/// result is stretched/trimmed to `frames` and multiplied by `pitch_shift`.
fn run_rmvpe(
    session: &mut Session,
    mel_extractor: &MelExtractor,
    audio_16k: &[f32],
    frames: usize,
    pitch_shift: f32,
) -> Result<Vec<f32>, VcError> {
    let f0_raw: Vec<f32> = if rmvpe_wants_mel(session) {
        let (mel, t) = mel_extractor.log_mel(audio_16k);
        if t == 0 {
            return Ok(vec![0.0; frames]);
        }
        // The U-Net needs T padded to a multiple of 32 (rmvpe.py pads with
        // zeros); the output is sliced back to the true frame count.
        let t_pad = t.div_ceil(32) * 32;
        let mut padded = vec![0.0f32; 128 * t_pad];
        for m in 0..128 {
            padded[m * t_pad..m * t_pad + t].copy_from_slice(&mel[m * t..(m + 1) * t]);
        }
        let name = session.inputs()[0].name().to_string();
        let input = Tensor::from_array(([1usize, 128, t_pad], padded.into_boxed_slice()))
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        let outputs = session
            .run(ort::inputs![name.as_str() => input])
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        let (_, salience) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        let frames_out = (salience.len() / 360).min(t);
        decode_salience(&salience[..frames_out * 360], frames_out)
    } else {
        let outputs = run_audio_graph(session, audio_16k)?;
        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        data.to_vec()
    };

    let mut f0 = vec![0.0f32; frames];
    if !f0_raw.is_empty() {
        for (i, dst) in f0.iter_mut().enumerate() {
            let src = (i * f0_raw.len()) / frames;
            *dst = f0_raw[src.min(f0_raw.len() - 1)] * pitch_shift;
        }
    }
    Ok(f0)
}

/// RVC coarse pitch bucket: mel-scale quantization of f0 into 1..=255.
fn coarse_pitch(hz: f32) -> i64 {
    const F0_MIN: f32 = 50.0;
    const F0_MAX: f32 = 1100.0;
    if hz <= 0.0 {
        return 1;
    }
    let mel = 1127.0 * (1.0 + hz / 700.0).ln();
    let mel_min = 1127.0 * (1.0 + F0_MIN / 700.0).ln();
    let mel_max = 1127.0 * (1.0 + F0_MAX / 700.0).ln();
    let scaled = (mel - mel_min) * 254.0 / (mel_max - mel_min) + 1.0;
    scaled.clamp(1.0, 255.0).round() as i64
}

/// Lock-free link between the audio engine and the inference thread.
pub struct VcLink {
    pub to_vc: HeapProd<f32>,
    pub from_vc: HeapCons<f32>,
}

pub struct VcWorker {
    pub alive: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl VcWorker {
    pub fn stop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for VcWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Spawn the inference thread. Returns the link to hand to the audio engine
/// and a worker handle owning the thread.
pub fn spawn_worker(
    mut session: RvcSession,
    chunk_ms: u32,
    on_error: impl Fn(String) + Send + 'static,
) -> (VcLink, VcWorker) {
    let chunk_len = (INTERNAL_RATE as usize * chunk_ms as usize) / 1000;
    // Rings hold 4 chunks of slack on each side.
    let in_ring = HeapRb::<f32>::new(chunk_len * 4);
    let (to_vc, mut vc_in) = in_ring.split();
    let out_ring = HeapRb::<f32>::new(chunk_len * 4 + FADE);
    let (mut vc_out, from_vc) = out_ring.split();

    let alive = Arc::new(AtomicBool::new(true));
    let alive_thread = alive.clone();

    let join = std::thread::Builder::new()
        .name("mask-vc-inference".into())
        .spawn(move || {
            let mut chunk = vec![0.0f32; chunk_len];
            let mut filled = 0usize;
            while alive_thread.load(Ordering::Relaxed) {
                while filled < chunk_len {
                    match vc_in.try_pop() {
                        Some(s) => {
                            chunk[filled] = s;
                            filled += 1;
                        }
                        None => break,
                    }
                }
                if filled < chunk_len {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                filled = 0;
                match session.process(&chunk) {
                    Ok(out) => {
                        for s in out {
                            let _ = vc_out.try_push(s);
                        }
                    }
                    Err(e) => {
                        on_error(e.to_string());
                        return;
                    }
                }
            }
        })
        .expect("vc inference thread");

    (
        VcLink { to_vc, from_vc },
        VcWorker {
            alive,
            join: Some(join),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coarse_pitch_is_monotonic_and_bounded() {
        assert_eq!(coarse_pitch(0.0), 1);
        assert_eq!(coarse_pitch(40.0), 1);
        assert_eq!(coarse_pitch(2000.0), 255);
        let a = coarse_pitch(100.0);
        let b = coarse_pitch(200.0);
        let c = coarse_pitch(400.0);
        assert!(a < b && b < c);
        assert!((1..=255).contains(&a));
    }
}
