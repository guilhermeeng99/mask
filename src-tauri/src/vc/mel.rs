//! Mel spectrogram + salience decoding for the RMVPE pitch model.
//!
//! Mirrors RVC's `infer/lib/rmvpe.py` preprocessing: 16 kHz audio → STFT
//! (n_fft 1024, hop 160, Hann, centered/reflect-padded) → 128 HTK mel bins
//! (30–8000 Hz, Slaney area normalization) → log clamped at 1e-5. The model
//! returns a [T, 360] salience map decoded to f0 via local weighted-average
//! cents (20-cent bins anchored at 1997.379 cents ≈ 31.7 Hz).

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

const SAMPLE_RATE: f32 = 16_000.0;
const N_FFT: usize = 1024;
const HOP: usize = 160;
const N_MELS: usize = 128;
const FMIN: f32 = 30.0;
const FMAX: f32 = 8_000.0;
const N_BINS: usize = N_FFT / 2 + 1;

const SALIENCE_BINS: usize = 360;
const CENTS_PER_BIN: f32 = 20.0;
const CENTS_OFFSET: f32 = 1_997.379_4;
/// Frames whose peak salience falls below this are unvoiced (rmvpe.py thred).
const VOICED_THRESHOLD: f32 = 0.03;

fn hz_to_mel(hz: f32) -> f32 {
    2_595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2_595.0) - 1.0)
}

/// Reusable mel extractor (FFT plan, window, and filterbank built once).
pub struct MelExtractor {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    /// Triangular filters as (first_bin, weights) per mel channel.
    filters: Vec<(usize, Vec<f32>)>,
}

impl Default for MelExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl MelExtractor {
    pub fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(N_FFT);
        let window: Vec<f32> = (0..N_FFT)
            .map(|i| {
                // Periodic Hann, matching torch.hann_window.
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos()
            })
            .collect();

        // HTK mel points with Slaney area normalization (librosa defaults
        // used by rmvpe.py apart from htk=True).
        let mel_min = hz_to_mel(FMIN);
        let mel_max = hz_to_mel(FMAX);
        let band_edges: Vec<f32> = (0..N_MELS + 2)
            .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f32 / (N_MELS + 1) as f32))
            .collect();
        let bin_freq = |k: usize| k as f32 * SAMPLE_RATE / N_FFT as f32;

        let mut filters = Vec::with_capacity(N_MELS);
        for m in 0..N_MELS {
            let (lo, center, hi) = (band_edges[m], band_edges[m + 1], band_edges[m + 2]);
            let norm = 2.0 / (hi - lo);
            let mut first_bin = None;
            let mut weights = Vec::new();
            for k in 0..N_BINS {
                let f = bin_freq(k);
                let w = if f <= lo || f >= hi {
                    0.0
                } else if f <= center {
                    (f - lo) / (center - lo)
                } else {
                    (hi - f) / (hi - center)
                };
                if w > 0.0 {
                    if first_bin.is_none() {
                        first_bin = Some(k);
                    }
                    weights.push(w * norm);
                } else if first_bin.is_some() {
                    break;
                }
            }
            filters.push((first_bin.unwrap_or(0), weights));
        }

        Self {
            fft,
            window,
            filters,
        }
    }

    /// Log-mel spectrogram in the model's [1, 128, T] layout (row-major
    /// `mel * frames + t`). Returns (data, frames).
    pub fn log_mel(&self, audio_16k: &[f32]) -> (Vec<f32>, usize) {
        // Centered STFT: reflect-pad by n_fft/2 on both ends.
        let pad = N_FFT / 2;
        let mut padded = Vec::with_capacity(audio_16k.len() + N_FFT);
        for i in (1..=pad).rev() {
            padded.push(*audio_16k.get(i).unwrap_or(&0.0));
        }
        padded.extend_from_slice(audio_16k);
        for i in (1..=pad).rev() {
            let idx = audio_16k.len().saturating_sub(1 + i);
            padded.push(*audio_16k.get(idx).unwrap_or(&0.0));
        }

        let frames = if padded.len() >= N_FFT {
            (padded.len() - N_FFT) / HOP + 1
        } else {
            0
        };
        let mut mel = vec![0.0f32; N_MELS * frames];
        let mut buf = vec![Complex::new(0.0f32, 0.0f32); N_FFT];
        let mut power = vec![0.0f32; N_BINS];

        for t in 0..frames {
            let start = t * HOP;
            for (i, slot) in buf.iter_mut().enumerate() {
                *slot = Complex::new(padded[start + i] * self.window[i], 0.0);
            }
            self.fft.process(&mut buf);
            for (k, p) in power.iter_mut().enumerate() {
                *p = buf[k].norm();
            }
            for (m, (first_bin, weights)) in self.filters.iter().enumerate() {
                let mut acc = 0.0f32;
                for (j, w) in weights.iter().enumerate() {
                    acc += power[first_bin + j] * w;
                }
                mel[m * frames + t] = acc.max(1e-5).ln();
            }
        }
        (mel, frames)
    }
}

/// Decode the RMVPE salience map [frames, 360] into f0 Hz per frame:
/// weighted average of cents over ±4 bins around the argmax; unvoiced → 0.
pub fn decode_salience(salience: &[f32], frames: usize) -> Vec<f32> {
    let mut f0 = vec![0.0f32; frames];
    for (t, out) in f0.iter_mut().enumerate() {
        let row = &salience[t * SALIENCE_BINS..(t + 1) * SALIENCE_BINS];
        let (peak_idx, peak) = row
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        if *peak < VOICED_THRESHOLD {
            continue;
        }
        let lo = peak_idx.saturating_sub(4);
        let hi = (peak_idx + 5).min(SALIENCE_BINS);
        let (mut weighted, mut total) = (0.0f32, 0.0f32);
        for (i, s) in row[lo..hi].iter().enumerate() {
            let cents = CENTS_PER_BIN * (lo + i) as f32 + CENTS_OFFSET;
            weighted += cents * s;
            total += s;
        }
        let cents = weighted / total;
        *out = 10.0 * 2f32.powf(cents / 1_200.0);
    }
    f0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_mel_shape_matches_hop_math() {
        let extractor = MelExtractor::new();
        let audio = vec![0.1f32; 16_000];
        let (mel, frames) = extractor.log_mel(&audio);
        // Centered STFT: 1 + N/hop frames.
        assert_eq!(frames, 16_000 / HOP + 1);
        assert_eq!(mel.len(), N_MELS * frames);
    }

    #[test]
    fn sine_concentrates_energy_in_the_right_mel_band() {
        let extractor = MelExtractor::new();
        let audio: Vec<f32> = (0..16_000)
            .map(|i| (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SAMPLE_RATE).sin())
            .collect();
        let (mel, frames) = extractor.log_mel(&audio);
        // Find the strongest mel channel mid-signal.
        let t = frames / 2;
        let (best_mel, _) = (0..N_MELS)
            .map(|m| (m, mel[m * frames + t]))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        let center = mel_to_hz(
            hz_to_mel(FMIN)
                + (hz_to_mel(FMAX) - hz_to_mel(FMIN)) * (best_mel + 1) as f32 / (N_MELS + 1) as f32,
        );
        assert!(
            (800.0..1_250.0).contains(&center),
            "1 kHz sine peaked at mel centered {center} Hz"
        );
    }

    #[test]
    fn decode_salience_recovers_a_known_pitch() {
        // Build a synthetic one-frame salience with a peak at the bin for 220 Hz.
        let cents = 1_200.0 * (220.0f32 / 10.0).log2();
        let bin = ((cents - CENTS_OFFSET) / CENTS_PER_BIN).round() as usize;
        let mut salience = vec![0.0f32; SALIENCE_BINS];
        salience[bin] = 1.0;
        salience[bin - 1] = 0.5;
        salience[bin + 1] = 0.5;
        let f0 = decode_salience(&salience, 1);
        assert!((f0[0] - 220.0).abs() < 5.0, "decoded {}", f0[0]);
    }

    #[test]
    fn decode_salience_marks_quiet_frames_unvoiced() {
        let salience = vec![0.001f32; SALIENCE_BINS];
        let f0 = decode_salience(&salience, 1);
        assert_eq!(f0[0], 0.0);
    }
}
