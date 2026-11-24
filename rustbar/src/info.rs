//! The date, the system numbers and the hardware names shown on the bar.

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::Instant;

use windows_sys::Wdk::System::SystemInformation::NtQuerySystemInformation;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, UNICODE_STRING};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_PRIMARY_DEVICE};
use windows_sys::Win32::Globalization::{GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS};
use windows_sys::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows_sys::Win32::NetworkManagement::WiFi::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR,
    STORAGE_PROPERTY_QUERY,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Performance::*;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_ANY, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GetSystemFirmwareTable, GetSystemInfo, GlobalMemoryStatusEx,
    RelationProcessorCore, MEMORYSTATUSEX, RSMB, SYSTEM_INFO, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::GetSystemTimes;

#[derive(Default, PartialEq)]
pub struct Info {
    /// Local short date and time, e.g. `Fri 6 Nov 21:43`.
    pub date: String,
    pub cpu: u32,
    pub gpu: Option<u32>,
    /// Dedicated GPU memory in use, in tenths of a GiB.
    pub vram: Option<u32>,
    /// Physical memory in use, in tenths of a GiB, and installed in whole GiB.
    pub mem: (u32, u32),
    /// Page file in use, in tenths of a GiB, and its size in whole GiB;
    /// `None` without a page file.
    pub swap: Option<(u32, u32)>,
    /// System drive used and size, in whole GiB.
    pub disk: Option<(u32, u32)>,
    /// Download and upload in bytes per second; `None` on the first sample.
    pub net: Option<(u64, u64)>,
    /// Charge percent and whether it is charging; `None` without a battery.
    pub battery: Option<(u32, bool)>,
    /// Connected network, `Some(None)` when disconnected; `None` without a
    /// Wi-Fi adapter.
    pub wifi: Option<Option<Wifi>>,
}

#[derive(PartialEq)]
pub struct Wifi {
    pub name: String,
    /// Wi-Fi generation, e.g. `6E`; `None` before Wi-Fi 4.
    pub standard: Option<&'static str>,
}

/// Keeps what the samples need between ticks: CPU time totals for the usage
/// percent and the WLAN and GPU counter handles.
#[derive(Default)]
pub struct Sampler {
    idle: u64,
    total: u64,
    /// WLAN client handle, opened on first use; 0 until then.
    wlan: isize,
    /// The last access point and whether it is on 6 GHz, which tells Wi-Fi
    /// 6E from 6.
    bss: ([u8; 6], bool),
    /// PDH query and counters for GPU usage and dedicated memory, opened on
    /// first use; 0 until then.
    gpu_query: isize,
    gpu_counter: isize,
    vram_counter: isize,
    /// Bytes received and sent so far, and when, for the network speed.
    net: Option<(u64, u64, Instant)>,
}

impl Sampler {
    pub fn sample(&mut self) -> Info {
        let (gpu, vram) = self.gpu();
        Info {
            date: date(),
            cpu: self.cpu(),
            gpu,
            vram,
            mem: mem(),
            swap: swap(),
            disk: disk(),
            net: self.net(),
            battery: battery(),
            wifi: self.wifi(),
        }
    }

    /// Traffic over the physical adapters since the last sample.
    fn net(&mut self) -> Option<(u64, u64)> {
        let mut table: *mut MIB_IF_TABLE2 = null_mut();
        if unsafe { GetIfTable2(&mut table) } != 0 {
            return None;
        }
        let rows = unsafe { std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize) };
        // Hardware interfaces with a connector, but not the filter drivers
        // layered on them, which repeat the same traffic. The flags are
        // HardwareInterface, FilterInterface and ConnectorPresent, lowest
        // bit first.
        let (mut down, mut up) = (0, 0);
        for row in rows.iter().filter(|r| r.InterfaceAndOperStatusFlags._bitfield & 0b111 == 0b101) {
            down += row.InOctets;
            up += row.OutOctets;
        }
        unsafe { FreeMibTable(table as *const c_void) };
        let now = Instant::now();
        let (last_down, last_up, last) = self.net.replace((down, up, now))?;
        let secs = now.duration_since(last).as_secs_f64();
        let rate = |now: u64, last: u64| (now.saturating_sub(last) as f64 / secs) as u64;
        (secs > 0.0).then(|| (rate(down, last_down), rate(up, last_up)))
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

    // The WLAN constants are lowercase.
    #[allow(non_upper_case_globals)]
    fn wifi(&mut self) -> Option<Option<Wifi>> {
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
                let assoc = &(*attrs).wlanAssociationAttributes;
                let standard = match assoc.dot11PhyType {
                    dot11_phy_type_ht => Some("4"),
                    dot11_phy_type_vht => Some("5"),
                    dot11_phy_type_he if self.on_6ghz(&i.InterfaceGuid, assoc) => Some("6E"),
                    dot11_phy_type_he => Some("6"),
                    dot11_phy_type_eht => Some("7"),
                    _ => None,
                };
                WlanFreeMemory(attrs as *const c_void);
                result = Some(Some(Wifi { name, standard }));
                break;
            }
            WlanFreeMemory(list as *const c_void);
            result
        }
    }

    /// Whether the connected access point is on 6 GHz, looked up once per
    /// access point.
    unsafe fn on_6ghz(&mut self, interface: &windows_sys::core::GUID, assoc: &WLAN_ASSOCIATION_ATTRIBUTES) -> bool {
        if self.bss.0 == assoc.dot11Bssid {
            return self.bss.1;
        }
        let mut list: *mut WLAN_BSS_LIST = null_mut();
        let status =
            WlanGetNetworkBssList(self.wlan as HANDLE, interface, &assoc.dot11Ssid, assoc.dot11BssType, 1, null(), &mut list);
        if status != 0 {
            return false;
        }
        let entries = std::slice::from_raw_parts((*list).wlanBssEntries.as_ptr(), (*list).dwNumberOfItems as usize);
        // The center frequency is in kHz.
        let on_6ghz = entries.iter().any(|e| e.dot11Bssid == assoc.dot11Bssid && e.ulChCenterFrequency >= 5_925_000);
        WlanFreeMemory(list as *const c_void);
        self.bss = (assoc.dot11Bssid, on_6ghz);
        on_6ghz
    }

    /// GPU usage like Task Manager's: per adapter and engine type (3D, Copy,
    /// VideoDecode, ...) the utilization of all processes is summed, and the
    /// busiest one wins. Also the dedicated memory in use on the adapter
    /// using the most. `None` if the counters are unavailable.
    fn gpu(&mut self) -> (Option<u32>, Option<u32>) {
        unsafe {
            if self.gpu_query == 0 {
                if PdhOpenQueryW(null(), 0, &mut self.gpu_query) != 0 {
                    return (None, None);
                }
                let path = wide(r"\GPU Engine(*)\Utilization Percentage");
                PdhAddEnglishCounterW(self.gpu_query, path.as_ptr(), 0, &mut self.gpu_counter);
                let path = wide(r"\GPU Adapter Memory(*)\Dedicated Usage");
                PdhAddEnglishCounterW(self.gpu_query, path.as_ptr(), 0, &mut self.vram_counter);
            }
            PdhCollectQueryData(self.gpu_query);
        }
        let usage = counter_array(self.gpu_counter).map(|items| {
            let mut totals: HashMap<String, f64> = HashMap::new();
            for (name, value) in items {
                // pid_1234_luid_0x00000000_0x0000B6E5_phys_0_eng_3_engtype_3D
                let parts: Vec<&str> = name.split('_').collect();
                if parts.len() < 11 {
                    continue;
                }
                let key = format!("{}_{}_{}", parts[3], parts[4], parts[10..].join("_"));
                *totals.entry(key).or_default() += value;
            }
            totals.values().fold(0.0f64, |a, &b| a.max(b)).min(100.0).round() as u32
        });
        let vram = counter_array(self.vram_counter)
            .map(|items| (items.iter().fold(0.0f64, |a, (_, b)| a.max(*b)) / GIB * 10.0).round() as u32);
        (usage, vram)
    }
}

const GIB: f64 = (1u64 << 30) as f64;

/// Instance names and values of a wildcard PDH counter.
fn counter_array(counter: isize) -> Option<Vec<(String, f64)>> {
    unsafe {
        let (mut size, mut count) = (0u32, 0u32);
        if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, null_mut()) != PDH_MORE_DATA {
            return None;
        }
        // A u64 buffer keeps the items aligned; their names follow them.
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 {
            return None;
        }
        let mut out = Vec::new();
        for item in std::slice::from_raw_parts(items, count as usize) {
            if item.FmtValue.CStatus > PDH_CSTATUS_NEW_DATA {
                continue;
            }
            let len = (0..).take_while(|&i| *item.szName.add(i) != 0).count();
            let name = String::from_utf16_lossy(std::slice::from_raw_parts(item.szName, len));
            out.push((name, item.FmtValue.Anonymous.doubleValue));
        }
        Some(out)
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

fn mem() -> (u32, u32) {
    let mut status: MEMORYSTATUSEX = unsafe { zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    unsafe { GlobalMemoryStatusEx(&mut status) };
    let used = (status.ullTotalPhys - status.ullAvailPhys) as f64 / GIB;
    let total = status.ullTotalPhys as f64 / GIB;
    ((used * 10.0).round() as u32, total.round() as u32)
}

/// `SYSTEM_PAGEFILE_INFORMATION`, one per page file; sizes are in pages.
#[repr(C)]
struct PageFile {
    next: u32,
    size: u32,
    in_use: u32,
    peak: u32,
    name: UNICODE_STRING,
}

fn swap() -> Option<(u32, u32)> {
    const SYSTEM_PAGEFILE_INFORMATION: i32 = 18;
    // Room for a few page files and their names.
    let mut buf = vec![0u64; 256];
    let status = unsafe {
        NtQuerySystemInformation(SYSTEM_PAGEFILE_INFORMATION, buf.as_mut_ptr().cast(), 8 * buf.len() as u32, null_mut())
    };
    if status != 0 {
        return None;
    }
    let (mut pages, mut used, mut offset) = (0u64, 0u64, 0);
    loop {
        let file = unsafe { &*(buf.as_ptr() as *const u8).add(offset).cast::<PageFile>() };
        pages += file.size as u64;
        used += file.in_use as u64;
        if file.next == 0 {
            break;
        }
        offset += file.next as usize;
    }
    if pages == 0 {
        return None;
    }
    let mut si: SYSTEM_INFO = unsafe { zeroed() };
    unsafe { GetSystemInfo(&mut si) };
    let gib = |pages: u64| (pages * si.dwPageSize as u64) as f64 / GIB;
    Some(((gib(used) * 10.0).round() as u32, gib(pages).round() as u32))
}

fn system_drive() -> String {
    std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
}

fn disk() -> Option<(u32, u32)> {
    let (mut total, mut free) = (0u64, 0u64);
    let root = wide(&format!("{}\\", system_drive()));
    if unsafe { GetDiskFreeSpaceExW(root.as_ptr(), null_mut(), &mut total, &mut free) } == 0 {
        return None;
    }
    let gib = |bytes: u64| (bytes as f64 / GIB).round() as u32;
    Some((gib(total - free), gib(total)))
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

/// The labels of the CPU, memory, GPU and system drive numbers, e.g.
/// `Ryzen 5 3600 6C/12T`, `DDR4-3200`, `RTX 4060 3072c GDDR6` and
/// `NVMe 3.3GB/s`, and the GPU's dedicated memory in whole GiB.
pub struct Names {
    pub cpu: String,
    pub gpu: String,
    pub mem: String,
    pub disk: String,
    pub vram: Option<u32>,
}

/// Reads the names once; they do not change while running.
pub fn names() -> Names {
    let cpu = registry_string(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString");
    let (gpu, key) = gpu_name().unzip();
    // Older drivers only store the 32-bit MemorySize.
    let vram = key.and_then(|key| {
        registry_u64(&key, "HardwareInformation.qwMemorySize")
            .or_else(|| registry_u64(&key, "HardwareInformation.MemorySize"))
    });
    let mut cpu = short_name(cpu.as_deref().unwrap_or("cpu"));
    if let Some((cores, threads)) = cores() {
        cpu += &format!(" {cores}C/{threads}T");
    }
    let mut gpu = short_name(&gpu.unwrap_or("gpu".into()));
    match GPUS.iter().find(|g| g.0 == gpu) {
        Some((_, cores, memory)) => gpu += &format!(" {cores}c {memory}"),
        None => gpu += " unknown model",
    }
    let mem = match memory() {
        Some((Some(kind), Some(speed))) => format!("{kind}-{speed}"),
        Some((Some(kind), None)) => kind.into(),
        _ => "mem".into(),
    };
    // The model is only used to look up the read speed.
    let (bus, model) = disk_name().unwrap_or(("disk", String::new()));
    let mut disk = bus.to_string();
    match DISKS.iter().find(|d| d.0 == model) {
        Some((_, read)) => disk += &format!(" {:.1}GB/s", *read as f64 / 1000.0),
        None => disk += " unknown model",
    }
    Names {
        cpu,
        gpu,
        mem,
        disk,
        vram: vram.map(|bytes| (bytes as f64 / GIB).round() as u32),
    }
}

/// Shader cores and memory type by short GPU name, which Windows does not
/// report. Add a row for another card.
const GPUS: [(&str, u32, &str); 1] = [("RTX 4060", 3072, "GDDR6")];

/// Rated sequential read speed in MB/s by drive name, which Windows does not
/// report. Add a row for another drive.
const DISKS: [(&str, u32); 1] = [("Lexar NM620", 3300)];

/// Physical cores and hardware threads.
fn cores() -> Option<(u32, u32)> {
    let mut size = 0;
    unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, null_mut(), &mut size) };
    // A u64 buffer keeps the entries aligned.
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    let ptr = buf.as_mut_ptr() as *mut u8;
    if size == 0 || unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, ptr.cast(), &mut size) } == 0 {
        return None;
    }
    let (mut cores, mut threads, mut offset) = (0, 0, 0);
    // One variable-sized entry per core.
    while offset < size as usize {
        let entry = unsafe { &*(ptr.add(offset) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX) };
        let processor = unsafe { &entry.Anonymous.Processor };
        let groups = unsafe { std::slice::from_raw_parts(processor.GroupMask.as_ptr(), processor.GroupCount as usize) };
        cores += 1;
        threads += groups.iter().map(|g| g.Mask.count_ones()).sum::<u32>();
        offset += entry.Size as usize;
    }
    Some((cores, threads))
}

/// The bus and model of the system drive, e.g. `NVMe` and `Lexar NM620` from
/// `Lexar SSD NM620 1TB`.
// The bus type constants are not uppercase.
#[allow(non_upper_case_globals)]
fn disk_name() -> Option<(&'static str, String)> {
    let path = wide(&format!(r"\\.\{}", system_drive()));
    // No access rights are needed to query the device.
    let handle = unsafe {
        CreateFileW(path.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, 0, null_mut())
    };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    // A u64 buffer keeps the descriptor aligned; its strings follow it.
    let mut buf = vec![0u64; 128];
    let mut size = 0;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *const c_void,
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            buf.as_mut_ptr().cast(),
            (buf.len() * 8) as u32,
            &mut size,
            null_mut(),
        )
    };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    let desc = unsafe { &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, size as usize) };
    let string = |offset: u32| {
        let s = bytes.get(offset as usize..).filter(|_| offset > 0).unwrap_or_default();
        let len = s.iter().position(|&b| b == 0).unwrap_or(s.len());
        String::from_utf8_lossy(&s[..len]).into_owned()
    };
    let bus = match desc.BusType {
        BusTypeNvme => "NVMe",
        BusTypeSata => "SATA",
        BusTypeUsb => "USB",
        BusTypeSd => "SD",
        BusTypeMmc => "eMMC",
        BusTypeUfs => "UFS",
        _ => "disk",
    };
    // Drop "SSD", the capacity (shown with the usage) and a vendor or bus
    // that is repeated.
    let capacity = |w: &str| (w.ends_with("TB") || w.ends_with("GB")) && w.starts_with(|c: char| c.is_ascii_digit());
    let model = string(desc.VendorIdOffset) + " " + &string(desc.ProductIdOffset);
    let mut words: Vec<&str> = vec![bus];
    for w in model.split_whitespace() {
        if w != "SSD" && !capacity(w) && !words.iter().any(|seen| seen.eq_ignore_ascii_case(w)) {
            words.push(w);
        }
    }
    Some((bus, words[1..].join(" ")))
}

/// The formatted part of each SMBIOS structure.
fn smbios() -> Vec<Vec<u8>> {
    let size = unsafe { GetSystemFirmwareTable(RSMB, 0, null_mut(), 0) };
    let mut buf = vec![0u8; size as usize];
    if size == 0 || unsafe { GetSystemFirmwareTable(RSMB, 0, buf.as_mut_ptr(), size) } != size {
        return Vec::new();
    }
    // An 8-byte RawSMBIOSData header precedes the table.
    let table = buf.get(8..).unwrap_or_default();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= table.len() {
        let (kind, len) = (table[i], table[i + 1] as usize);
        let Some(s) = table.get(i..i + len) else { break };
        out.push(s.to_vec());
        // Strings follow the formatted part and end with a double NUL.
        i += len;
        while i + 1 < table.len() && (table[i] != 0 || table[i + 1] != 0) {
            i += 1;
        }
        i += 2;
        if kind == 127 {
            break;
        }
    }
    out
}

/// The type and speed in MT/s of the first installed memory module, from the
/// SMBIOS Memory Device (type 17) structures.
fn memory() -> Option<(Option<&'static str>, Option<u16>)> {
    for s in smbios() {
        // Size 0 is an empty slot.
        if s[0] == 17 && s.len() > 0x12 && u16::from_le_bytes([s[0x0C], s[0x0D]]) != 0 {
            let word = |at: usize| s.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
            // The configured speed (which includes XMP) or else the rated
            // speed; 0 is unknown and 0xFFFF means a speed above 65 GT/s.
            let speed = [word(0x20), word(0x15)].into_iter().flatten().find(|&v| v != 0 && v != 0xFFFF);
            let kind = match s[0x12] {
                0x12 => Some("DDR"),
                0x13 => Some("DDR2"),
                0x18 => Some("DDR3"),
                0x1A => Some("DDR4"),
                0x1B => Some("LPDDR"),
                0x1C => Some("LPDDR2"),
                0x1D => Some("LPDDR3"),
                0x1E => Some("LPDDR4"),
                0x22 => Some("DDR5"),
                0x23 => Some("LPDDR5"),
                _ => None,
            };
            return Some((kind, speed));
        }
    }
    None
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A QWORD, DWORD or 8-byte binary value; `None` if missing or zero.
fn registry_u64(key: &str, value: &str) -> Option<u64> {
    let mut out = 0u64;
    let mut size = size_of::<u64>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            wide(key).as_ptr(),
            wide(value).as_ptr(),
            RRF_RT_ANY,
            null_mut(),
            &mut out as *mut u64 as *mut c_void,
            &mut size,
        )
    };
    (status == 0 && out > 0).then_some(out)
}

fn registry_string(key: &str, value: &str) -> Option<String> {
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

/// The adapter driving the primary monitor, and its registry key under HKLM.
fn gpu_name() -> Option<(String, String)> {
    let mut first = None;
    for i in 0.. {
        let mut dd: DISPLAY_DEVICEW = unsafe { zeroed() };
        dd.cb = size_of::<DISPLAY_DEVICEW>() as u32;
        if unsafe { EnumDisplayDevicesW(null(), i, &mut dd, 0) } == 0 {
            break;
        }
        // \Registry\Machine\System\CurrentControlSet\Control\Video\{...}\0000
        let key = from_wide(&dd.DeviceKey);
        let prefix = r"\Registry\Machine\";
        let key = match key.get(..prefix.len()) {
            Some(p) if p.eq_ignore_ascii_case(prefix) => key[prefix.len()..].to_string(),
            _ => key,
        };
        let name = (from_wide(&dd.DeviceString), key);
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
