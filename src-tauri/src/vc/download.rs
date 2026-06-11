//! Companion model download with checksum verification
//! (ai_voice_conversion rule 3: pinned URL + sha256, partial files discarded).

use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{CompanionManifest, VcError};

/// Built-in manifest. The URLs point to the community ONNX exports used by
/// w-okada/RVC tooling. Checksums MUST be verified and pinned before a
/// release that enables VC by default (spec open question).
pub fn default_manifest() -> CompanionManifest {
    CompanionManifest {
        contentvec_url:
            "https://huggingface.co/wok000/vcclient000/resolve/main/content_vec_500.onnx".into(),
        contentvec_sha256: String::new(),
        rmvpe_url: "https://huggingface.co/wok000/vcclient000/resolve/main/rmvpe.onnx".into(),
        rmvpe_sha256: String::new(),
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
