use nvml_wrapper::Nvml;

use crate::events::GpuStat;

use super::format::{bytes_to_gb, format_gb};

#[cfg(any(target_os = "linux", test))]
use std::fs;
#[cfg(any(target_os = "linux", test))]
use std::path::Path;

#[cfg(target_os = "linux")]
const AMD_DRM_ROOT: &str = "/sys/class/drm";

/// Probes the available telemetry backend once at startup.
///
/// Detection is cached: NVML handles are expensive to initialise, AMD sysfs
/// layout does not change at runtime, and `system_profiler` takes seconds. A
/// per-tick probe would violate the idle CPU budget.
pub struct GpuReader {
    backend: Backend,
}

enum Backend {
    // Boxed because `Nvml` is ~10 KB; most machines fall back to `Unavailable`.
    Nvidia(Box<Nvml>),
    #[cfg(target_os = "linux")]
    Amd,
    #[cfg(target_os = "macos")]
    AppleUnified,
    Unavailable,
}

impl GpuReader {
    pub async fn detect() -> Self {
        Self {
            backend: detect_backend().await,
        }
    }

    /// Reads the cached backend. Never blocks the runtime: NVML and sysfs calls
    /// are in-process and short, and the Apple subprocess only ran at startup.
    pub fn read(&self, system_memory_gb: (f32, f32)) -> GpuStat {
        #[cfg(not(target_os = "macos"))]
        let _ = system_memory_gb;

        match &self.backend {
            Backend::Nvidia(nvml) => read_nvidia(nvml),
            #[cfg(target_os = "linux")]
            Backend::Amd => read_amd(),
            #[cfg(target_os = "macos")]
            Backend::AppleUnified => GpuStat::AppleUnified {
                mem_used_gb: system_memory_gb.0,
                mem_total_gb: system_memory_gb.1,
            },
            Backend::Unavailable => GpuStat::Unavailable,
        }
    }

    pub fn label(&self) -> String {
        match &self.backend {
            Backend::Nvidia(nvml) => nvml
                .device_by_index(0)
                .ok()
                .and_then(|device| device.memory_info().ok())
                .map(|memory| format!("NVIDIA via NVML ({} GB VRAM)", format_gb(memory.total)))
                .unwrap_or_else(|| "NVIDIA via NVML".to_owned()),
            #[cfg(target_os = "linux")]
            Backend::Amd => parse_amd(Path::new(AMD_DRM_ROOT))
                .map(|(_, total)| format!("AMD sysfs ({} GB VRAM)", format_gb(total)))
                .unwrap_or_else(|| "AMD sysfs".to_owned()),
            #[cfg(target_os = "macos")]
            Backend::AppleUnified => "Apple Silicon (unified memory)".to_owned(),
            Backend::Unavailable => "unavailable".to_owned(),
        }
    }
}

async fn detect_backend() -> Backend {
    if let Some(nvml) = detect_nvidia() {
        return Backend::Nvidia(Box::new(nvml));
    }

    #[cfg(target_os = "linux")]
    {
        if parse_amd(Path::new(AMD_DRM_ROOT)).is_some() {
            return Backend::Amd;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if apple::detect().await {
            return Backend::AppleUnified;
        }
    }

    Backend::Unavailable
}

fn detect_nvidia() -> Option<Nvml> {
    // nvml-wrapper loads the driver library at runtime, so this is safe on
    // machines without an NVIDIA driver.
    let nvml = Nvml::init().ok()?;
    match nvml.device_count() {
        Ok(count) if count > 0 => Some(nvml),
        _ => None,
    }
}

fn read_nvidia(nvml: &Nvml) -> GpuStat {
    let device = match nvml.device_by_index(0) {
        Ok(device) => device,
        Err(_) => return GpuStat::Unavailable,
    };

    let memory = match device.memory_info() {
        Ok(memory) => memory,
        Err(_) => return GpuStat::Unavailable,
    };

    // Utilisation is optional: VRAM is still useful when the query fails.
    let util_pct = device
        .utilization_rates()
        .map(|utilization| utilization.gpu.min(100) as u8)
        .unwrap_or(0);

    GpuStat::Nvidia {
        vram_used_gb: bytes_to_gb(memory.used),
        vram_total_gb: bytes_to_gb(memory.total),
        util_pct,
    }
}

#[cfg(target_os = "linux")]
fn read_amd() -> GpuStat {
    match parse_amd(Path::new(AMD_DRM_ROOT)) {
        Some((used, total)) => GpuStat::Amd {
            vram_used_gb: bytes_to_gb(used),
            vram_total_gb: bytes_to_gb(total),
        },
        None => GpuStat::Unavailable,
    }
}

/// Exposed for the unit test, which points it at a temp directory.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_amd(drm_root: &Path) -> Option<(u64, u64)> {
    let entries = fs::read_dir(drm_root).ok()?;

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        if !file_name.to_string_lossy().starts_with("card") {
            continue;
        }

        if let Some(vram) = read_amd_card(&entry.path().join("device")) {
            return Some(vram);
        }
    }

    None
}

#[cfg(any(target_os = "linux", test))]
fn read_amd_card(device: &Path) -> Option<(u64, u64)> {
    let used = read_u64(&device.join("mem_info_vram_used"))?;
    let total = read_u64(&device.join("mem_info_vram_total"))?;
    // A total of zero means the card does not expose dedicated VRAM.
    (total > 0).then_some((used, total))
}

#[cfg(any(target_os = "linux", test))]
fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(any(target_os = "macos", test))]
mod apple {
    #[cfg(target_os = "macos")]
    use std::process::Stdio;
    #[cfg(target_os = "macos")]
    use std::time::Duration;

    #[cfg(target_os = "macos")]
    use tokio::process::Command;

    #[cfg(target_os = "macos")]
    const PROBE_TIMEOUT_MS: u64 = 3_000;

    /// Apple Silicon has no dedicated VRAM. Presence is detected once here;
    /// per-tick figures come from sysinfo's unified-memory counters.
    #[cfg(target_os = "macos")]
    pub async fn detect() -> bool {
        let mut command = Command::new("system_profiler");
        command
            .args(["SPDisplaysDataType", "-json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        match tokio::time::timeout(Duration::from_millis(PROBE_TIMEOUT_MS), command.output()).await
        {
            Ok(Ok(output)) if output.status.success() => parse(&output.stdout),
            _ => false,
        }
    }

    pub(super) fn parse(json: &[u8]) -> bool {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(json) else {
            return false;
        };

        value
            .get("SPDisplaysDataType")
            .and_then(|displays| displays.as_array())
            .map(|gpus| gpus.iter().any(is_apple_gpu))
            .unwrap_or(false)
    }

    fn is_apple_gpu(gpu: &serde_json::Value) -> bool {
        ["sppci_model", "_name", "spdisplays_vendor"]
            .iter()
            .filter_map(|key| gpu.get(*key))
            .filter_map(|value| value.as_str())
            .any(|value| value.contains("Apple"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn detects_apple_silicon() {
            let json = br#"{"SPDisplaysDataType":[{"_name":"Apple M2","sppci_model":"Apple M2"}]}"#;
            assert!(parse(json));
        }

        #[test]
        fn rejects_intel_and_malformed_payloads() {
            let intel =
                br#"{"SPDisplaysDataType":[{"_name":"Intel Iris Plus","sppci_model":"Intel Iris Plus"}]}"#;
            assert!(!parse(intel));
            assert!(!parse(b"not json"));
            assert!(!parse(b"{}"));
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            path.push(format!("lra-test-{tag}-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_vram(card_device: &Path, used: &str, total: &str) {
        fs::create_dir_all(card_device).unwrap();
        fs::write(card_device.join("mem_info_vram_used"), used).unwrap();
        fs::write(card_device.join("mem_info_vram_total"), total).unwrap();
    }

    #[test]
    fn parses_amd_sysfs_vram_files() {
        let dir = TempDir::new("amd");
        write_vram(
            &dir.path().join("card0").join("device"),
            "4402341478\n",
            "8589934592\n",
        );

        let (used, total) = parse_amd(dir.path()).unwrap();
        assert_eq!(total, 8_589_934_592);
        assert!((bytes_to_gb(used) - 4.1).abs() < 0.05);
        assert_eq!(bytes_to_gb(total), 8.0);
    }

    #[test]
    fn amd_parser_returns_none_for_unreadable_cards() {
        let dir = TempDir::new("amd-empty");
        fs::create_dir_all(dir.path().join("card0").join("device")).unwrap();

        assert_eq!(parse_amd(dir.path()), None);
    }

    #[test]
    fn amd_parser_skips_zero_vram_and_finds_the_next_card() {
        let dir = TempDir::new("amd-multi");
        write_vram(&dir.path().join("card0").join("device"), "0\n", "0\n");
        write_vram(
            &dir.path().join("card1").join("device"),
            "1073741824\n",
            "4294967296\n",
        );

        let (used, total) = parse_amd(dir.path()).unwrap();
        assert_eq!((used, total), (1_073_741_824, 4_294_967_296));
    }
}
