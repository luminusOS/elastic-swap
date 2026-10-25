# elastic-swap

Swap that grows and shrinks with your memory use.

elastic-swap keeps cold memory pages compressed in RAM with zswap and writes the overflow to swap files. It creates those files when memory runs short and deletes them when it recovers, so you do not have to guess a swap size at install time.

It is a systemd service written in Rust, built for image-based systems such as Fedora bootc, where the root is read-only and `/etc/fstab` is off limits.

## What it does

- The default mode is `zswap+swapfile`. The kernel compresses cold pages in RAM, and the daemon manages the files they spill to.
- Swap files are 512 MB each, up to 28, in `/var/swap`. On btrfs that directory is a NOCOW subvolume.
- Other modes are a zram pool with disk overflow, zram only, or a choice made from the hardware.
- A restarted service adopts the swap that is already active. Stopping it leaves active swap in place.

See [how it works](docs/how-it-works.md) for the thresholds and the zram pool.

## Install

On Fedora 45 (x86_64), install the RPM from the [latest release](https://github.com/luminusOS/elastic-swap/releases/latest):

```bash
sudo dnf install https://github.com/luminusOS/elastic-swap/releases/download/v0.2.1/elastic-swap-0.2.1-1.fc45.x86_64.rpm
sudo systemctl enable --now elastic-swap
```

> [!NOTE]
> If `zram-generator-defaults` is installed, remove it. zswap and zram should not compress the same pages twice.

To build from source or make your own RPM, see [building and releasing](docs/building.md).

## Use

```bash
elastic-swap status                 # zram pool, swap files and memory breakdown
sudo elastic-swap autoconfig        # the configuration it would pick for this machine
sudo elastic-swap --dry-run start   # what start would do, without changing anything
journalctl -u elastic-swap -f       # follow the logs
```

## Configure

It works without a configuration file. To override a default, create `/etc/elastic-swap/swap.conf`:

```ini
swap_mode=zram+swapfile
swapfile_path=/mnt/data/swap
```

[Configuration](docs/configuration.md) lists where settings come from and shows more examples. `/usr/share/elastic-swap/swap-default.conf` documents every option, and [kernel tuning](docs/kernel-tuning.md) covers the sysctl values.

## Credits

elastic-swap is a fork of [biglinux-systemd-swap](https://github.com/biglinux/biglinux-systemd-swap), a Rust rewrite of [systemd-swap](https://github.com/Nefelim4ag/systemd-swap) by Timofey Titovets. It is not part of systemd. [NOTICE](NOTICE) and [AUTHORS](AUTHORS) have the details.
