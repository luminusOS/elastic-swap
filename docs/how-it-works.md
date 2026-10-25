# How elastic-swap works

elastic-swap runs as a systemd service. It picks a swap mode at start, sets up the swap devices that mode needs, and keeps resizing them while the machine runs. For the module map, the state machine and the external processes it calls, see [ARCHITECTURE.md](../ARCHITECTURE.md).

## Swap modes

Set the mode with `swap_mode` in the configuration (see [configuration.md](configuration.md)).

| Mode | Primary | Secondary | When to use it |
|------|---------|-----------|----------------|
| `zswap+swapfile` | Zswap (kernel) | Swap files (disk) | Default |
| `auto` | Detected | Detected | Lets the daemon pick from the hardware |
| `zram+swapfile` | Zram (RAM) | Swap files (disk) | btrfs, ext4 or xfs with free space |
| `zram` | Zram (RAM) | None | Live systems, low disk, tmpfs |
| `manual` | Explicit flags | Explicit flags | `zram_enabled`, `zswap_enabled` and `swapfile_enabled` decide |
| `disabled` | None | None | The service exits cleanly |

### Auto-detection

In `auto` mode the daemon checks, in order:

1. A live system (tmpfs, squashfs or overlay root) gets `zram` only.
2. A filesystem that cannot hold swap files (anything but btrfs, ext4 or xfs) gets `zram` only.
3. Less free disk than RAM gets `zram` only.
4. Everything else gets `zram+swapfile`.

## Zswap mode

In `zswap+swapfile` mode the kernel's zswap does the compression:

- Pages are compressed in a pool before they reach the disk.
- The shrinker moves cold compressed pages to the swap files.
- The pool is limited to 45% of RAM.
- Zswap needs a disk-backed swap device to write to, which is why the swap files are required.

The daemon disables zswap when a zram mode is selected, because the kernel documentation advises against compressing twice.

## Swap files

Swap files are the overflow of every mode except `zram`.

- Location: `/var/swap`. `elastic-swap prepare` creates it as a NOCOW subvolume on btrfs and as a plain directory elsewhere.
- Size: 512 MB each, created on demand. On btrfs a headroom file is sized to bring total free swap back to twice the trigger level.
- Limit: 28 files, 14 GB in total.
- Priority: negative, below zram, so the kernel uses them only when zram is full.
- NOCOW: enabled on btrfs, which prevents a deadlock under memory pressure.
- Created when: the free space in the files drops under 40%, every file is at least 85% full, or free RAM is at most 20% while total free swap is under 15%. The last rule acts ahead of systemd-oomd's default 90% swap limit.
- Removed when: the free space in the files rises over 70% and free RAM is over 20%. In `zswap+swapfile` mode the daemon is more careful, because the files are write-back targets: it needs more than 85% free (the setting is ignored), more than two unused files, and 300 seconds since the last file this run created.

## Zram pool

The zram modes manage a pool of zram devices that expands and contracts with demand.

- Initial pool: one device sized to 95% of `mem_limit`. Compression is per CPU since Linux 4.7, so a second device adds no throughput.
- Expansion: at 85% utilization the daemon adds a device with only the slots the remaining RAM could back if the next data did not compress at all. Data that compresses 4x leaves three quarters of each page in the budget, so the pool grows toward `zram_size` (150% of RAM). Random data earns nothing.
- Contraction: idle devices are removed when utilization stays under 20% for 120 seconds, and the last device is removed as soon as the pool advertises slots its RAM can no longer back and that device is cheap to empty.
- Monitoring: the kernel wakes the daemon the moment memory pressure starts (a PSI trigger, free while idle). Otherwise it checks every 5 seconds for slow trends. New devices are enabled with `swapon` directly, before systemd adopts the unit.

Each device uses:

- Algorithm: zstd (level 3), the best ratio for the speed.
- mem_limit: 50% of RAM for the pool. The kernel only offers a limit per device, so each device may use what it holds plus everything the pool has left. With one priority per device the kernel writes to one device at a time, and the limit binds only when the pool total does.
- Reserve: expansion commits 65% of that ceiling. When churn eats into the rest (compressible pages leave free slots behind without the RAM to back them), incompressible pages go to the writeback store on disk.
- Priority: 32767 for the first device and one less for each later one, so the kernel uses zram before disk swap.

Sizing works this way because a slot the pool cannot back in RAM is a trap the kernel cannot see. Once `mem_limit` is reached, writes to the device fail, but `should_reclaim_retry()` still counts the unbacked slots as free swap and keeps aiming reclaim at the same device instead of falling through to the swap files. In a VM, a pool advertising 3.0x against data compressing 1.02x produced 1.5 million `Write-error on swap-device` lines, left the swap files at 0 bytes and hung without an OOM kill.

Typical compression ratios are 3 to 4x for desktop workloads, 5 to 10x for text-heavy server loads, and about 1x for media or encrypted data.

## Data flow

```
Memory pressure (free RAM < threshold)
  -> MGLRU protects the working set (pages younger than 1s)
  -> the kernel swaps cold anonymous pages:
       zram:  compress with zstd level 3, store in RAM
                pool utilization > 85%  -> the daemon adds a zram device
                all disksize consumed   -> the kernel falls back to swap files
       zswap: compress in the kernel pool, the shrinker writes back to disk

Swap file monitor (every second while files exist):
  files' free space < 40%                        -> create a 512 MB swap file
  free RAM <= 20% and total free swap < 15%      -> create a headroom file now
  files' free space > 70% and free RAM > 20%     -> remove an idle file
                                                    (zswap mode: > 85% free, > 2 unused files, 300 s)

Zram pool monitor (every 5 seconds):
  utilization > 85%                   -> add a zram device (up to 8)
  utilization < 20% for 120 seconds   -> remove an idle device
```

## Restarts and shutdown

A new instance adopts the zram devices and swap files that are still active. Stopping or removing the service leaves active swap in place, so nothing is forced back into RAM. systemd deactivates the generated `.swap` units at shutdown.

## Source layout

```
src/
  main.rs        CLI (clap), mode dispatch, kernel tuning, THP and MGLRU
  lib.rs         module declarations, global SHUTDOWN and DRY_RUN flags
  config.rs      config parser (key=value, ${VAR} expansion, arithmetic)
  defaults.rs    compile-time defaults (paths, MGLRU TTL, fallbacks)
  autoconfig.rs  hardware detection and recommended configuration
  zram.rs        dynamic zram pool (expansion, contraction, monitoring)
  swapfile.rs    dynamic swap files (preallocated, NOCOW on btrfs)
  zswap.rs       zswap kernel module configuration
  tuning.rs      vm.min_free_kbytes and transparent huge pages
  meminfo.rs     /proc/meminfo parser, effective swap calculation
  systemd.rs     unit generation, sd-notify
  helpers.rs     shared utilities (parse_size, filesystem detection, logging)
```
