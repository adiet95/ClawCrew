# Ponytail: Lazy Senior Dev Mode (Khusus Tugas Kode)

> [!NOTE]
> Aturan ini **HANYA BERLAKU UNTUK TUGAS KODE** (menulis kode baru, refactoring, fixing bugs, mendesain fungsi/modul, dan pemilihan dependensi).
> Aturan ini **TIDAK BERLAKU** untuk obrolan non-coding (penjelasan teori, tanya-jawab umum, eksplorasi arsitektur, dokumentasi murni, atau terjemahan).

---

## Prinsip Utama
Terapkan pola pikir *lazy senior developer*: malas berarti efisien dan tepat sasaran, bukan ceroboh. Kode terbaik adalah kode yang tidak perlu ditulis.

Saat menangani kode, berhentilah pada anak tangga pertama yang bisa menyelesaikan masalah:

1. **YAGNI (You Aren't Gonna Need It)**: Apakah kode/fitur ini benar-benar perlu dibuat? Jika tidak, lewati.
2. **Reuse Codebase**: Apakah sudah ada utilitas, helper, atau pola serupa di codebase ini? Gunakan kembali, jangan menulis ulang fungsi yang serupa.
3. **Standard Library First**: Apakah library bawaan bahasa pemrograman (stdlib) sudah menyediakannya? Gunakan stdlib.
4. **Native Platform Feature**: Apakah fitur bawaan OS/browser/platform sudah mengatasinya? Gunakan fitur bawaan platform.
5. **Existing Dependency**: Apakah library/crate/package yang sudah terpasang di proyek bisa menyelesaikannya? Manfaatkan library yang sudah ada, jangan tambah dependensi baru.
6. **One-Liner**: Bisakah diselesaikan dalam 1 baris kode yang jelas? Jadikan satu baris.
7. **Minimal Working Implementation**: Hanya jika langkah 1–6 tidak mencukupi, tulis kode seminimal dan sesederhana mungkin yang bekerja dengan benar.

---

## Aturan Spesifik Coding
- **No Speculative Abstractions**: Jangan membuat generic class, trait, wrapper, atau factory spekulatif kecuali secara eksplisit diminta.
- **Zero Unneeded Dependencies**: Jangan menambahkan crate/package baru untuk utilitas kecil yang bisa diselesaikan dengan beberapa baris kode stdlib.
- **Root Cause, Bukan Gejala**: Untuk bug fix, perbaiki langsung di fungsi sumbernya (root cause), bukan menambal kondisi di setiap caller.
- **Deletion over Addition**: Utamakan pengurangan kode atau penyederhanaan daripada menambahkan lapisan kode baru.
- **Pertanyakan Kompleksitas**: Jika user meminta solusi yang terlalu rumit, sarankan alternatif yang jauh lebih ringkas.

---

## Yang Tidak Boleh Diabaikan (Non-Negotiable)
Efisiensi tidak boleh mengorbankan kualitas sistem:
- **Pahami Masalah Terlebih Dahulu**: Baca dan telusuri flow kode secara utuh sebelum menulis perbaikan; diff kecil di tempat yang salah adalah bug baru.
- **Keamanan & Trust Boundaries**: Validasi input pada boundary eksternal tetap wajib.
- **Error Handling**: Tetap tangani error agar tidak terjadi panic atau kehilangan data (*data loss*).
- **Verifikasi**: Setiap logika non-trivial harus memiliki setidaknya satu runnable test/check minimal yang memverifikasi kebenarannya.
