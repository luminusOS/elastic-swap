# Kernel tuning

On start the daemon sets `vm.min_free_kbytes` and transparent huge pages (`madvise`), and it sets MGLRU `min_ttl_ms`. Set `kernel_tuning=no` to leave the first two alone.

The other parameters below are recommendations for zram and zswap. The daemon does not apply them. Configure them with `/etc/sysctl.d/99-swap.conf` or your distribution's tuning service. The package ships only `vm.page-cluster = 0`, in `/usr/lib/sysctl.d/60-elastic-swap.conf`.

## Memory management

| Parameter | Value | Purpose |
|-----------|-------|---------|
| `vm.swappiness` | 120 (zram+swapfile), 180 (zram only) | Prefer swap over file cache. Zram is in memory, so swapping is fast. |
| `vm.min_free_kbytes` | 3% of RAM, at most 512 MB | Emergency reserve that gives kswapd headroom before an OOM. Set by the daemon. |
| `vm.watermark_scale_factor` | 150 (1.5%) | Gap between the min, low and high watermarks. |
| `vm.vfs_cache_pressure` | 75 | Balance between VFS cache retention and anonymous page reclaim. |
| `vm.dirty_ratio` | 10 | Maximum dirty pages before writes block, which reduces memory pressure. |
| `vm.dirty_background_ratio` | 3 | Starts background writeback early. |
| `vm.page-cluster` | 0 (zram), 2 (zswap) | Pages read per swap-in. 0 reads one page at a time, which is best for zram. |

## Memory compaction

| Parameter | Value | Purpose |
|-----------|-------|---------|
| `vm.compaction_proactiveness` | 20 | Background defragmentation level. A lower value avoids wasted CPU with zram-heavy workloads. |
| `vm.watermark_boost_factor` | 15000 | Boosts watermarks after fragmentation events so compaction can recover. |
| `vm.extfrag_threshold` | 300 | How eagerly the kernel compacts instead of reclaiming. Lower means more willing to compact. |

## Transparent huge pages

| Parameter | Value | Purpose |
|-----------|-------|---------|
| THP enabled | `madvise` | Only programs that request huge pages get them, which avoids compaction stalls. Set by the daemon. |

## MGLRU (multi-gen LRU)

| Parameter | Value | Purpose |
|-----------|-------|---------|
| `min_ttl_ms` | 1000 | Pages younger than one second are never reclaimed, which protects the working set from thrashing. Needs kernel 6.1 or later. Set by the daemon. |
