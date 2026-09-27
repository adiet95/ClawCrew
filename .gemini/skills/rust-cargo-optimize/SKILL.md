---
name: rust-cargo-optimize
description: Auto-activates on any Rust codebase or Rust file changes (.rs, Cargo.toml). Enforces high-speed Cargo compilation, targeted checking, fast linking with rust-lld/mold, nextest test execution, and optimized dev profiles.
---

# Rust & Cargo Performance Optimization Skill

This skill governs compilation, checking, and testing workflows for any Rust project. It eliminates compile-time bottlenecks, prevents build-lock contentions on Windows/Linux, and dramatically reduces iteration latency.

## When to Use
- **Automatically activated** when editing, compiling, checking, or testing any Rust project or `.rs` file.
- When running `cargo check`, `cargo clippy`, `cargo test`, `cargo build`, or benchmark suites.
- When encountering `Blocking waiting for file lock on build directory` or slow link times.

---

## 1. Targeted Checking (Fast Inner Loop)

In large workspaces with multiple crates, never run whole-workspace checks for routine edits.

```powershell
# Fast crate check (1-2 seconds)
cargo check -p <crate-name>

# Crate library only (bypasses bin/tests)
cargo check -p <crate-name> --lib

# Targeted linting
cargo clippy -p <crate-name> -- -D warnings
```

---

## 2. Ultra-Fast Parallel Testing with `nextest`

Standard `cargo test` runs crate test binaries serially and generates redundant output. Use `cargo-nextest` for instant, isolated, parallel execution.

```powershell
# Run tests for a specific crate in parallel
cargo nextest run -p <crate-name>

# Run a specific test function
cargo nextest run -p <crate-name> -E 'test(<test_function_name>)'

# Check that tests compile without waiting for them to run
cargo test --no-run -p <crate-name>
```

---

## 3. Resolving Build Directory Locks (`target/` Contention)

If another process (like rust-analyzer or a background terminal) is compiling:

1. **Check who owns the lock**:
   ```powershell
   Get-CimInstance Win32_Process -Filter "Name LIKE '%cargo%' OR Name LIKE '%rustc%'" | Select-Object ProcessId, CommandLine
   ```
2. **Clear orphaned processes**:
   ```powershell
   Stop-Process -Name cargo, cargo-clippy, rustc -Force -ErrorAction SilentlyContinue
   ```
3. **Isolate Target Directory (Zero Collision)**:
   ```powershell
   $env:CARGO_TARGET_DIR="target/agent"; cargo check -p <crate-name>
   ```

---

## 4. Recommended `Cargo.toml` Profile Tuning

Ensure the workspace `Cargo.toml` contains these dev optimizations:

```toml
[profile.dev]
incremental = true
opt-level = 0
debug = 1               # Cuts debuginfo generation & link time by 50% while preserving backtraces
split-debuginfo = "unpacked"

[profile.dev.package."*"]
opt-level = 2           # Pre-optimizes third-party dependencies so runtime execution is fast
```

---

## 5. Fast Linkers Configuration (`.cargo/config.toml`)

- **Windows (MSVC)**: Use the bundled `rust-lld.exe` or fast link args.
- **Linux**: Use `mold` via `-C link-arg=-fuse-ld=mold`.
