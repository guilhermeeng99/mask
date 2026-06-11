//! One-click virtual microphone install (virtual_mic_setup.md).
//!
//! Downloads the MIT-licensed, Microsoft-signed Virtual-Audio-Driver release
//! (sha256-pinned), extracts it, and relaunches mask.exe elevated in helper
//! mode (`--install-virtual-driver`) to register the devnode. Progress and
//! the outcome are reported through `vmic://*` events.

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter};

use crate::vc::download::download_verified;

/// VirtualDrivers/Virtual-Audio-Driver release 25.7.14 (MIT, signed).
/// sha256 pinned from the file fetched on 2026-06-11.
const DRIVER_ZIP_URL: &str = "https://github.com/VirtualDrivers/Virtual-Audio-Driver/releases/download/25.7.14/Virtual.Audio.Driver.Signed.-.25.7.14.zip";
const DRIVER_ZIP_SHA256: &str = "dd10560994de65a7e587fb8b93c0d7e9838292d9c3566a0976c2786d727292bd";

#[tauri::command]
pub fn virtual_mic_install(app: AppHandle) -> Result<(), String> {
    std::thread::Builder::new()
        .name("mask-vmic-install".into())
        .spawn(move || {
            let emit = |state: &str, detail: serde_json::Value| {
                let _ = app.emit(
                    "vmic://install",
                    serde_json::json!({ "state": state, "detail": detail }),
                );
            };
            emit("downloading", serde_json::Value::Null);
            match run_install() {
                Ok(reboot_required) => emit(
                    "done",
                    serde_json::json!({ "rebootRequired": reboot_required }),
                ),
                Err(message) => emit("error", serde_json::json!(message)),
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn run_install() -> Result<bool, String> {
    let work_dir = std::env::temp_dir().join("mask-virtual-driver");
    let _ = std::fs::remove_dir_all(&work_dir);
    std::fs::create_dir_all(&work_dir).map_err(|e| e.to_string())?;

    let zip_path = work_dir.join("driver.zip");
    download_verified(DRIVER_ZIP_URL, DRIVER_ZIP_SHA256, &zip_path, |_, _| {})
        .map_err(|e| e.to_string())?;
    extract_zip(&zip_path, &work_dir)?;

    let inf = find_inf(&work_dir)
        .ok_or_else(|| "driver package did not contain an .inf file".to_string())?;

    // UAC prompt: run ourselves elevated in helper mode and wait.
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    match run_elevated_and_wait(&exe, &inf)? {
        0 => Ok(false),
        2 => Ok(true),
        1223 => Err("installation was cancelled at the Windows permission prompt".into()),
        code => Err(format!("driver installer exited with code {code}")),
    }
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    archive.extract(dest).map_err(|e| e.to_string())
}

fn find_inf(dir: &Path) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("inf"))
            {
                return Some(path);
            }
        }
    }
    None
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(once(0)).collect()
}

/// ShellExecuteExW with the `runas` verb (triggers UAC), then wait for the
/// helper to finish and return its exit code. 1223 = user declined the prompt.
fn run_elevated_and_wait(exe: &Path, inf: &Path) -> Result<u32, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    let verb = wide(OsStr::new("runas"));
    let file = wide(exe.as_os_str());
    let params_str = format!("--install-virtual-driver \"{}\"", inf.display());
    let params = wide(OsStr::new(&params_str));

    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = 0; // SW_HIDE

        if ShellExecuteExW(&mut info) == 0 {
            // The common failure here is the user pressing "No" on UAC
            // (ERROR_CANCELLED = 1223 as last error).
            let last = std::io::Error::last_os_error();
            if last.raw_os_error() == Some(1223) {
                return Ok(1223);
            }
            return Err(format!("could not launch the elevated installer: {last}"));
        }
        if info.hProcess.is_null() {
            return Err("elevated installer produced no process handle".into());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code: u32 = 1;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}
