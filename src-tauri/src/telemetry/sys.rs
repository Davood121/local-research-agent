use std::path::Path;

use sysinfo::{Disks, System};

use super::format::bytes_to_gb;

pub struct SysSampler {
    system: System,
    disks: Disks,
}

pub struct SysSnapshot {
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub disk_free_gb: f32,
}

impl SysSampler {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_memory();
        Self {
            system,
            disks: Disks::new_with_refreshed_list(),
        }
    }

    /// Cheap refresh: memory counters + cached disk list. Called on a timer, so
    /// it must not enumerate volumes from scratch on every tick.
    pub fn sample(&mut self, data_dir: &Path) -> SysSnapshot {
        self.system.refresh_memory();
        self.disks.refresh(true);

        SysSnapshot {
            ram_used_gb: bytes_to_gb(self.system.used_memory()),
            ram_total_gb: bytes_to_gb(self.system.total_memory()),
            disk_free_gb: available_gb(&self.disks, data_dir),
        }
    }
}

impl Default for SysSampler {
    fn default() -> Self {
        Self::new()
    }
}

/// Longest mount-point prefix wins, which picks the correct volume when one is
/// mounted inside another (e.g. a home partition under `/`).
pub fn available_gb(disks: &Disks, path: &Path) -> f32 {
    best_available(
        disks
            .list()
            .iter()
            .map(|disk| (disk.mount_point(), disk.available_space())),
        path,
    )
    .map(bytes_to_gb)
    .unwrap_or(0.0)
}

fn best_available<'a>(
    candidates: impl Iterator<Item = (&'a Path, u64)>,
    path: &Path,
) -> Option<u64> {
    candidates
        .filter(|(mount, _)| path.starts_with(mount))
        .max_by_key(|(mount, _)| mount.as_os_str().len())
        .map(|(_, available)| available)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn picks_the_longest_matching_mount() {
        let candidates = [
            (Path::new("data"), 100_u64),
            (Path::new("data/app"), 200_u64),
            (Path::new("other"), 300_u64),
        ];

        let picked = best_available(candidates.into_iter(), Path::new("data/app/sub"));
        assert_eq!(picked, Some(200));
    }

    #[test]
    fn reports_no_match_instead_of_guessing() {
        let candidates = [(Path::new("data"), 100_u64)];
        assert_eq!(
            best_available(candidates.into_iter(), Path::new("elsewhere/x")),
            None
        );
    }
}
