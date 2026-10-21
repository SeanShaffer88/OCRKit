# OCRKit — agent notes

- Zero Rust dependencies is a design rule. The engine stack (zlib,
  libpng, Leptonica, Tesseract) is vendored C/C++: pinned in pins.txt,
  hash-verified by fetch.sh/fetch.ps1, built static by build.rs via
  CMake, never committed. Do not add Rust binding crates for Tesseract
  or Leptonica; the hand-declared C API subset in src/engine.rs is
  checked against the pinned 5.5.3 capi.h.
- Memory ownership across the engine boundary is load-bearing:
  GetUTF8Text results are freed with TessDeleteText (never free()),
  Pix objects with pixDestroy, API handles with TessBaseAPIDelete.
  Construction must leave every failure path leak-free.
- Two build flavors are both first-class: engine-absent (fresh
  checkout — FFI compiles, gate green, engine entry points report
  ERR_NOT_SUPPORTED after argument checks) and engine-present. Never
  make the host gate depend on the vendored sources or fixtures.
- Bumping any pin = change version + URL + SHA-256 together in
  pins.txt, re-check the C API declarations against the new headers,
  and re-run the e2e proof, in one commit.
- Claims discipline: README numbers are the e2e's measured results on
  the pinned stack (fixture text exact, confidence 96). The e2e
  asserts the exact fixture text + a confidence floor — keep it that
  way; don't broaden claims beyond the fixture.
- The `Error in pixReadMemTiff` stderr lines during engine init are
  known cosmetic noise from the TIFF-less Leptonica build (Tesseract
  probing for its internal font pixa). They do not affect output; if a
  future Leptonica config enables TIFF they should disappear.
- Gate: WinDevKit tools/gate.sh (fmt + clippy BOTH targets + full
  tests). Check/clippy do not link the engine — CI's fetch + real
  build + labeled e2e step are the link + native-OCR gates. Windows
  engine build/OCR is UNVERIFIED until that CI step runs green; do
  not claim otherwise.
