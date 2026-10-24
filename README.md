# elastic-swap

Swap that grows and shrinks on demand, written in Rust. Cold pages are
compressed in RAM first (zswap, or a zram pool) and spill to swap files that are
created as memory runs short and removed once it recovers.

elastic-swap is a fork of
[biglinux-systemd-swap](https://github.com/biglinux/biglinux-systemd-swap), a
Rust rewrite of [systemd-swap](https://github.com/Nefelim4ag/systemd-swap) by
Timofey Titovets. It is not part of systemd. See [NOTICE](NOTICE) and [AUTHORS](AUTHORS).

It targets image-based systems with a read-only root: swap files live in
`/var/swap`, and nothing edits `/etc/fstab`.

## How It Works

### Swap Modes

| Mode | Primary | Secondary | Selection |
|------|---------|-----------|-----------|
| `zswap+swapfile` | Zswap (kernel) | Swap files (disk) | **Default** |
| `auto` | Auto-detected | Auto-detected | Picks a mode from the hardware |
| `zram+swapfile` | Zram (RAM) | Swap files (disk) | btrfs / ext4 / xfs with free space |
| `zram` | Zram (RAM) | None | LiveCD, low disk, tmpfs |
| `manual` | Explicit flags | Explicit flags | Advanced users |
| `disabled` | — | — | Service exits cleanly |

### Auto-Detection Logic

In `auto` mode, the daemon checks:

1. **LiveCD?** (tmpfs/squashfs/overlay root) → `zram` only
2. **Filesystem supports swap files?** (btrfs/ext4/xfs) → if no, `zram` only
3. **Free disk ≥ RAM?** → if no, `zram` only
4. **Otherwise** → `zram+swapfile` (zram primary + disk overflow)

### Zram Pool Architecture

The daemon manages a **dynamic pool of zram devices** that expands and
contracts based on demand:

- **Initial pool**: one device sized to 95% of `mem_limit` (compression is
  per-CPU since Linux 4.7)
- **Expansion**: at 85% utilization, adds a device with only the slots the
  remaining RAM could back if the next data did not compress at all. Data
  that compresses 4x leaves three quarters of each page in the budget, so the
  pool grows with it toward `zram_size` (150% of RAM); random data earns
  nothing
- **Contraction**: removes idle devices when utilization drops below 20% for
  120s, or the last device as soon as the pool advertises slots its RAM can no
  longer back and that device is cheap to empty
- **Monitoring**: woken by the kernel the moment memory pressure starts
  (a PSI trigger, no cost while idle), otherwise every 5 seconds for slow
  trends; new devices are `swapon`ed directly, before systemd adopts the unit

Each zram device uses:
- **Algorithm**: zstd (level 3) — best ratio-to-speed balance
- **mem_limit**: 50% of RAM for the pool. The kernel only offers a limit per
  device, so each device may use what it holds plus everything the pool has
  left; with one priority per device the kernel writes one device at a time
  and the limit binds only when the pool total does
- **Reserve**: expansion commits 65% of that ceiling. When churn eats into the
  rest — compressible pages leaving free slots behind without the RAM to back
  them — incompressible pages are written to the writeback store on disk
- **Priority**: 32767 for the first device, one less for each later one
  (kernel uses zram before disk swap)

Sizing works this way because a slot the pool cannot back in RAM is a trap,
and the kernel cannot tell: once `mem_limit` is reached, writes to the device
fail, but `should_reclaim_retry()` still counts the unbacked slots as free
swap and keeps aiming reclaim at the same device instead of falling through
to the swap files below it. Measured in a VM: a pool advertising 3.0x against
data compressing 1.02x produced 1.5 million `Write-error on swap-device`
lines, left the swap files at 0 bytes, and hung with no OOM kill.

**Compression ratios** (typical):
- Desktop workloads: 3–4x
- Server / text-heavy: 5–10x
- Incompressible data (media, encrypted): ~1x

### Swap Files (Overflow)

In `zram+swapfile` mode, swap files provide emergency overflow:

- **Size**: 512MB each, created on demand; a headroom file on btrfs is sized
  to bring total free swap back to twice the trigger level
- **Maximum**: 28 files (14GB total capacity)
- **Priority**: negative, below zram (kernel only uses them when zram is full)
- **NOCOW**: enabled on btrfs (prevents deadlock under pressure)
- **Created when**: free space in the files < 40%, every file ≥ 85% full, or
  free RAM ≤ 20% with total free swap under 15% (ahead of systemd-oomd's
  default 90% swap limit)
- **Removed when**: free space in the files > 70% and free RAM > 20%

### Zswap Mode

In `zswap+swapfile` mode, the kernel's zswap handles compression:

- Compresses pages before writing to disk swap
- Shrinker moves cold compressed pages to disk automatically
- Pool limited to 45% of RAM
- Requires disk-backed swap files as backing storage

## Recommended Kernel Tuning

On start the daemon sets `vm.min_free_kbytes` and THP `madvise` (switch off with
`kernel_tuning=no`) and MGLRU `min_ttl_ms`. The other parameters are **not applied** — they
are recommendations for zram/zswap. Configure them via
`/etc/sysctl.d/99-swap.conf` or your distribution's tuning service.

### Memory Management

| Parameter | Value | Purpose |
|-----------|-------|---------|
| `vm.swappiness` | 120 (zram+swapfile) / 180 (zram only) | Prefer swap over file cache (zram is in-memory, so swapping is fast) |
| `vm.min_free_kbytes` | 3% of RAM (max 512MB) | Emergency reserve — gives kswapd headroom before OOM |
| `vm.watermark_scale_factor` | 150 (1.5%) | Gap between min/low/high watermarks for kswapd headroom |
| `vm.vfs_cache_pressure` | 75 | Balance between VFS cache retention and anonymous page reclaim |
| `vm.dirty_ratio` | 10 | Max dirty pages before blocking writes (reduces memory pressure) |
| `vm.dirty_background_ratio` | 3 | Start background writeback early |
| `vm.page-cluster` | 0 (zram) / 2 (zswap) | Pages read per swap-in. 0 = page-at-a-time (optimal for zram) |

### Memory Compaction

| Parameter | Value | Purpose |
|-----------|-------|---------|
| `vm.compaction_proactiveness` | 20 | Background defragmentation level. Lower avoids CPU waste with zram-heavy workloads |
| `vm.watermark_boost_factor` | 15000 | Boosts watermarks after fragmentation events for compaction recovery |
| `vm.extfrag_threshold` | 300 | Eagerness to compact vs reclaim. Lower = more willing to compact |

### Transparent Huge Pages

| Parameter | Value | Purpose |
|-----------|-------|---------|
| THP enabled | `madvise` | Only apps requesting huge pages get them — avoids compaction stalls |

### MGLRU (Multi-Gen LRU)

| Parameter | Value | Purpose |
|-----------|-------|---------|
| `min_ttl_ms` | 1000 | Pages younger than 1s are never reclaimed — protects working set from thrashing |

## Installation

### Fedora and other RPM systems

```bash
packaging/build-rpm.sh          # writes dist/*.rpm; needs rpm-build, cargo, rust
sudo dnf install dist/elastic-swap-*.x86_64.rpm
sudo systemctl enable --now elastic-swap
```

When publishing a release, attach `dist/*.src.rpm` and the two source tarballs
to the same release as the binary RPM: GPLv3 section 6 requires the
corresponding source to be offered where the binary is.

### Manual Build

Requirements: Rust 1.93+, `util-linux`, `btrfs-progs` and `e2fsprogs` (for `chattr`)

```bash
cargo build --release
sudo make install
sudo systemctl enable --now elastic-swap
```

## Usage

### Check Status

```bash
elastic-swap status
```

Shows zram pool stats (compression ratio, utilization, device count),
swap file details, and memory breakdown.

### Show Recommended Config

```bash
sudo elastic-swap autoconfig
```

Displays the auto-detected configuration for the current hardware.

### Restart

```bash
sudo systemctl restart elastic-swap
```

### View Logs

```bash
journalctl -u elastic-swap -f
```

## Configuration

Configuration files (in order of priority):

1. `/usr/share/elastic-swap/swap-default.conf` — defaults (do not edit)
2. `/etc/elastic-swap/swap.conf` — user overrides (not installed; create it)
3. `/usr/lib/`, `/run/` and `/etc/elastic-swap/swap.conf.d/*.conf` — drop-in fragments

All options support `${NCPU}` and `${RAM_SIZE}` variables, plus simple
arithmetic with `$(( expr ))`.

### Common Options

**Change swap mode:**
```ini
swap_mode=zram+swapfile    # or: auto, zram, zswap+swapfile, manual, disabled
```

**Customize zram size:**
```ini
zram_size=200%             # Virtual disksize (% of RAM)
```

**Customize swap file location:**
```ini
swapfile_path=/mnt/data/swapfile
```

**Adjust anti-thrashing protection:**
```ini
mglru_min_ttl_ms=3000      # Higher = more protection, less reclaim
```

**Customize zram pool behavior:**
```ini
zram_expand_threshold=90        # Expand pool above this utilization %
zram_contract_threshold=15      # Contract pool below this utilization %
```

### Full Option Reference

See `/usr/share/elastic-swap/swap-default.conf` for all available options
with descriptions.

## Architecture

```
elastic-swap (Rust daemon)
├── main.rs          — CLI (clap), mode dispatch, kernel tuning, THP/MGLRU
├── lib.rs           — Module declarations, global SHUTDOWN/DRY_RUN flags
├── config.rs        — Config parser (key=value, ${VAR} expansion, arithmetic)
├── defaults.rs      — Compile-time defaults (paths, MGLRU TTL, fallbacks)
├── autoconfig.rs    — Hardware detection, recommended config generation
├── zram.rs          — Dynamic zram pool (expansion, contraction, monitoring)
├── swapfile.rs      — Dynamic swap file management (preallocated, NOCOW on btrfs)
├── zswap.rs         — Zswap kernel module configuration
├── meminfo.rs       — /proc/meminfo parser, effective swap calculation
├── systemd.rs       — Systemd unit generation, sd-notify
└── helpers.rs       — Shared utilities (parse_size, fs detection, logging)
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full module map, state
machine and external-process inventory.

### Data Flow

```
Memory pressure (free RAM < threshold)
  → MGLRU protects working set (pages < 1s old)
  → Kernel swaps cold anonymous pages:
      ├─ zram: compress with zstd level 3 → store in RAM
      │   ├─ Pool utilization > 85% → daemon adds zram device
      │   └─ All disksize consumed → kernel falls back to swapfiles
      └─ zswap: compress in kernel pool → shrinker writes back to disk

SwapFile monitor (1s interval while files exist):
  ├─ files' free space < 40% → create 512MB swap file
  ├─ free_ram ≤ 20% and total free swap < 15% → create a headroom file now
  └─ files' free space > 70% and free_ram > 20% → remove an idle file

ZramPool monitor (5s interval):
  ├─ utilization > 85% → add zram device (up to 8)
  └─ utilization < 20% (120s stable) → remove idle device
```

## Features

- **Zero configuration**: works out of the box for any system
- **Dynamic scaling**: creates/removes swap resources on demand
- **MGLRU integration**: protects working set from premature eviction (kernel 6.1+)
- **Zswap disabled for zram**: prevents double compression per kernel docs
- **NOCOW swap files**: safe on btrfs under memory pressure
- **Adopt on restart**: a new instance reuses the zram devices and swap files left active, without swapoff
- **Stop without swapoff**: stopping or removing the service leaves active swap in place; nothing is forced back into RAM

## For contributors and AI agents

- [AGENTS.md](AGENTS.md) — entrypoint: real build/test/lint commands, module
  map, where to edit.
- [ARCHITECTURE.md](ARCHITECTURE.md) — module map, state machine, external
  processes.
- [INVARIANTS.md](INVARIANTS.md) — H1–H7 living contract (memory safety,
  subprocess argv, no fstab writes, hardening, config parser, shutdown).

Quick gate: `cargo fmt --all -- --check && cargo clippy --workspace
--all-targets --all-features -- -D warnings && cargo test --all-features
--workspace`. Full CI in `.github/workflows/ci.yml`.

## License

GPL-3.0-only. See [NOTICE](NOTICE) for why and for the upstream credits.
