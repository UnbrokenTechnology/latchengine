// memory.rs
//! Cross-platform helpers to query cache line size, L1/L2/L3 sizes, and total RAM.
//! Falls back to conservative defaults when unavailable.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub struct Memory {
    pub cache_line: usize, // bytes
    pub l1: usize,         // bytes
    pub l2: usize,         // bytes
    pub l3: usize,         // bytes
    pub total_ram: u64,    // bytes
}

impl Memory {
    pub fn detect() -> Self {
        static INSTANCE: OnceLock<Memory> = OnceLock::new();
        *INSTANCE.get_or_init(|| Self::detect_impl())
    }

    fn detect_impl() -> Self {
        Self {
            cache_line: cache_line_size().unwrap_or(64),
            l1: l1_size().unwrap_or(32 * 1024),
            l2: l2_size().unwrap_or(256 * 1024),
            l3: l3_size().unwrap_or(4 * 1024 * 1024),
            total_ram: total_ram_bytes().unwrap_or(1 * 1024 * 1024 * 1024),
        }
    }
}

/* -------------------------- Windows -------------------------- */

#[cfg(target_os = "windows")]
fn cache_line_size() -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, SYSTEM_LOGICAL_PROCESSOR_INFORMATION, RelationCache,
        CACHE_DESCRIPTOR, PROCESSOR_CACHE_TYPE, CacheData,
    };
    unsafe {
        let mut len = 0u32;
        GetLogicalProcessorInformation(std::ptr::null_mut(), &mut len);
        if len == 0 { return None; }
        let count = len as usize / std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
        let mut buf = vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); count];
        if GetLogicalProcessorInformation(buf.as_mut_ptr(), &mut len) == 0 { return None; }
        for rec in &buf {
            if rec.Relationship == RelationCache {
                let c: CACHE_DESCRIPTOR = unsafe { rec.Anonymous.Cache };
                if c.Level == 1 && c.Type as u32 == CacheData { return Some(c.LineSize as usize); }
            }
        }
        // fallback: any data cache line size
        for rec in &buf {
            if rec.Relationship == RelationCache {
                let c: CACHE_DESCRIPTOR = unsafe { rec.Anonymous.Cache };
                if c.Type as u32 == CacheData { return Some(c.LineSize as usize); }
            }
        }
        None
    }
}
#[cfg(target_os = "windows")]
fn cache_size_by_level(level: u8) -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, SYSTEM_LOGICAL_PROCESSOR_INFORMATION, RelationCache,
        CACHE_DESCRIPTOR, PROCESSOR_CACHE_TYPE, CacheData,
    };
    unsafe {
        let mut len = 0u32;
        GetLogicalProcessorInformation(std::ptr::null_mut(), &mut len);
        if len == 0 { return None; }
        let count = len as usize / std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
        let mut buf = vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); count];
        if GetLogicalProcessorInformation(buf.as_mut_ptr(), &mut len) == 0 { return None; }
        for rec in &buf {
            if rec.Relationship == RelationCache {
                let c: CACHE_DESCRIPTOR = unsafe { rec.Anonymous.Cache };
                if c.Type as u32 == CacheData && c.Level == level { return Some(c.Size as usize); }
            }
        }
        None
    }
}
#[cfg(target_os = "windows")]
fn l1_size() -> Option<usize> { cache_size_by_level(1) }
#[cfg(target_os = "windows")]
fn l2_size() -> Option<usize> { cache_size_by_level(2) }
#[cfg(target_os = "windows")]
fn l3_size() -> Option<usize> { cache_size_by_level(3) }

#[cfg(target_os = "windows")]
fn total_ram_bytes() -> Option<u64> {
    use windows_sys::Win32::System::Memory::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    unsafe {
        let mut st = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        if GlobalMemoryStatusEx(&mut st) != 0 { Some(st.ullTotalPhys as u64) } else { None }
    }
}

/* --------------------- macOS / iOS (Darwin) --------------------- */

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn sysctl_usize(name: &str) -> Option<usize> {
    use libc::{c_void, size_t, sysctlbyname};
    let cname = std::ffi::CString::new(name).ok()?;
    let mut val: usize = 0;
    let mut len: size_t = std::mem::size_of::<usize>() as _;
    let rc = unsafe { sysctlbyname(cname.as_ptr(), &mut val as *mut _ as *mut c_void, &mut len, std::ptr::null_mut(), 0) };
    if rc == 0 && val != 0 { Some(val) } else { None }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn sysctl_u64(name: &str) -> Option<u64> {
    use libc::{c_void, size_t, sysctlbyname};
    let cname = std::ffi::CString::new(name).ok()?;
    let mut val: u64 = 0;
    let mut len: size_t = std::mem::size_of::<u64>() as _;
    let rc = unsafe { sysctlbyname(cname.as_ptr(), &mut val as *mut _ as *mut c_void, &mut len, std::ptr::null_mut(), 0) };
    if rc == 0 && val != 0 { Some(val) } else { None }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn cache_line_size() -> Option<usize> { sysctl_usize("hw.cachelinesize") }
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn l1_size() -> Option<usize> { sysctl_usize("hw.l1dcachesize") }
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn l2_size() -> Option<usize> { sysctl_usize("hw.l2cachesize") }
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn l3_size() -> Option<usize> { sysctl_usize("hw.l3cachesize") }
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn total_ram_bytes() -> Option<u64> { sysctl_u64("hw.memsize") }

/* --------------------- Linux / Android --------------------- */

#[cfg(any(target_os = "linux", target_os = "android"))]
fn read_to_string<P: AsRef<std::path::Path>>(p: P) -> Option<String> {
    std::fs::read_to_string(p).ok()
}
#[cfg(any(target_os = "linux", target_os = "android"))]
fn parse_size_token(s: &str) -> Option<usize> {
    // Sysfs uses e.g. "32K", "256K", "2M"
    let t = s.trim();
    let (num, suffix) = t.split_at(t.chars().take_while(|c| c.is_ascii_digit()).count());
    let n: usize = num.parse().ok()?;
    let mult = match suffix.trim().to_ascii_uppercase().as_str() {
        "K" => 1024,
        "M" => 1024 * 1024,
        "G" => 1024 * 1024 * 1024,
        ""  => 1,
        _ => return None,
    };
    Some(n * mult)
}
#[cfg(any(target_os = "linux", target_os = "android"))]
fn cache_line_size() -> Option<usize> {
    // Try coherency_line_size from any data cache
    for i in 0..8 {
        let typ = read_to_string(format!("/sys/devices/system/cpu/cpu0/cache/index{}/type", i))?;
        if typ.trim() == "Data" {
            if let Some(s) = read_to_string(format!("/sys/devices/system/cpu/cpu0/cache/index{}/coherency_line_size", i)) {
                if let Ok(n) = s.trim().parse::<usize>() { if n > 0 { return Some(n); } }
            }
        }
    }
    None
}
#[cfg(any(target_os = "linux", target_os = "android"))]
fn cache_size_by_level(level: &str) -> Option<usize> {
    for i in 0..8 {
        let typ = read_to_string(format!("/sys/devices/system/cpu/cpu0/cache/index{}/type", i))?;
        let lev = read_to_string(format!("/sys/devices/system/cpu/cpu0/cache/index{}/level", i))?;
        if typ.trim() == "Data" && lev.trim() == level {
            if let Some(s) = read_to_string(format!("/sys/devices/system/cpu/cpu0/cache/index{}/size", i)) {
                if let Some(n) = parse_size_token(&s) { return Some(n); }
            }
        }
    }
    None
}
#[cfg(any(target_os = "linux", target_os = "android"))]
fn l1_size() -> Option<usize> { cache_size_by_level("1") }
#[cfg(any(target_os = "linux", target_os = "android"))]
fn l2_size() -> Option<usize> { cache_size_by_level("2") }
#[cfg(any(target_os = "linux", target_os = "android"))]
fn l3_size() -> Option<usize> { cache_size_by_level("3") }

#[cfg(any(target_os = "linux", target_os = "android"))]
fn total_ram_bytes() -> Option<u64> {
    // /proc/meminfo: "MemTotal:  16367168 kB"
    if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kb * 1024);
            }
        }
    }
    None
}

/* --------------------- Other / WASM / Fallbacks --------------------- */

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
fn cache_line_size() -> Option<usize> { None }
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
fn l1_size() -> Option<usize> { None }
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
fn l2_size() -> Option<usize> { None }
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
fn l3_size() -> Option<usize> { None }
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
fn total_ram_bytes() -> Option<u64> { None }