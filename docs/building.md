# Building and releasing

## Build from source

You need Rust 1.93 or later, plus `util-linux`, `btrfs-progs` and `e2fsprogs` (for `chattr`) at run time.

```bash
cargo build --release
sudo make install
sudo systemctl enable --now elastic-swap
```

`make install` honors `DESTDIR` and `prefix`. `make uninstall` refuses to run while the service is active.

## Build the RPM

```bash
packaging/build-rpm.sh
```

The script needs `rpm-build`, `cargo`, `rust` and `git`. It archives the files git tracks, vendors the crates, builds offline and runs the tests in `%check`. Everything lands in `dist/`: the binary RPM, the source RPM, the two source tarballs and a `SHA256SUMS` file. The source tarball includes `LICENSE.dependencies`, generated from the vendored crates.

## Publish a release

Push a tag named `v<version>`, where the version matches `Cargo.toml`. The `release` workflow builds the RPM in a Fedora 45 container and creates the GitHub release with the binary RPM, the source RPM, both tarballs and `SHA256SUMS`.

The same workflow runs without publishing on pushes to `master` that touch the sources or the packaging.

Publish the source RPM and the tarballs together with the binary. GPLv3 section 6 requires the corresponding source to be offered where the binary is.

A rebuilt RPM has a different checksum. Anything that pins the digest of a release asset, such as an image build, must use the digest of the file the workflow published.

## Develop

[AGENTS.md](../AGENTS.md) lists the build, test and lint commands and the module map. [ARCHITECTURE.md](../ARCHITECTURE.md) covers the state machine and the external processes. [INVARIANTS.md](../INVARIANTS.md) is the contract for memory safety, subprocess arguments, the fstab rule, hardening, the config parser and shutdown.

The quick gate before a commit:

```bash
cargo fmt --all -- --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings && \
cargo test --all-features --workspace
```

CI mirrors it in `.github/workflows/ci.yml`. Behavior under memory pressure is checked by hand with `tests/stress/`, on a test machine and never on a workstation someone is using.
