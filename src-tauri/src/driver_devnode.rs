//! Devnode inspection for the virtual audio driver (virtual_mic_setup.md
//! rules 9 and 10).
//!
//! Endpoint enumeration alone cannot distinguish "driver not installed" from
//! "driver installed but Windows refuses to start it" (e.g. Device Manager
//! code 52: signature rejected under Secure Boot / Memory Integrity — no
//! endpoint ever appears and rebooting does not help). This module asks
//! SetupAPI directly so detection and the install helper can tell the two
//! apart.

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Status, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW, CR_SUCCESS, DIGCF_ALLCLASSES,
    DIGCF_PRESENT, DN_HAS_PROBLEM, SPDRP_HARDWAREID, SP_DEVINFO_DATA,
};

/// Snapshot of the devnode matching a hardware id.
///
/// `present` — a devnode with this hardware id exists on the system.
/// `problem_code` — Device Manager problem code when the devnode failed to
/// start (52 = unsigned/rejected driver signature); `None` when healthy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevnodeState {
    pub present: bool,
    pub problem_code: Option<u32>,
}

/// Find the first present devnode whose hardware-id list contains
/// `hardware_id` (case-insensitive) and report its start state.
///
/// Example: `query_devnode("ROOT\\VirtualAudioDriver")` returns
/// `DevnodeState { present: true, problem_code: Some(52) }` when the one-click
/// driver installed but Windows rejected its signature.
pub fn query_devnode(hardware_id: &str) -> DevnodeState {
    const ABSENT: DevnodeState = DevnodeState {
        present: false,
        problem_code: None,
    };
    unsafe {
        let devinfo =
            SetupDiGetClassDevsW(std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), {
                DIGCF_ALLCLASSES | DIGCF_PRESENT
            });
        if devinfo == -1 {
            return ABSENT;
        }
        let state = scan_devinfo_list(devinfo, hardware_id);
        SetupDiDestroyDeviceInfoList(devinfo);
        state.unwrap_or(ABSENT)
    }
}

unsafe fn scan_devinfo_list(devinfo: isize, hardware_id: &str) -> Option<DevnodeState> {
    let wanted = hardware_id.to_lowercase();
    let mut index = 0u32;
    loop {
        let mut data: SP_DEVINFO_DATA = std::mem::zeroed();
        data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        if SetupDiEnumDeviceInfo(devinfo, index, &mut data) == 0 {
            return None;
        }
        index += 1;
        if hardware_ids_of(devinfo, &mut data)
            .iter()
            .any(|id| id == &wanted)
        {
            return Some(DevnodeState {
                present: true,
                problem_code: problem_code_of(data.DevInst),
            });
        }
    }
}

/// SPDRP_HARDWAREID is REG_MULTI_SZ: NUL-separated ids, double-NUL terminated.
unsafe fn hardware_ids_of(devinfo: isize, data: &mut SP_DEVINFO_DATA) -> Vec<String> {
    let mut buf = [0u16; 1024];
    let ok = SetupDiGetDeviceRegistryPropertyW(
        devinfo,
        data,
        SPDRP_HARDWAREID,
        std::ptr::null_mut(),
        buf.as_mut_ptr() as *mut u8,
        (buf.len() * 2) as u32,
        std::ptr::null_mut(),
    );
    if ok == 0 {
        return Vec::new();
    }
    buf.split(|&c| c == 0)
        .take_while(|chunk| !chunk.is_empty())
        .map(|chunk| String::from_utf16_lossy(chunk).to_lowercase())
        .collect()
}

unsafe fn problem_code_of(devinst: u32) -> Option<u32> {
    let mut status = 0u32;
    let mut problem = 0u32;
    if CM_Get_DevNode_Status(&mut status, &mut problem, devinst, 0) != CR_SUCCESS {
        return None;
    }
    (status & DN_HAS_PROBLEM != 0).then_some(problem)
}
