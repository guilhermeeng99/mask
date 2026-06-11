//! Companion model download with checksum verification
//! (ai_voice_conversion rule 3: pinned URL + sha256, partial files discarded).

use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{CompanionManifest, VcError};

/// Built-in manifest, verified reachable on 2026-06-11.
///
/// - ContentVec 768: vec-768-layer-12.onnx from MoeSS-SUBModel (the original
///   community export RVC's own onnx_inference.py defaults to), ~360 MB.
/// - ContentVec 256: vec-256-layer-9.onnx for RVC v1 / 256-dim exports.
/// - RMVPE: from lj1995/VoiceConversionWebUI (the official RVC weights repo).
///
/// Checksums pinned from the files fetched on 2026-06-11 (trust on first
/// download, recorded permanently; any upstream change now fails loudly).
pub fn default_manifest() -> CompanionManifest {
    CompanionManifest {
        contentvec_url:
            "https://huggingface.co/NaruseMioShirakana/MoeSS-SUBModel/resolve/main/vec-768-layer-12.onnx"
                .into(),
        contentvec_sha256: "b3886e7dff1495cda514f94f4680a7b1261e05d6929f5c764cdb17934b413c2a"
            .into(),
        contentvec256_url: "https://huggingface.co/ozada/onnx_rvc/resolve/main/vec-256-layer-9.onnx"
            .into(),
        contentvec256_sha256: "61d0d0598803f74d5a4bcba65ca367571889accc0e86196f0982e8e455d56393"
            .into(),
        rmvpe_url: "https://huggingface.co/lj1995/VoiceConversionWebUI/resolve/main/rmvpe.onnx"
            .into(),
        rmvpe_sha256: "5370e71ac80af8b4b7c793d27efd51fd8bf962de3a7ede0766dac0befa3660fd".into(),
    }
}

/// Download one file to `dest`, streaming through a temp file so an
/// interrupted download never leaves a half-written companion behind.
pub fn download_verified(
    url: &str,
    expected_sha256: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), VcError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("part");

    let response = ureq::get(url)
        .call()
        .map_err(|e| VcError::Runtime(format!("download failed: {e}")))?;
    let total = response
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok());

    let mut reader = response.into_reader();
    let mut file = fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| VcError::Runtime(format!("download interrupted: {e}")))?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n])?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    drop(file);

    let digest = format!("{:x}", hasher.finalize());
    // An empty expected hash means "trust on first use" (pre-release builds);
    // a pinned hash is enforced strictly.
    if !expected_sha256.is_empty() && digest != expected_sha256.to_lowercase() {
        let _ = fs::remove_file(&tmp);
        return Err(VcError::Runtime(format!(
            "checksum mismatch for {url}: got {digest}"
        )));
    }
    fs::rename(&tmp, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_manifest_points_at_https() {
        let m = default_manifest();
        assert!(m.contentvec_url.starts_with("https://"));
        assert!(m.rmvpe_url.starts_with("https://"));
    }
}
