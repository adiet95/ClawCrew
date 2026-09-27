# Rust & Cargo Performance Optimization Steering

> [!IMPORTANT]
> **TRIGGER RULE**: Aturan ini **WAJIB DIAKTIFKAN OTOMATIS** setiap kali terdapat perubahan kode Rust, pembuatan kode baru, modifikasi file `.rs`, `Cargo.toml`, `Cargo.lock`, atau saat menjalankan kompilasi, checking, linting, dan testing proyek berbasis Rust.

---

## 1. Fast Feedback Loop (Prinsip Pengecekan Cepat)

Di workspace multi-crate besar (seperti ClawCrew dengan 25+ crates), menjalankan pengecekan menyeluruh pada seluruh workspace akan memakan waktu 1–2 menit per iterasi.

### Aturan Wajib Pengecekan:
1. **Targeted Crate Check (Utamakan ini)**:
   * **JANGAN** menjalankan `cargo check --workspace` untuk iterasi perubahan kode biasa.
   * **SELALU** jalankan pengecekan spesifik pada crate yang sedang dimodifikasi:
     ```bash
     cargo check -p <crate-name>
     ```
     *Contoh: `cargo check -p clawcrew-runtime` atau `cargo check -p clawcrew-config` (waktu: 1–2 detik).*
2. **Targeted Clippy**:
   * Jalankan clippy per-crate:
     ```bash
     cargo clippy -p <crate-name> -- -D warnings
     ```
3. **Pemeriksaan Hanya Library**:
   * Jika hanya mengubah modul internal tanpa menyentuh bin/tests, gunakan:
     ```bash
     cargo check -p <crate-name> --lib
     ```

---

## 2. Fast Testing Loop dengan `cargo-nextest`

Di lingkungan Windows dan multi-core CPU, `cargo test` standar mengeksekusi test secara serial antar-crate dan lambat.

### Aturan Wajib Pengujian:
1. **Gunakan `cargo-nextest`**:
   * Sistem telah dilengkapi dengan `cargo-nextest`. Selalu gunakan `cargo nextest run` sebagai runner utama:
     ```bash
     cargo nextest run -p <crate-name>
     ```
2. **Uji Test Tertentu (Specific Filter)**:
   * Jangan jalankan seluruh test suite jika hanya memvalidasi satu fungsi:
     ```bash
     cargo nextest run -p <crate-name> -E 'test(<nama_fungsi_test>)'
     # atau
     cargo test -p <crate-name> <nama_fungsi_test>
     ```
3. **Compile-Only Check untuk Test**:
   * Jika hanya ingin memastikan test lolos kompilasi tanpa perlu menunggu eksekusinya:
     ```bash
     cargo test --no-run -p <crate-name>
     ```

---

## 3. Pencegahan Build Directory Lock (`target/` contention)

Pada sistem operasi Windows, background rust-analyzer atau proses cargo yang berjalan bersamaan dapat mengunci direktori `target/` dengan pesan:
`Blocking waiting for file lock on build directory`

### Solusi & Protokol:
1. **Pemisahan Direktori Target Background**:
   * Pastikan `.vscode/settings.json` menyertakan `"rust-analyzer.cargo.targetDir": true` agar proses background tidak bertabrakan dengan terminal/agent.
2. **Isolasi Target Agent Jika Diperlukan**:
   * Jika perintah CLI harus berjalan bersamaan tanpa menunggu lock root `target/`:
     ```powershell
     $env:CARGO_TARGET_DIR="target/agent"; cargo check -p <crate-name>
     ```
3. **Pembersihan Proses Yatim (*Orphan Processes*)**:
   * Jika terjadi deadlock lock yang tidak kunjung selesai, bersihkan proses background yang menggantung:
     ```powershell
     Stop-Process -Name cargo, cargo-clippy, rustc -Force -ErrorAction SilentlyContinue
     ```

---

## 4. Penggunaan Profile yang Tepat

1. **Development (`dev`)**:
   * Gunakan untuk edit harian, debugging, dan unit test lokal (`opt-level = 0`, `debug = 1`).
2. **Iterasi Cepat Release (`release-fast`)**:
   * Jika butuh menguji performa binary yang mendekati release tanpa menunggu *Fat LTO* yang lama:
     ```bash
     cargo build --profile release-fast
     ```
3. **Distribusi Final (`release` / `ci`)**:
   * Hanya digunakan saat validasi pre-PR akhir (`./dev/ci.sh all`) atau rilis produksi.
