//! Extra mono effects, all preallocated at construction and allocation-free
//! in `process` (real-time rule, CLAUDE.md).

/// Ring modulator: multiplies the signal by a sine carrier ("robot").
pub struct RingMod {
    phase: f32,
    step: f32,
}

impl RingMod {
    pub fn new(sample_rate: u32, freq_hz: f32) -> Self {
        Self {
            phase: 0.0,
            step: 2.0 * std::f32::consts::PI * freq_hz / sample_rate as f32,
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf {
            *s *= self.phase.sin();
            self.phase += self.step;
            if self.phase > 2.0 * std::f32::consts::PI {
                self.phase -= 2.0 * std::f32::consts::PI;
            }
        }
    }
}

/// Soft-saturation distortion. `drive` 0..1 maps to gain 1..11 before tanh;
/// output is normalized so perceived level stays comparable.
pub struct Distortion {
    gain: f32,
    norm: f32,
}

impl Distortion {
    pub fn new(drive: f32) -> Self {
        let gain = 1.0 + drive.clamp(0.0, 1.0) * 10.0;
        Self {
            gain,
            norm: 1.0 / gain.tanh(),
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf {
            *s = (*s * self.gain).tanh() * self.norm;
        }
    }
}

/// RBJ biquad filter (low-pass / high-pass), Q = 0.707.
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn from_coeffs(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn low_pass(sample_rate: u32, cutoff_hz: f32) -> Self {
        let w = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2.recip());
        Self::from_coeffs(
            (1.0 - cos) / 2.0,
            1.0 - cos,
            (1.0 - cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    pub fn high_pass(sample_rate: u32, cutoff_hz: f32) -> Self {
        let w = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2.recip());
        Self::from_coeffs(
            (1.0 + cos) / 2.0,
            -(1.0 + cos),
            (1.0 + cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf {
            let x = *s;
            let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
                - self.a1 * self.y1
                - self.a2 * self.y2;
            self.x2 = self.x1;
            self.x1 = x;
            self.y2 = self.y1;
            self.y1 = y;
            *s = y;
        }
    }
}

/// Schroeder reverb: 4 parallel combs + 2 series allpasses. Small and cheap;
/// good enough for the "cave" preset, not a mastering reverb.
pub struct Reverb {
    combs: [Comb; 4],
    allpasses: [Allpass; 2],
    mix: f32,
}

impl Reverb {
    pub fn new(sample_rate: u32, mix: f32, decay_secs: f32) -> Self {
        // Classic comb tunings (in seconds), scaled to the actual sample rate.
        let tunings = [0.0297, 0.0371, 0.0411, 0.0437];
        let combs = tunings.map(|t| {
            let delay = (t * sample_rate as f32) as usize;
            // Feedback gain for -60 dB after `decay_secs`.
            let g = 10f32.powf(-3.0 * t / decay_secs.max(0.1));
            Comb::new(delay, g)
        });
        let allpasses = [
            Allpass::new((0.005 * sample_rate as f32) as usize),
            Allpass::new((0.0017 * sample_rate as f32) as usize),
        ];
        Self {
            combs,
            allpasses,
            mix: mix.clamp(0.0, 1.0),
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf {
            let dry = *s;
            let mut wet = 0.0;
            for comb in &mut self.combs {
                wet += comb.tick(dry);
            }
            wet *= 0.25;
            for ap in &mut self.allpasses {
                wet = ap.tick(wet);
            }
            *s = dry * (1.0 - self.mix) + wet * self.mix;
        }
    }
}

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    feedback: f32,
}

impl Comb {
    fn new(delay: usize, feedback: f32) -> Self {
        Self {
            buf: vec![0.0; delay.max(1)],
            pos: 0,
            feedback,
        }
    }

    fn tick(&mut self, input: f32) -> f32 {
        let out = self.buf[self.pos];
        self.buf[self.pos] = input + out * self.feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    const GAIN: f32 = 0.5;

    fn new(delay: usize) -> Self {
        Self {
            buf: vec![0.0; delay.max(1)],
            pos: 0,
        }
    }

    fn tick(&mut self, input: f32) -> f32 {
        let delayed = self.buf[self.pos];
        let out = -input * Self::GAIN + delayed;
        self.buf[self.pos] = input + delayed * Self::GAIN;
        self.pos = (self.pos + 1) % self.buf.len();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_pass_attenuates_high_frequencies() {
        let mut lp = Biquad::low_pass(48_000, 500.0);
        let high: Vec<f32> = (0..4800)
            .map(|i| (2.0 * std::f32::consts::PI * 8000.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buf = high.clone();
        lp.process(&mut buf);
        let in_rms: f32 = (high.iter().map(|s| s * s).sum::<f32>() / 4800.0).sqrt();
        let out_rms: f32 = (buf[2400..].iter().map(|s| s * s).sum::<f32>() / 2400.0).sqrt();
        assert!(out_rms < in_rms * 0.05, "8 kHz should be > 26 dB down");
    }

    #[test]
    fn distortion_keeps_signal_bounded() {
        let mut d = Distortion::new(1.0);
        let mut buf: Vec<f32> = (0..480).map(|i| (i as f32 / 480.0) * 4.0 - 2.0).collect();
        d.process(&mut buf);
        assert!(buf.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn reverb_impulse_has_a_tail() {
        let mut r = Reverb::new(48_000, 1.0, 2.0);
        let mut buf = vec![0.0f32; 48_000];
        buf[0] = 1.0;
        r.process(&mut buf);
        let tail_energy: f32 = buf[24_000..].iter().map(|s| s.abs()).sum();
        assert!(tail_energy > 0.0, "decay 2 s must still ring at 0.5 s");
    }
}
