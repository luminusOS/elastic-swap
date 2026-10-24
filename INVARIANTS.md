# INVARIANTS — elastic-swap

Living contract. Each invariant: rule → reason → validation.

Scope: single binary `/usr/bin/elastic-swap` (Rust, no workspace).
Daemon launched by a systemd unit at boot. Manages a zram pool, zswap and
swap files dynamically.

"Validation" names the check that enforces or evidences the rule today.
Where no automated test exists yet, it is stated explicitly so an agent does
not look for one.

## H1 — Memory safety

### H1.1 — `#![deny(unsafe_code)]` in `lib.rs`; the only `unsafe` is `libc::swapoff`

- Rule: `src/lib.rs` carries `#![deny(unsafe_code)]`. The single `unsafe`
  block is in `src/systemd.rs` invoking `libc::swapoff` with a local
  `#[allow(unsafe_code)]` and a `SAFETY:` comment justifying the pointer
  validity of the `CString`.
- Rule: every caller that needs to disable a swap device goes through
  `systemd::swapoff`. Nothing spawns the `swapoff` binary, so the `unsafe`
  surface stays at one block however many call sites appear.
- Reason: minimal `unsafe` surface; `swapoff(2)` has no safe wrapper in
  `nix` 0.30 (no `sys::swap` module under any feature) nor in `rustix` 1.1,
  and pulling in a crate to wrap one syscall would cost more than the single
  audited line it removes. Shelling out to `/usr/sbin/swapoff` would remove
  the `unsafe` but spawns a process on the teardown path — where memory is
  tightest and a spawn is least likely to succeed — and reduces a real errno
  to an exit status.
- Note: gettext-rs briefly added a second block here. Its `setlocale` is
  `unsafe` in 0.8 because it mutates process-global locale state with no
  synchronisation (RUSTSEC-2026-0244), and the daemon carried a guard to
  uphold the precondition. Dropping i18n removed the dependency, the block
  and the advisory together — the cheapest way to satisfy an invariant is
  not to need the code it constrains.
- Validation: `grep -Rn 'unsafe' src/` returns exactly the documented block
  in `systemd.rs`; `grep -n '#!\[deny(unsafe_code)\]' src/lib.rs`.

## H2 — Subprocess argv

### H2.1 — `run_cmd_output` takes a vector argv; no `sh -c`

- Rule: external binaries are spawned with `Command::new(<fixed binary>)`
  and an argv of separate arguments; new call sites go through
  `src/helpers.rs::run_cmd_output(&[&str])`. A handful of older sites
  (`systemctl`, `systemd-escape`, `findmnt`, `mkswap`, `btrfs`, `chattr`,
  `swapon`, `du`) build their own `Command` the same way — the full list is
  ARCHITECTURE.md §5. `Command::new("sh"|"bash")` is forbidden in production
  Rust (no shell script ships in this package).
- Reason: paths and device names come from system detection; the rule
  prevents injection in any future regression.
- Validation: `! grep -RnE 'Command::new\("(sh|bash)"\)' src/`.

### H2.2 — `systemd-escape` applied to every unit name derived from a path

- Rule: `systemd::gen_swap_unit` runs `systemd-escape --path` before composing
  `.swap` unit names.
- Reason: paths with `/`, `-`, `\\` become invalid unit names without
  escaping; silent reuse could collide.
- Validation: `grep -n 'systemd-escape' src/systemd.rs`. No dedicated
  corpus test; escaping is exercised on real hardware by every unit the
  daemon generates (`tests/stress/`).

## H3 — fstab integrity

### H3.1 — elastic-swap never writes `/etc/fstab`

- Rule: no code path opens `/etc/fstab` for writing. Swap is activated with
  `swapon` and described to systemd by generated `.swap` units in
  `/run/systemd/system`.
- Reason: a corrupt fstab blocks the next boot, and the root of an image-based
  system is read-only anyway. This replaces the atomic-append rule of the
  fork's bash helper, which is gone.
- Validation: `! grep -Rn 'fstab' src/ | grep -v '//'` finds no write.

## H4 — MGLRU tuning

### H4.1 — MGLRU `min_ttl_ms` comes from a compile-time default, written via sysfs

- Rule: `src/defaults.rs` defines `MGLRU_MIN_TTL_MS`; `autoconfig`
  injects `mglru_min_ttl_ms` and `main.rs::configure_mglru()` writes it to
  `/sys/kernel/mm/lru_gen/min_ttl_ms`, no-op when the sysfs node is absent.
- Reason: the kernel exposes the lru_gen node only when MGLRU is built in;
  the write is best-effort, so no `uname -r` version parser is needed.
- Validation: `src/autoconfig.rs` `#[cfg(test)]` asserts `mglru_min_ttl_ms`
  is injected in every mode. There is no kernel-version test because there
  is no kernel-version parser in the tree.

## H5 — Privilege hardening

### H5.1 — systemd unit with `SystemCallFilter=@system-service @swap @mount`

- Rule: `include/elastic-swap.service` declares
  `SystemCallFilter=@system-service @swap @mount`, `NoNewPrivileges=yes`,
  `ProtectSystem=strict`, `ProtectHome=read-only`,
  `CapabilityBoundingSet=CAP_SYS_ADMIN`.
- Reason: the daemon runs as root; a minimal sandbox reduces blast radius
  if a bug exists.
- Validation: `grep -E 'SystemCallFilter|NoNewPrivileges|ProtectSystem'
  include/elastic-swap.service` lists the three.

### H5.2 — `prepare` runs unsandboxed before the daemon; the daemon is `Type=simple`

- Rule: `include/elastic-swap.service` runs `ExecStartPre=-+/usr/bin/elastic-swap
  prepare` and then `ExecStart=… start`, `Type=simple` with
  `RemainAfterExit=yes`. `prepare` only creates the swap directory.
- Reason: under `ProtectSystem=strict` the swap directory is a bind mount and
  cannot be replaced by a btrfs subvolume, so that step has to happen before
  the namespace is built. Everything else stays sandboxed.
- Validation: `tests/boundaries.rs::unit_prepares_swap_directory_before_the_daemon_starts`.
- Note: because the daemon is `Type=simple`, systemd ignores `READY=1`. The
  `sd-notify` calls in `src/main.rs` (`notify_ready`, status updates) are
  advisory only — useful for logs and a future `Type=notify` switch, not a
  readiness handshake today. Code and unit agree on `Type=simple`.
- Validation: `grep -E 'Type=|ExecStart' include/elastic-swap.service`.

## H6 — Config parser safety

### H6.1 — `${VAR}` and `$(( … ))` in swap.conf are parsed with a typed scope, no shell

- Rule: `src/config.rs::expand_value` resolves `${VAR}` from an explicit map
  (then `std::env`) and evaluates `$(( expr ))` with
  `evaluate_simple_arithmetic` (integer `+ - * /` only). The config string
  is never passed to `sh -c`. Command substitution `$( … )` (no double
  paren) is not interpreted at all — it is left as-is, never executed.
- Reason: `/etc/elastic-swap/swap.conf` is root-owned, but a parser bug could
  otherwise escalate into command injection via include drop-ins.
- Validation: `src/config.rs` `#[cfg(test)]` covers the arithmetic
  evaluator; `tests/behavior.rs` exercises
  `${VAR}`/`$(( … ))` expansion against realistic `swap.conf` fragments
  written via `tempfile`. There is no `tests/config_corpus.rs`; the safety
  property holds structurally because only `$(( … ))` is ever evaluated.

## H7 — Shutdown lifecycle

### H7.1 — Stopping the service never swaps off

- Rule: the unit has no `ExecStop=` and the binary no `stop` command. SIGTERM
  sets `SHUTDOWN` (`ctrlc`, `Ordering::Release`, read with `Acquire`); the
  blocking loops (`ZramPool::run_monitor`, `SwapFile::run`,
  `idle_until_shutdown`) return and the process exits, leaving every zram
  device and swap file active. The next `start` adopts them
  (`clear_previous_instance` only restores zswap settings and clears the work
  directory); at shutdown systemd deactivates the generated `.swap` units.
- Reason: a swapoff has to fit every swapped page back into RAM at once, and
  a restart, an uninstall or a shutdown is no reason to do that. Measured on
  the 3.7 GB notebook: a restart under load sat in swapoff until
  `TimeoutStopSec` killed it, and the still-running daemon created files #4
  and #5 while #1–#3 were being removed.
- Validation: on a test machine, `systemctl restart elastic-swap` with two
  zram devices and two swap files active keeps `swapon --show` unchanged,
  logs no swapoff, and the new instance reports the devices as adopted. No
  automated test: it needs systemd and root. The stop is bounded by the
  loops' poll interval (`swapfile_frequency`, default 1 s;
  `zram_check_interval`, default 5 s; the idle loop, 60 s).

