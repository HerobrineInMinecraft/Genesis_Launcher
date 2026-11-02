//! 物理内存检测。
//!
//! 启动器可分配内存上限来自操作系统报告的物理内存总量，并按 512MB
//! 向下对齐，保证加减按钮始终落在整数步进上。

use std::sync::OnceLock;

const MEMORY_STEP_MB: u64 = 512;
const MIN_MEMORY_MB: u64 = 2048;
const FALLBACK_MEMORY_MB: u64 = 16 * 1024;

/// 返回本机可分配给 Minecraft 的内存上限（MB）。
pub(super) fn memory_limit_mb() -> u16 {
    static LIMIT: OnceLock<u16> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        let total = physical_memory_mb().unwrap_or(FALLBACK_MEMORY_MB);
        align_memory_limit(total)
    })
}

/// 把物理内存总量转换为可用的 512MB 对齐上限。
fn align_memory_limit(total_mb: u64) -> u16 {
    let aligned = total_mb / MEMORY_STEP_MB * MEMORY_STEP_MB;
    aligned.max(MIN_MEMORY_MB).min(u16::MAX as u64) as u16
}

#[cfg(target_os = "windows")]
fn physical_memory_mb() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return None;
    }
    Some(status.ullTotalPhys / (1024 * 1024))
}

#[cfg(target_os = "linux")]
fn physical_memory_mb() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kib = contents
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    Some(kib / 1024)
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn physical_memory_mb() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_physical_memory_down_to_supported_step() {
        assert_eq!(align_memory_limit(15_912), 15_872);
        assert_eq!(align_memory_limit(16_384), 16_384);
        assert_eq!(align_memory_limit(1_024), 2_048);
    }
}
