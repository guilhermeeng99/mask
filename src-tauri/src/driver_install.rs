//! Elevated virtual-audio-driver installation (devcon-style).
//!
//! Runs in a separate elevated instance of mask.exe
//! (`mask.exe --install-virtual-driver <path-to-inf>`): creates the
//! root-enumerated devnode for `ROOT\VirtualAudioDriver` via SetupAPI, then
//! installs the signed INF with `UpdateDriverForPlugAndPlayDevicesW`.
//!
//! Exit codes: 0 = installed, 2 = installed but reboot required, 1 = failed.

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;

use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList, SetupDiCreateDeviceInfoW,
    SetupDiDestroyDeviceInfoList, SetupDiGetINFClassW, SetupDiSetDeviceRegistryPropertyW,
    UpdateDriverForPlugAndPlayDevicesW, DICD_GENERATE_ID, DIF_REGISTERDEVICE, DIF_REMOVE,
    INSTALLFLAG_FORCE, SPDRP_HARDWAREID, SP_DEVINFO_DATA,
};
pub const HARDWARE_ID: &str = "ROOT\\VirtualAudioDriver";

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

/// REG_MULTI_SZ: the id, a terminator, and the list terminator.
fn wide_multi(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = OsStr::new(s).encode_wide().collect();
    v.push(0);
    v.push(0);
    v
}

pub fn install(inf_path: &str) -> i32 {
    // A retry after a failed/blocked attempt must reuse the existing devnode;
    // registering again would pile up ROOT\MEDIA duplicates
    // (virtual_mic_setup.md rule 10).
    if crate::driver_devnode::query_devnode(HARDWARE_ID).present {
        return match update_driver(inf_path) {
            Ok(true) => 2,
            Ok(false) => 0,
            Err(()) => 1,
        };
    }
    register_devnode_and_install(inf_path)
}

/// Install/refresh the driver on every devnode matching `HARDWARE_ID`.
/// Returns whether Windows asked for a reboot.
fn update_driver(inf_path: &str) -> Result<bool, ()> {
    let inf_w = wide(inf_path);
    let hwid_w = wide(HARDWARE_ID);
    unsafe {
        let mut reboot_required = 0i32;
        let installed = UpdateDriverForPlugAndPlayDevicesW(
            std::ptr::null_mut(),
            hwid_w.as_ptr(),
            inf_w.as_ptr(),
            INSTALLFLAG_FORCE,
            &mut reboot_required,
        );
        if installed == 0 {
            eprintln!("UpdateDriverForPlugAndPlayDevicesW failed");
            return Err(());
        }
        Ok(reboot_required != 0)
    }
}

fn register_devnode_and_install(inf_path: &str) -> i32 {
    let inf_w = wide(inf_path);
    let hwid_multi = wide_multi(HARDWARE_ID);

    unsafe {
        // Class GUID + name come from the INF itself.
        let mut class_guid: GUID = std::mem::zeroed();
        let mut class_name = [0u16; 64];
        if SetupDiGetINFClassW(
            inf_w.as_ptr(),
            &mut class_guid,
            class_name.as_mut_ptr(),
            class_name.len() as u32,
            std::ptr::null_mut(),
        ) == 0
        {
            eprintln!("SetupDiGetINFClassW failed");
            return 1;
        }

        let devinfo = SetupDiCreateDeviceInfoList(&class_guid, std::ptr::null_mut());
        // HDEVINFO is an isize handle; -1 is INVALID_HANDLE_VALUE.
        if devinfo == -1 {
            eprintln!("SetupDiCreateDeviceInfoList failed");
            return 1;
        }

        let mut devinfo_data: SP_DEVINFO_DATA = std::mem::zeroed();
        devinfo_data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        if SetupDiCreateDeviceInfoW(
            devinfo,
            class_name.as_ptr(),
            &class_guid,
            std::ptr::null(),
            std::ptr::null_mut(),
            DICD_GENERATE_ID,
            &mut devinfo_data,
        ) == 0
        {
            eprintln!("SetupDiCreateDeviceInfoW failed");
            SetupDiDestroyDeviceInfoList(devinfo);
            return 1;
        }

        if SetupDiSetDeviceRegistryPropertyW(
            devinfo,
            &mut devinfo_data,
            SPDRP_HARDWAREID,
            hwid_multi.as_ptr() as *const u8,
            (hwid_multi.len() * 2) as u32,
        ) == 0
        {
            eprintln!("SetupDiSetDeviceRegistryPropertyW failed");
            SetupDiDestroyDeviceInfoList(devinfo);
            return 1;
        }

        if SetupDiCallClassInstaller(DIF_REGISTERDEVICE, devinfo, &devinfo_data) == 0 {
            eprintln!("SetupDiCallClassInstaller(DIF_REGISTERDEVICE) failed");
            SetupDiDestroyDeviceInfoList(devinfo);
            return 1;
        }

        match update_driver(inf_path) {
            Ok(reboot_required) => {
                SetupDiDestroyDeviceInfoList(devinfo);
                if reboot_required {
                    2
                } else {
                    0
                }
            }
            Err(()) => {
                // Roll back the devnode we just registered so a retry starts clean.
                let _ = SetupDiCallClassInstaller(DIF_REMOVE, devinfo, &devinfo_data);
                SetupDiDestroyDeviceInfoList(devinfo);
                1
            }
        }
    }
}
