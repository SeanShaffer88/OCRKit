# Third-party notices

OCRKit itself is MIT © 2026 Sean Shaffer. When built with its engine,
it statically links the following components, fetched at build time
from their pinned, SHA-256-verified sources (see `pins.txt`) and never
committed to this repository. Their licenses apply to their code.

| Component | Version (pinned) | License | Upstream |
|---|---|---|---|
| Tesseract OCR | 5.5.3 | Apache-2.0 | https://github.com/tesseract-ocr/tesseract |
| Leptonica | 1.87.0 | Leptonica license (BSD-style, 2-clause) | https://github.com/DanBloomberg/leptonica |
| libpng | 1.6.59 | libpng license | https://github.com/pnggroup/libpng |
| zlib | 1.3.2 | zlib license | https://github.com/madler/zlib |
| tessdata_fast `eng.traineddata` (test fixture only, not shipped in the DLL) | pinned by SHA-256 in `pins.txt` | Apache-2.0 | https://github.com/tesseract-ocr/tessdata_fast |

License texts ship with each upstream source distribution; the fetch
step leaves them under `vendor/src-*/` in the build tree.
