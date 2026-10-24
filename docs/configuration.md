# Configuration

elastic-swap works without any configuration file. The defaults live in `/usr/share/elastic-swap/swap-default.conf`, which also documents every option. Read it for the full list.

## Where settings come from

Later entries win:

1. `/usr/share/elastic-swap/swap-default.conf`: the shipped defaults. Do not edit it.
2. `/etc/elastic-swap/swap.conf`: your overrides. The package does not install this file, so create it.
3. Drop-in fragments named `*.conf` in `swap.conf.d/` under `/usr/lib/elastic-swap`, `/run/elastic-swap` and `/etc/elastic-swap`. Fragments are sorted by file name across all three directories and the last one wins. A fragment in `/etc` replaces one with the same name in `/run` or `/usr/lib`.

Values can use `${NCPU}` and `${RAM_SIZE}`, and simple integer arithmetic with `$(( expr ))`. The manual pages `elastic-swap(8)` and `elastic-swap.conf(5)` describe every key.

> [!IMPORTANT]
> `elastic-swap prepare` runs with full privileges and ignores `/run/elastic-swap/swap.conf.d`. Set `swapfile_path` and `swap_mode` in `/etc/elastic-swap` or `/usr/lib/elastic-swap`, or the swap directory will not be prepared where the daemon later looks for it.

## Examples

Change the swap mode (`zswap+swapfile`, `auto`, `zram+swapfile`, `zram`, `manual` or `disabled`):

```ini
swap_mode=zram+swapfile
```

Move the swap files:

```ini
swapfile_path=/mnt/data/swap
```

An administrator who moves `swapfile_path` must also let the service write there, with a drop-in that extends `ReadWritePaths=` in `elastic-swap.service`.

Resize the zram pool:

```ini
zram_size=200%                  # virtual disksize, as a percentage of RAM
zram_expand_threshold=90        # expand the pool above this utilization %
zram_contract_threshold=15      # contract the pool below this utilization %
```

Protect the working set harder:

```ini
mglru_min_ttl_ms=3000           # higher means more protection and less reclaim
```

Leave `vm.min_free_kbytes` and transparent huge pages alone:

```ini
kernel_tuning=no
```
