# Deviations

## D1: The toolchain pin is named `nightly-2026-09-21`
- Kind: deviation
- Files: rust-toolchain.toml
- Pins: rust-toolchain.toml fb4a4c2e7f2b0acf
- Claims: -
- Why: M0 asks for "the dated nightly c's spike ran: 2026-09-20, rustc 1.100.0-nightly". That compiler is rustc `bba531001 2026-09-20`, which rustup publishes as `nightly-2026-09-21` (a nightly is dated the day after its commit). Pinning `nightly-2026-09-20` would pin the build before it. The pin also adds `rust-src`, which the pinned rust-analyzer needs to load the sysroot for C14.
- Evidence: rustup's channel manifest for the date, and the installed compiler.

```text
$ rustup toolchain install nightly-2026-09-21 --profile minimal --component clippy --component rust-analyzer --component rust-src
info: latest update on 2026-09-21 for version 1.100.0-nightly (bba531001 2026-09-20)
  nightly-2026-09-21-aarch64-apple-darwin installed - rustc 1.100.0-nightly (bba531001 2026-09-20)
$ grep date ~/.rustup/toolchains/nightly-aarch64-apple-darwin/lib/rustlib/multirust-channel-manifest.toml   # the toolchain c's spike ran
date = "2026-09-21"
```
