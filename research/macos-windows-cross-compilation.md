# Cross-Compiling Windows Binaries on macOS Using Rust

## Overview

Rust supports compilation for Windows targets from macOS, but out-of-the-box it fails during the linking stage due to missing Windows toolchains, runtime libraries (CRT), and platform SDKs.

## The Core Challenge: Linking

While `rustc` can emit object files for Windows targets (via LLVM code generation and `rustup target add <target>`), the final binary requires a platform linker and system libraries:

1. **MSVC Targets (`*-pc-windows-msvc`)**:
    - Requires Microsoft C Runtime (`ucrt.lib`, `vcruntime.lib`, `msvcrt.lib`) and Windows SDK import libraries (`kernel32.lib`, `advapi32.lib`, etc.).
    - Standard macOS installations do not include Microsoft SDK libraries.
    - `rust-lld` can act as the linker (`lld-link`), but cannot succeed without access to these library files.

2. **GNU Targets (`*-pc-windows-gnu`)**:
    - Requires MinGW-w64 headers, CRT libraries, and the `x86_64-w64-mingw32-gcc` toolchain.
    - macOS does not ship with MinGW by default.
    - Does not have full native support for `aarch64-pc-windows` via GCC.

---

## Solutions on macOS

### 1. `cargo-xwin` (Recommended for MSVC)

[`cargo-xwin`](https://github.com/rust-cross/cargo-xwin) allows compiling MSVC targets without a physical Windows machine:

- Leverages Rust's built-in `rust-lld`.
- Automatically fetches and unpacks official CRT and Windows SDK redistributable packages published by Microsoft.
- Supports both `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`.

```bash
brew install cargo-xwin
rustup target add x86_64-pc-windows-msvc
rustup target add aarch64-pc-windows-msvc

cargo xwin build --release --target x86_64-pc-windows-msvc
cargo xwin build --release --target aarch64-pc-windows-msvc
```

### 2. MinGW-w64 (GNU Targets)

For GNU-based targets, Homebrew provides the cross-compiler toolchain:

```bash
brew install mingw-w64
rustup target add x86_64-pc-windows-gnu

cargo build --release --target x86_64-pc-windows-gnu
```

_Limitations_:

- Produces binaries dependent on MinGW runtime conventions rather than native MSVC.
- Cannot easily target Windows ARM64 (`aarch64-pc-windows`).

### 3. Containerized Cross-Compilation (`cross`)

Using [`cross`](https://github.com/cross-rs/cross) to execute compilation inside container images (Docker/Podman):

- Relies on pre-built Linux containers with cross-toolchains installed.
- Requires running a container runtime (Docker Desktop/Colima) on macOS.

---

## Why CI Uses Separate Windows and macOS Runners

While macOS can cross-compile Windows binaries using `cargo-xwin`, utilizing GitHub Actions matrix runners (`windows-latest` + `macos-latest`) remains the optimal pattern:

1. **Native Test Execution**:
    - Windows binaries (`.exe`) cannot run natively on macOS.
    - Testing them requires Wine, which has limitations (especially on macOS Apple Silicon and for ARM64 Windows).
    - Running on `windows-latest` allows direct invocation of `cargo test`.

2. **No Extra SDK Download Overhead**:
    - `windows-latest` runners have MSVC, Windows SDK, and ARM64 cross-compilers pre-installed.
    - Cross-compilation on macOS requires pulling hundreds of megabytes of SDK data during cold builds.

3. **Parallel Concurrency**:
    - Matrix builds run in parallel on GitHub Actions runners, resulting in faster wall-clock execution times.
