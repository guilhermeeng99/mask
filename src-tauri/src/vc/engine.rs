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
    model: Session,
    model_sr: u32,
    pitch_shift: f32,
    has_pitch_inputs: bool,
    feat_dim: usize,
    prev_tail: Vec<f32>,
}

impl RvcSession {
    pub fn load(
        contentvec_path: &Path,
        rmvpe_path: &Path,
        model_path: &Path,
        model_sr: u32,
        pitch_semitones: i32,
        backend: InferenceBackend,
    ) -> Result<Self, VcError> {
        let contentvec = build_session(contentvec_path, backend)?;
        let rmvpe = build_session(rmvpe_path, backend)?;
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

        Ok(Self {
            contentvec,
            rmvpe,
            model,
            model_sr,
            pitch_shift: 2f32.powf(pitch_semitones as f32 / 12.0),
            has_pitch_inputs,
            feat_dim: 768,
            prev_tail: Vec::new(),
        })
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
        // ContentVec ONNX (w-okada export): input [1, 1, N] f32 → [1, T, 768].
        let input = Tensor::from_array((
            [1usize, 1, audio_16k.len()],
            audio_16k.to_vec().into_boxed_slice(),
        ))
        .map_err(|e| VcError::Runtime(e.to_string()))?;
        let name = self.contentvec.inputs()[0].name().to_string();
        let outputs = self
            .contentvec
            .run(ort::inputs![name.as_str() => input])
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        let feat_dim = *dims
            .last()
            .ok_or_else(|| VcError::Runtime("contentvec returned a scalar output".into()))?;
        self.feat_dim = feat_dim;
        // RVC consumes features at 100 fps; ContentVec emits 50 fps → repeat 2x.
        let frames = data.len() / feat_dim;
        let mut doubled = Vec::with_capacity(data.len() * 2);
        for f in 0..frames {
            let row = &data[f * feat_dim..(f + 1) * feat_dim];
            doubled.extend_from_slice(row);
            doubled.extend_from_slice(row);
        }
        Ok(doubled)
    }

    fn extract_pitch(&mut self, audio_16k: &[f32], frames: usize) -> Result<Vec<f32>, VcError> {
        // RMVPE ONNX: input [1, N] f32 16 kHz → f0 [1, T] Hz (10 ms hop).
        let input = Tensor::from_array((
            [1usize, audio_16k.len()],
            audio_16k.to_vec().into_boxed_slice(),
        ))
        .map_err(|e| VcError::Runtime(e.to_string()))?;
        let name = self.rmvpe.inputs()[0].name().to_string();
        let outputs = self
            .rmvpe
            .run(ort::inputs![name.as_str() => input])
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VcError::Runtime(e.to_string()))?;

        // Apply the pitch shift and stretch/trim to the feature frame count.
        let mut f0 = vec![0.0f32; frames];
        if !data.is_empty() {
            for (i, dst) in f0.iter_mut().enumerate() {
                let src = (i * data.len()) / frames;
                *dst = data[src.min(data.len() - 1)] * self.pitch_shift;
            }
        }
        Ok(f0)
    }

    fn run_model(&mut self, feats: &[f32], frames: usize, f0: &[f32]) -> Result<Vec<f32>, VcError> {
        let feat_dim = self.feat_dim;
        let phone = Tensor::from_array((
            [1usize, frames, feat_dim],
            feats.to_vec().into_boxed_slice(),
        ))
        .map_err(|e| VcError::Runtime(e.to_string()))?;
        let phone_lengths = Tensor::from_array(([1usize], vec![frames as i64].into_boxed_slice()))
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        let ds = Tensor::from_array(([1usize], vec![0i64].into_boxed_slice()))
            .map_err(|e| VcError::Runtime(e.to_string()))?;

        let outputs = if self.has_pitch_inputs {
            let coarse: Vec<i64> = f0.iter().map(|hz| coarse_pitch(*hz)).collect();
            let pitch = Tensor::from_array(([1usize, frames], coarse.into_boxed_slice()))
                .map_err(|e| VcError::Runtime(e.to_string()))?;
            let pitchf = Tensor::from_array(([1usize, frames], f0.to_vec().into_boxed_slice()))
                .map_err(|e| VcError::Runtime(e.to_string()))?;
            self.model
                .run(ort::inputs![
                    "phone" => phone,
                    "phone_lengths" => phone_lengths,
                    "pitch" => pitch,
                    "pitchf" => pitchf,
                    "ds" => ds,
                ])
                .map_err(|e| VcError::Runtime(e.to_string()))?
        } else {
            self.model
                .run(ort::inputs![
                    "phone" => phone,
                    "phone_lengths" => phone_lengths,
                    "ds" => ds,
                ])
                .map_err(|e| VcError::Runtime(e.to_string()))?
        };

        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VcError::Runtime(e.to_string()))?;
        Ok(data.to_vec())
    }
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
