# OCRKit

Offline OCR behind one clean C ABI. DLL #13 in the Windows DLL Collection.

OCRKit wraps a vendored [Tesseract](https://github.com/tesseract-ocr/tesseract) engine
(with its Leptonica/zlib/libpng support stack) in a zero-Rust-dependency `cdylib`
with the collection's house FFI style: opaque `u64` handles, thread-local
last-error, caller-owned result strings, and JSON output. Everything runs
locally — no network, no cloud OCR service.

## ABI (`include/ocrkit.h`, prefix `ok_`)

| Function | Purpose |
|---|---|
| `ok_engine_create(tessdata_dir, lang)` | Create an engine for a language set (e.g. `"eng"`, `"eng+deu"`). `tessdata_dir` is a directory containing the `.traineddata` files. |
| `ok_engine_free(handle)` | Destroy an engine. |
| `ok_set_variable(handle, name, value)` | Set a Tesseract variable (e.g. `tessedit_pageseg_mode`). |
| `ok_recognize_file(handle, image_path)` | OCR an image file → JSON (below). |
| `ok_recognize_png(handle, bytes, len)` | OCR PNG bytes in memory → JSON. The PNG signature is validated before the bytes reach the engine. |
| `ok_version*`, `ok_last_error*`, `ok_free_string` | Version info, thread-local error reporting, string disposal. |

Recognition returns:

```json
{"text": "The quick brown fox jumps over the lazy dog 12345\n", "confidence": 96, "lang": "eng"}
```

`confidence` is Tesseract's mean word confidence (0–100). Failures return
null/0 plus a code from `ok_last_error_code()` (0–6, the collection-wide
convention) and a specific message from `ok_last_error()` — a missing
tessdata directory or language file is `ERR_NOT_FOUND` naming the path,
a corrupt/unsupported image is `ERR_INVALID_ARG`, and an engine-less
build (see below) is `ERR_NOT_SUPPORTED`.

One handle owns one Tesseract instance and is **not** thread-safe; use
distinct handles in parallel (that works).

## Building

```sh
./fetch.sh        # or fetch.ps1 on Windows — downloads + SHA-256-verifies the pinned sources
cargo build --release
```

Nothing upstream is committed to this repo. `fetch` downloads the pinned
tarballs listed in `pins.txt` (zlib, libpng, Leptonica, Tesseract — each
verified against its recorded SHA-256) into `vendor/`, and `build.rs`
builds them as static libraries with CMake and links them into the DLL.
Without `vendor/`, the crate still compiles and passes its gate: the FFI
surface is complete and engine calls report `ERR_NOT_SUPPORTED` — the
two build flavors are both first-class.

**Language data is user-supplied.** Tesseract's `tessdata` files are not
bundled with the DLL; point `ok_engine_create` at a directory holding the
languages you need (from the `tessdata_fast` or `tessdata` projects).
The pinned `eng.traineddata` used by the test suite is a test fixture
only.

## Image formats (v0.1)

PNG (file and in-memory), BMP, and PNM via Leptonica's readers. The
vendored Leptonica is built without its optional format libraries, so
**JPEG, TIFF, GIF, and WebP are not accepted in v0.1** — callers get a
clear `ERR_INVALID_ARG`. JPEG support is first on the roadmap.

One cosmetic note: with this TIFF-less Leptonica build, Tesseract prints
harmless `Error in pixReadMemTiff` lines to stderr while initializing
its internal font helpers. Recognition output is unaffected (the e2e
fixture reads at confidence 96).

## Verification

- Gate (WinDevKit `tools/gate.sh`): fmt, clippy on host **and**
  `x86_64-pc-windows-msvc`, full test suite — green in engine-absent
  mode; the full suite is green with the engine built.
- End-to-end proof (local, Linux): the committed fixture
  `tests/fixtures/hello-ocr.png` (text rendered with PIL/DejaVu Sans —
  regenerate with `tools/make_fixture.py`) is recognized through both
  the file and in-memory PNG paths as
  `"The quick brown fox jumps over the lazy dog 12345"` with mean
  confidence **96**, using the pinned `eng.traineddata`.
- **Unverified:** the Windows/MSVC build of the engine stack and native
  Windows OCR have not run yet. CI (root `ci.yml`, copied to
  `.github/workflows/ci.yml`) runs the fetch, the gate, and this same
  e2e on `windows-latest`; that run is the Windows gate.

## v0.1 limits

- Plain text + mean confidence only — no hOCR/TSV/word-box output yet.
- One language set per engine handle.
- No orientation/script detection.
- JPEG/TIFF/GIF/WebP input not accepted (see formats above).

See `ROADMAP.md` for what's next and `THIRD-PARTY-NOTICES.md` for the
vendored components and their licenses.

## License

MIT © 2026 Sean Shaffer. The vendored engine components carry their own
licenses (Apache-2.0 / zlib / libpng) — see `THIRD-PARTY-NOTICES.md`.
Their sources are fetched at build time, never committed here.
