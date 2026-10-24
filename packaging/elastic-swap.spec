Name:           elastic-swap
Version:        0.2.0
Release:        1%{?dist}
Summary:        Dynamic swap management for zram, zswap and swap files
# MIT and BSD-3-Clause come from the statically linked crates (see LICENSE.dependencies).
License:        GPL-3.0-only AND MIT AND BSD-3-Clause
URL:            https://github.com/luminusOS/elastic-swap
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  systemd-rpm-macros

Requires:       btrfs-progs
Requires:       e2fsprogs
Requires:       kmod
Requires:       systemd
Requires:       util-linux

# Two swap daemons on one host would fight over the same swap devices.
Conflicts:      systemd-swap
Conflicts:      biglinux-systemd-swap

%description
elastic-swap grows and shrinks swap files on demand. Cold pages are compressed
in RAM first (zswap, or a zram pool) and spill to the swap files, which are
created as memory runs short and removed once it recovers.

It is a fork of biglinux-systemd-swap, a Rust rewrite of systemd-swap by
Timofey Titovets. The source of the exact version installed is the tarball
listed as Source0, and the upstream repository carries the full history.

%prep
%autosetup -n %{name}-%{version}
tar -xf %{SOURCE1}
mkdir -p .cargo
cat > .cargo/config.toml <<'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF

%build
cargo build --release --locked --offline

%install
%make_install prefix=%{_prefix} exec_prefix=%{_prefix} libdir=%{_prefix}/lib \
    datarootdir=%{_datadir} CARGO_FLAGS="--release --locked --offline"

%check
cargo test --release --locked --offline

%post
%systemd_post elastic-swap.service

%preun
%systemd_preun elastic-swap.service

# No restart on upgrade: stopping the daemon leaves its swap active and the next
# start adopts it, but a restart under memory pressure is not worth the risk.
%postun
%systemd_postun elastic-swap.service

%files
%license LICENSE NOTICE LICENSE.dependencies
%doc README.md ARCHITECTURE.md INVARIANTS.md AUTHORS
%{_bindir}/elastic-swap
%{_unitdir}/elastic-swap.service
%{_prefix}/lib/sysctl.d/60-elastic-swap.conf
%{_datadir}/elastic-swap/
%{_mandir}/man5/elastic-swap.conf.5*
%{_mandir}/man8/elastic-swap.8*

%changelog
* Sat Oct 24 2026 LuminusOS <noreply@luminusos.invalid> - 0.2.0-1
- Fork of biglinux-systemd-swap, renamed to elastic-swap
- Swap files default to /var/swap, created by "elastic-swap prepare"
- zswap+swapfile is the default mode
- Drop the pre-systemd-swap helper, which edited /etc/fstab
