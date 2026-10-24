//! Checks on the shipped systemd unit files.
//!
//! These read `include/*.service` at compile time and fail on settings that
//! would stop the unit before a single line of the daemon runs.
// SPDX-License-Identifier: GPL-3.0-or-later

// ── Unit-file sandbox reachability ──────────────────────────────────────────
//
// `ProtectSystem=strict` makes `ReadWritePaths=` boot-critical: systemd builds
// the mount namespace *before* ExecStart, so an entry that does not exist
// aborts the unit with 226/NAMESPACE without running a single line of this
// daemon. Measured on a live host: a stale `/var/lib/elastic-swap` entry that
// nothing in this tree creates or writes left the machine with no swap at all
// and 4637 restart attempts. A path that may legitimately be absent must carry
// systemd's `-` prefix (ignore-if-missing) instead.

#[test]
fn unit_readwritepaths_are_all_reachable() {
    let line = include_str!("../include/elastic-swap.service")
        .lines()
        .find_map(|l| l.strip_prefix("ReadWritePaths="))
        .expect("elastic-swap.service must declare ReadWritePaths");

    for entry in line.split_whitespace() {
        // Created at runtime by `elastic-swap prepare`; its absence only
        // costs disk swap, so it must not fail the whole namespace.
        if let Some(optional) = entry.strip_prefix('-') {
            assert!(
                optional.starts_with('/'),
                "{entry}: ReadWritePaths entries must be absolute"
            );
            continue;
        }
        assert!(
            entry.starts_with('/'),
            "{entry}: ReadWritePaths entries must be absolute"
        );
        assert!(
            std::path::Path::new(entry).exists(),
            "{entry} does not exist, so systemd cannot bind it read-write and \
             the unit dies 226/NAMESPACE before ExecStart. Either drop the \
             entry or mark it optional as -{entry}."
        );
    }
}

// `DevicePolicy=auto` permits every device only while no `DeviceAllow=` exists;
// a single entry promotes it to `strict`. The daemon hot-adds its own zram and
// loop nodes at runtime, so an allowlist can only be incomplete. Measured cost
// of getting this wrong: `DeviceAllow=block-loop rw` denied /dev/zram*, mkswap
// returned EPERM, and the pool never started.
#[test]
fn unit_does_not_narrow_device_policy() {
    let unit = include_str!("../include/elastic-swap.service");
    assert!(
        !unit
            .lines()
            .any(|l| l.trim_start().starts_with("DeviceAllow=")),
        "elastic-swap.service: DeviceAllow= turns DevicePolicy=auto into strict \
         and locks out the zram/loop nodes this daemon creates at runtime"
    );
}

// `prepare` has to run unsandboxed (`+`) and before the daemon: inside the
// namespace the swap directory is a bind mount and cannot become a subvolume.
#[test]
fn unit_prepares_swap_directory_before_the_daemon_starts() {
    let unit = include_str!("../include/elastic-swap.service");
    let position = |needle: &str| {
        unit.lines()
            .position(|l| l == needle)
            .unwrap_or_else(|| panic!("{needle} missing from elastic-swap.service"))
    };

    assert!(
        position("ExecStartPre=-+/usr/bin/elastic-swap prepare")
            < position("ExecStart=/usr/bin/elastic-swap start")
    );
}

#[test]
fn unit_opens_the_default_swap_directory() {
    let unit = include_str!("../include/elastic-swap.service");
    let paths = unit
        .lines()
        .find_map(|l| l.strip_prefix("ReadWritePaths="))
        .expect("ReadWritePaths missing");

    let expected = format!("-{}", elastic_swap::defaults::SWAPFILE_PATH);
    assert!(
        paths.split_whitespace().any(|p| p == expected),
        "ReadWritePaths={paths} does not open {expected}"
    );
}

#[test]
fn unit_does_not_depend_on_a_helper_that_edits_fstab() {
    let unit = include_str!("../include/elastic-swap.service");
    for dependency in ["Requires=", "Wants=", "After=", "Before="] {
        for line in unit.lines().filter(|l| l.starts_with(dependency)) {
            assert!(!line.contains("pre-"), "{line}: the helper unit is gone");
        }
    }
    assert!(unit.contains("Wants=modprobe@zram.service"));
}

#[test]
fn shipped_defaults_select_zswap_with_swap_files() {
    let defaults = include_str!("../include/swap-default.conf");
    let mode = defaults
        .lines()
        .find_map(|l| l.strip_prefix("swap_mode="))
        .expect("swap_mode missing");
    assert_eq!(mode, "zswap+swapfile");
}
