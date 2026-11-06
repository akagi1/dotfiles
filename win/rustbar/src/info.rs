//! The date and the system numbers shown on the bar.

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{FILETIME, HANDLE};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_PRIMARY_DEVICE};
use windows_sys::Win32::Globalization::{GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS};
use windows_sys::Win32::NetworkManagement::WiFi::*;
use windows_sys::Win32::System::Performance::*;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::GetSystemTimes;

#[derive(Default, PartialEq)]
pub struct Info {
    /// Local short date and time, e.g. `Fri 6 Nov 21:43`.
    pub date: String,
    pub cpu: u32,
    pub gpu: Option<u32>,
    pub mem: u32,
    /// Charge percent and whether it is charging; `None` without a battery.
    pub battery: Option<(u32, bool)>,
    /// Connected network name, `Some(None)` when disconnected; `None`
    /// without a Wi-Fi adapter.
    pub wifi: Option<Option<String>>,
}

/// Keeps what the samples need between ticks: CPU time totals for the usage
/// percent and the WLAN and GPU counter handles.
#[derive(Default)]
pub struct Sampler {
    idle: u64,
    total: u64,
    /// WLAN client handle, opened on first use; 0 until then.
    wlan: isize,
    /// PDH query and counter for GPU usage, opened on first use; 0 until then.
    gpu_query: isize,
    gpu_counter: isize,
}

impl Sampler {
    pub fn sample(&mut self) -> Info {
        Info { date: date(), cpu: self.cpu(), gpu: self.gpu(), mem: mem(), battery: battery(), wifi: self.wifi() }
    }

    fn cpu(&mut self) -> u32 {
        let ft = |f: FILETIME| (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64;
        let (mut idle, mut kernel, mut user) = unsafe { (zeroed(), zeroed(), zeroed()) };
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return 0;
        }
        // Kernel time includes idle time.
        let (idle, total) = (ft(idle), ft(kernel) + ft(user));
        let (d_idle, d_total) = (idle - self.idle, total - self.total);
        self.idle = idle;
        self.total = total;
        if d_total == 0 {
            return 0;
        }
        (100 * (d_total - d_idle.min(d_total)) / d_total) as u32
    }

    fn wifi(&mut self) -> Option<Option<String>> {
        unsafe {
            if self.wlan == 0 {
                let (mut version, mut handle) = (0, null_mut());
                if WlanOpenHandle(2, null(), &mut version, &mut handle) != 0 {
                    return None;
                }
                self.wlan = handle as isize;
            }
            let wlan = self.wlan as HANDLE;
            let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
            if WlanEnumInterfaces(wlan, null(), &mut list) != 0 {
                return None;
            }
            let count = (*list).dwNumberOfItems as usize;
            let interfaces = std::slice::from_raw_parts((*list).InterfaceInfo.as_ptr(), count);
            let mut result = if count == 0 { None } else { Some(None) };
            for i in interfaces.iter().filter(|i| i.isState == wlan_interface_state_connected) {
                let (mut size, mut attrs): (u32, *mut WLAN_CONNECTION_ATTRIBUTES) = (0, null_mut());
                let query = WlanQueryInterface(
                    wlan,
                    &i.InterfaceGuid,
                    wlan_intf_opcode_current_connection,
                    null(),
                    &mut size,
                    &mut attrs as *mut _ as *mut *mut c_void,
                    null_mut(),
                );
                if query != 0 {
                    continue;
                }
                let ssid = &(*attrs).wlanAssociationAttributes.dot11Ssid;
                let mut name = String::from_utf8_lossy(&ssid.ucSSID[..(ssid.uSSIDLength as usize).min(32)]).into_owned();
                // Without location access Windows hides the SSID; the
                // profile name is normally the same.
                if name.is_empty() {
                    name = from_wide(&(*attrs).strProfileName);
                }
                WlanFreeMemory(attrs as *const c_void);
                result = Some(Some(name));
                break;
            }
            WlanFreeMemory(list as *const c_void);
            result
        }
    }

    /// GPU usage like Task Manager's: per adapter and engine type (3D, Copy,
    /// VideoDecode, ...) the utilization of all processes is summed, and the
    /// busiest one wins. `None` if the counters are unavailable.
    fn gpu(&mut self) -> Option<u32> {
        unsafe {
            if self.gpu_query == 0 {
                let path: Vec<u16> = r"\GPU Engine(*)\Utilization Percentage".encode_utf16().chain(Some(0)).collect();
                if PdhOpenQueryW(null(), 0, &mut self.gpu_query) != 0 {
                    return None;
                }
                PdhAddEnglishCounterW(self.gpu_query, path.as_ptr(), 0, &mut self.gpu_counter);
            }
            PdhCollectQueryData(self.gpu_query);
            let (mut size, mut count) = (0u32, 0u32);
            if PdhGetFormattedCounterArrayW(self.gpu_counter, PDH_FMT_DOUBLE, &mut size, &mut count, null_mut())
                != PDH_MORE_DATA
            {
                return None;
            }
            // A u64 buffer keeps the items aligned; their names follow them.
            let mut buf = vec![0u64; (size as usize).div_ceil(8)];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(self.gpu_counter, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 {
                return None;
            }
            let mut totals: HashMap<String, f64> = HashMap::new();
            for item in std::slice::from_raw_parts(items, count as usize) {
                if item.FmtValue.CStatus > PDH_CSTATUS_NEW_DATA {
                    continue;
                }
                let len = (0..).take_while(|&i| *item.szName.add(i) != 0).count();
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(item.szName, len));
                // pid_1234_luid_0x00000000_0x0000B6E5_phys_0_eng_3_engtype_3D
                let parts: Vec<&str> = name.split('_').collect();
                if parts.len() < 11 {
                    continue;
                }
                let key = format!("{}_{}_{}", parts[3], parts[4], parts[10..].join("_"));
                *totals.entry(key).or_default() += item.FmtValue.Anonymous.doubleValue;
            }
            Some(totals.values().fold(0.0f64, |a, &b| a.max(b)).min(100.0).round() as u32)
        }
    }
}

fn date() -> String {
    let mut buf = [0u16; 64];
    let picture: Vec<u16> = "ddd d MMM".encode_utf16().chain(Some(0)).collect();
    let mut out = String::new();
    unsafe {
        let n = GetDateFormatEx(null(), 0, null(), picture.as_ptr(), buf.as_mut_ptr(), 64, null());
        out += &String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize]);
        let n = GetTimeFormatEx(null(), TIME_NOSECONDS, null(), null(), buf.as_mut_ptr(), 64);
        out.push(' ');
        out += &String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize]);
    }
    out
}

fn mem() -> u32 {
    let mut status: MEMORYSTATUSEX = unsafe { zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    unsafe { GlobalMemoryStatusEx(&mut status) };
    status.dwMemoryLoad
}

fn battery() -> Option<(u32, bool)> {
    const NO_BATTERY: u8 = 128;
    const CHARGING: u8 = 8;
    let mut ps: SYSTEM_POWER_STATUS = unsafe { zeroed() };
    if unsafe { GetSystemPowerStatus(&mut ps) } == 0 || ps.BatteryFlag & NO_BATTERY != 0 || ps.BatteryLifePercent > 100 {
        return None;
    }
    Some((ps.BatteryLifePercent as u32, ps.BatteryFlag & CHARGING != 0))
}

/// Short CPU and GPU names shown as the labels of their usage numbers.
pub struct Names {
    pub cpu: String,
    pub gpu: String,
}

/// Reads the names once; they do not change while running.
pub fn names() -> Names {
    let cpu = registry_string(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString");
    Names { cpu: short_name(cpu.as_deref().unwrap_or("cpu")), gpu: short_name(&gpu_name().unwrap_or("gpu".into())) }
}

fn registry_string(key: &str, value: &str) -> Option<String> {
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let mut buf = [0u16; 256];
    let mut size = size_of::<[u16; 256]>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            wide(key).as_ptr(),
            wide(value).as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    (status == 0).then(|| from_wide(&buf))
}

/// The adapter driving the primary monitor.
fn gpu_name() -> Option<String> {
    let mut first = None;
    for i in 0.. {
        let mut dd: DISPLAY_DEVICEW = unsafe { zeroed() };
        dd.cb = size_of::<DISPLAY_DEVICEW>() as u32;
        if unsafe { EnumDisplayDevicesW(null(), i, &mut dd, 0) } == 0 {
            break;
        }
        let name = from_wide(&dd.DeviceString);
        if dd.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0 {
            return Some(name);
        }
        first.get_or_insert(name);
    }
    first
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Drops vendor and marketing words: `AMD Ryzen 5 3600 6-Core Processor`
/// becomes `Ryzen 5 3600`, `NVIDIA GeForce RTX 4060` becomes `RTX 4060`.
fn short_name(name: &str) -> String {
    let mut s = name.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    for cut in [" with ", " CPU", " @", " Processor"] {
        if let Some(i) = s.find(cut) {
            s.truncate(i);
        }
    }
    const DROP: [&str; 7] = ["AMD", "Intel", "NVIDIA", "GeForce", "Radeon", "Core", "Gen"];
    // "13th" as in "13th Gen Intel Core i7-13700K".
    let ordinal = |w: &str| w.len() > 2 && w.ends_with("th") && w[..w.len() - 2].bytes().all(|b| b.is_ascii_digit());
    let words: Vec<&str> = s
        .split_whitespace()
        .filter(|w| !DROP.contains(w) && !w.ends_with("-Core") && !ordinal(w))
        .collect();
    // Integrated GPUs are often just "AMD Radeon Graphics".
    match words.as_slice() {
        [] | ["Graphics"] => s.split_whitespace().collect::<Vec<_>>().join(" "),
        _ => words.join(" "),
    }
}
