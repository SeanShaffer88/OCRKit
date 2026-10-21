# OCRKit roadmap

## Next

- **JPEG input** via vendored libjpeg-turbo (pinned + hash-verified like
  the rest of the stack), then TIFF via libtiff if demand warrants.
  Scanned documents arrive as JPEG often enough that this is the top
  gap in v0.1.
- **Structured output**: hOCR and TSV modes, per-word bounding boxes +
  confidences as JSON — the ABI gains `ok_recognize_file_ex` with a
  format flag; v0.1's functions stay as-is.
- **Orientation & script detection** (Tesseract OSD) behind an explicit
  call, with the detected angle reported in the JSON.

## Later

- Additional tessdata guidance: a checked `fetch-tessdata` helper for
  common language sets (still user-supplied, still hash-pinned).
- DPI normalization helper for low-DPI inputs (Leptonica pixScaleToGray
  class preprocessing) once there's a measured accuracy case for it.
- Streaming region OCR (recognize a sub-rectangle without re-decoding
  the whole image).

## Non-goals

- No cloud OCR fallback — OCRKit is offline by design.
- No bundled language data in the DLL; tessdata stays a user asset.
