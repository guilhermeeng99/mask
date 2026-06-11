//! Decode any supported audio file to the pipeline's internal format:
//! mono f32 at 48 kHz. Runs on command/decode threads, never the audio thread.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::SoundboardError;

const TARGET_RATE: u32 = 48_000;

pub fn decode_to_mono_48k(path: &Path) -> Result<Vec<f32>, SoundboardError> {
    let file = File::open(path).map_err(SoundboardError::Io)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| SoundboardError::Decode(e.to_string()))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| SoundboardError::Decode("no audio track".into()))?;
    let track_id = track.id;
    let source_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| SoundboardError::Decode("unknown sample rate".into()))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| SoundboardError::Decode(e.to_string()))?;

    let mut mono: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(SoundboardError::Decode(e.to_string())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => append_downmixed(&decoded, &mut mono),
            // A corrupt packet mid-file is skippable; a decode reset is fatal.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(SoundboardError::Decode(e.to_string())),
        }
    }

    if mono.is_empty() {
        return Err(SoundboardError::Decode("file contained no audio".into()));
    }
    Ok(resample_linear(&mono, source_rate, TARGET_RATE))
}

/// Average all channels into mono, appending to `out`.
fn append_downmixed(decoded: &AudioBufferRef<'_>, out: &mut Vec<f32>) {
    macro_rules! downmix {
        ($buf:expr, $to_f32:expr) => {{
            let channels = $buf.spec().channels.count();
            let frames = $buf.frames();
            for frame in 0..frames {
                let mut acc = 0.0f32;
                for ch in 0..channels {
                    #[allow(clippy::redundant_closure_call)]
                    {
                        acc += $to_f32($buf.chan(ch)[frame]);
                    }
                }
                out.push(acc / channels as f32);
            }
        }};
    }
    match decoded {
        AudioBufferRef::F32(buf) => downmix!(buf, |s: f32| s),
        AudioBufferRef::F64(buf) => downmix!(buf, |s: f64| s as f32),
        AudioBufferRef::S32(buf) => downmix!(buf, |s: i32| s as f32 / i32::MAX as f32),
        AudioBufferRef::S16(buf) => downmix!(buf, |s: i16| s as f32 / i16::MAX as f32),
        AudioBufferRef::U8(buf) => downmix!(buf, |s: u8| (s as f32 - 128.0) / 128.0),
        AudioBufferRef::S24(buf) => {
            downmix!(buf, |s: symphonia::core::sample::i24| s.inner() as f32
                / 8_388_607.0)
        }
        AudioBufferRef::U16(buf) => downmix!(buf, |s: u16| (s as f32 - 32_768.0) / 32_768.0),
        AudioBufferRef::U24(buf) => {
            downmix!(buf, |s: symphonia::core::sample::u24| (s.inner() as f32
                - 8_388_608.0)
                / 8_388_608.0)
        }
        AudioBufferRef::U32(buf) => {
            downmix!(buf, |s: u32| (s as f64 / u32::MAX as f64 * 2.0 - 1.0)
                as f32)
        }
        AudioBufferRef::S8(buf) => downmix!(buf, |s: i8| s as f32 / i8::MAX as f32),
    }
}

/// Linear-interpolation resampler. Quality is fine for soundboard clips
/// (decode-time, not the live voice path).
pub fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        let a = input[idx];
        let b = *input.get(idx + 1).unwrap_or(&a);
        out.push(a + (b - a) * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_preserves_duration() {
        let input = vec![0.5f32; 44_100];
        let out = resample_linear(&input, 44_100, 48_000);
        assert!((out.len() as i64 - 48_000).abs() <= 2);
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resample_same_rate_is_identity() {
        let input: Vec<f32> = (0..100).map(|i| i as f32).collect();
        assert_eq!(resample_linear(&input, 48_000, 48_000), input);
    }
}
