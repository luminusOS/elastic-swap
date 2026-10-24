//! Kernel memory settings that suit compressed swap on a desktop.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;

use crate::config::Config;
use crate::meminfo::get_ram_size;
use crate::{info, warn};

const MIN_FREE_KBYTES_PATH: &str = "/proc/sys/vm/min_free_kbytes";
const THP_ENABLED_PATH: &str = "/sys/kernel/mm/transparent_hugepage/enabled";
const MIN_FREE_CAP_KB: u64 = 512 * 1024;

/// 3 % of RAM, capped at 512 MiB: headroom for kswapd to reclaim before an OOM.
pub fn min_free_kbytes(ram_bytes: u64) -> u64 {
    (ram_bytes / 1024 * 3 / 100).min(MIN_FREE_CAP_KB)
}

fn enabled(config: &Config) -> bool {
    !matches!(
        config
            .get_opt("kernel_tuning")
            .map(str::to_lowercase)
            .as_deref(),
        Some("no" | "n" | "0" | "false" | "off")
    )
}

/// Set `vm.min_free_kbytes` and transparent huge pages to `madvise`.
///
/// `always` promotes anonymous ranges to 2 MiB pages, which a desktop pays for
/// in internal fragmentation; `madvise` keeps huge pages for programs that ask.
pub fn apply(config: &Config) {
    if !enabled(config) {
        return;
    }

    if let Ok(ram) = get_ram_size() {
        let kb = min_free_kbytes(ram);
        match fs::write(MIN_FREE_KBYTES_PATH, kb.to_string()) {
            Ok(()) => info!("Tuning: vm.min_free_kbytes = {}", kb),
            Err(e) => warn!("Tuning: vm.min_free_kbytes not set: {}", e),
        }
    }

    if fs::metadata(THP_ENABLED_PATH).is_ok() {
        match fs::write(THP_ENABLED_PATH, "madvise") {
            Ok(()) => info!("Tuning: transparent_hugepage = madvise"),
            Err(e) => warn!("Tuning: transparent_hugepage not set: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn min_free_is_three_percent_of_ram() {
        assert_eq!(min_free_kbytes(4 * GIB), 4 * 1024 * 1024 * 3 / 100);
    }

    #[test]
    fn min_free_is_capped_at_512_mib() {
        assert_eq!(min_free_kbytes(64 * GIB), 524_288);
        assert_eq!(min_free_kbytes(17 * GIB), 524_288);
    }

    #[test]
    fn min_free_of_no_ram_is_zero() {
        assert_eq!(min_free_kbytes(0), 0);
    }

    #[test]
    fn tuning_is_on_unless_disabled() {
        assert!(enabled(&Config::from_pairs_for_tests([("x", "y")])));
        for off in ["no", "NO", "0", "false", "n", "off"] {
            assert!(!enabled(&Config::from_pairs_for_tests([(
                "kernel_tuning",
                off
            )])));
        }
        assert!(enabled(&Config::from_pairs_for_tests([(
            "kernel_tuning",
            "yes"
        )])));
    }
}
