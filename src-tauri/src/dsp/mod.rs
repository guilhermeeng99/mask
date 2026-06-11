//! DSP voice effects: pitch/formant shift (signalsmith-stretch) plus a fixed
//! chain of extra effects. Contract: docs/specs/dsp_effects.md.

mod effects;
mod presets;

pub use presets::{builtin_presets, DspPreset, ExtraEffect};

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use effects::{Biquad, Distortion, Reverb, RingMod};
use signalsmith_stretch::Stretch;

/// Atomically shared live parameters (pitch/formant sliders write, audio reads).
pub struct DspParams {
    pitch_bits: AtomicU32,
    formant_bits: AtomicU32,
}

impl DspParams {
    pub fn new(pitch: f32, formant: f32) -> Arc<Self> {
        Arc::new(Self {
            pitch_bits: AtomicU32::new(pitch.to_bits()),
            formant_bits: AtomicU32::new(formant.to_bits()),
        })
    }

    pub fn set(&self, pitch: f32, formant: f32) {
        // Clamp at the boundary so the audio thread never sees wild values.
        let pitch = pitch.clamp(-12.0, 12.0);
        let formant = formant.clamp(-12.0, 12.0);
        self.pitch_bits.store(pitch.to_bits(), Ordering::Relaxed);
        self.formant_bits
            .store(formant.to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> (f32, f32) {
        (
            f32::from_bits(self.pitch_bits.load(Ordering::Relaxed)),
            f32::from_bits(self.formant_bits.load(Ordering::Relaxed)),
        )
    }
}

enum BuiltEffect {
    RingMod(RingMod),
    Distortion(Distortion),
    Reverb(Reverb),
    Filter(Biquad),
}

impl BuiltEffect {
    fn process(&mut self, buf: &mut [f32]) {
        match self {
            BuiltEffect::RingMod(e) => e.process(buf),
            BuiltEffect::Distortion(e) => e.process(buf),
            BuiltEffect::Reverb(e) => e.process(buf),
            BuiltEffect::Filter(e) => e.process(buf),
        }
    }
}

/// One fully-built voice chain: shift engine + extra effects, mono.
/// Construction allocates; `process` does not.
pub struct DspChain {
    stretch: Stretch,
    extras: Vec<BuiltEffect>,
    params: Arc<DspParams>,
    last_pitch: f32,
    last_formant: f32,
    /// True bypass when the whole chain is neutral (rule 8).
    neutral_extras: bool,
}

impl DspChain {
    /// `block` and `interval` trade quality for latency; 1024/256 at 48 kHz
    /// adds ~27 ms, inside the 50 ms budget with 10 ms device buffers.
    pub fn new(sample_rate: u32, preset: &DspPreset, params: Arc<DspParams>) -> Self {
        let mut stretch = Stretch::new(1, 1024, 256);
        let (pitch, formant) = (preset.pitch, preset.formant);
        stretch.set_transpose_factor_semitones(pitch, Some(8000.0 / sample_rate as f32));
        stretch.set_formant_factor_semitones(formant, false);
        params.set(pitch, formant);

        let extras: Vec<BuiltEffect> = preset
            .effects
            .iter()
            .map(|fx| match *fx {
                ExtraEffect::RingMod { freq_hz } => {
                    BuiltEffect::RingMod(RingMod::new(sample_rate, freq_hz))
                }
                ExtraEffect::Distortion { drive } => {
                    BuiltEffect::Distortion(Distortion::new(drive))
                }
                ExtraEffect::Reverb { mix, decay } => {
                    BuiltEffect::Reverb(Reverb::new(sample_rate, mix, decay))
                }
                ExtraEffect::HighPass { cutoff_hz } => {
                    BuiltEffect::Filter(Biquad::high_pass(sample_rate, cutoff_hz))
                }
                ExtraEffect::LowPass { cutoff_hz } => {
                    BuiltEffect::Filter(Biquad::low_pass(sample_rate, cutoff_hz))
                }
            })
            .collect();

        Self {
            stretch,
            neutral_extras: extras.is_empty(),
            extras,
            params,
            last_pitch: pitch,
            last_formant: formant,
        }
    }

    /// Process one mono block in place. Real-time safe.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let (pitch, formant) = self.params.get();
        if pitch != self.last_pitch || formant != self.last_formant {
            // Stretch smooths parameter changes internally; no zipper noise.
            self.stretch
                .set_transpose_factor_semitones(pitch, Some(8000.0 / 48_000.0));
            self.stretch.set_formant_factor_semitones(formant, false);
            self.last_pitch = pitch;
            self.last_formant = formant;
        }

        if pitch == 0.0 && formant == 0.0 && self.neutral_extras {
            output.copy_from_slice(input);
            return;
        }

        self.stretch.process(input, &mut output[..]);
        for fx in &mut self.extras {
            fx.process(output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, sample_rate: u32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
            .collect()
    }

    /// Dominant frequency via naive DFT peak scan (test-only, slow but exact enough).
    fn dominant_freq(buf: &[f32], sample_rate: u32) -> f32 {
        let n = buf.len();
        let mut best = (0.0f32, 0.0f32);
        let mut f = 50.0;
        while f < 2000.0 {
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, s) in buf.iter().enumerate() {
                let w = 2.0 * std::f32::consts::PI * f * i as f32 / sample_rate as f32;
                re += s * w.cos();
                im += s * w.sin();
            }
            let mag = re * re + im * im;
            if mag > best.1 {
                best = (f, mag);
            }
            f += 5.0;
        }
        let _ = n;
        best.0
    }

    fn neutral_preset() -> DspPreset {
        DspPreset {
            id: "none".into(),
            name: "Clean".into(),
            pitch: 0.0,
            formant: 0.0,
            effects: vec![],
            is_builtin: true,
        }
    }

    #[test]
    fn neutral_chain_is_bit_exact_passthrough() {
        let params = DspParams::new(0.0, 0.0);
        let mut chain = DspChain::new(48_000, &neutral_preset(), params);
        let input = sine(440.0, 48_000, 480);
        let mut output = vec![0.0f32; 480];
        chain.process(&input, &mut output);
        assert_eq!(input, output);
    }

    #[test]
    fn pitch_up_one_octave_doubles_frequency() {
        let params = DspParams::new(12.0, 0.0);
        let preset = DspPreset {
            pitch: 12.0,
            ..neutral_preset()
        };
        let mut chain = DspChain::new(48_000, &preset, params);
        let input = sine(220.0, 48_000, 48_000);
        let mut output = vec![0.0f32; 48_000];
        // Feed in blocks like the real pipeline does.
        for (inb, outb) in input.chunks(480).zip(output.chunks_mut(480)) {
            chain.process(inb, outb);
        }
        // Skip the latency head, measure the steady tail.
        let tail = &output[24_000..];
        let f = dominant_freq(tail, 48_000);
        assert!(
            (f - 440.0).abs() < 25.0,
            "expected ~440 Hz after +12 st, got {f}"
        );
    }

    #[test]
    fn params_clamp_to_valid_range() {
        let params = DspParams::new(0.0, 0.0);
        params.set(99.0, -99.0);
        assert_eq!(params.get(), (12.0, -12.0));
    }
}
