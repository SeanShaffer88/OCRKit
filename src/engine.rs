//! Engine: engine-handle registry + the Tesseract/Leptonica bindings.
//!
//! Handles are opaque u64s into a process-wide registry (the
//! collection pattern): the registry lock is held only to clone the
//! per-engine `Arc<Mutex<Engine>>` out, never during recognition, so
//! distinct engines recognize in parallel while one engine is used by
//! at most one thread at a time (a caller sharing a handle across
//! threads gets serialized calls, never corruption — one TessBaseAPI
//! is NOT thread-safe).
//!
//! When the engine stack was not vendored at build time
//! (`cfg(ok_engine)` off), creation reports ERR_NOT_SUPPORTED after
//! the caller's inputs — including tessdata/language existence — have
//! been validated, so argument and missing-file errors keep their
//! precise codes on every build.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(not(ok_engine))]
use crate::ERR_NOT_SUPPORTED;
use crate::{CResult, OcrResult, ERR_INTERNAL, ERR_INVALID_ARG, ERR_NOT_FOUND};

/// First 8 bytes of every PNG file (ISO/IEC 15948).
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

pub fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= PNG_SIGNATURE.len() && bytes[..PNG_SIGNATURE.len()] == PNG_SIGNATURE
}

pub struct Engine {
    /// Language list the engine was created with ("eng", "eng+deu").
    lang: String,
    #[cfg(ok_engine)]
    raw: *mut sys::TessBaseAPI,
}

// The raw TessBaseAPI is owned by exactly this struct and only ever
// touched while holding the per-engine Mutex in the registry below.
#[cfg(ok_engine)]
unsafe impl Send for Engine {}

#[cfg(ok_engine)]
impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            sys::TessBaseAPIEnd(self.raw);
            sys::TessBaseAPIDelete(self.raw);
        }
    }
}

fn registry() -> &'static Mutex<HashMap<u64, Arc<Mutex<Engine>>>> {
    static REG: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Engine>>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn lookup(handle: u64) -> CResult<Arc<Mutex<Engine>>> {
    registry()
        .lock()
        .map_err(|_| (ERR_INTERNAL, "engine registry lock poisoned".into()))?
        .get(&handle)
        .cloned()
        .ok_or((ERR_NOT_FOUND, format!("no such engine handle: {handle}")))
}

/// Validate the tessdata dir + language list: the directory must exist
/// and hold a `<lang>.traineddata` for every `+`-separated language.
/// This runs on every build flavor so a typo'd path is
/// ERR_NOT_FOUND, not a misleading NOT_SUPPORTED.
fn validate_tessdata(tessdata_dir: &str, lang: &str) -> CResult<()> {
    if lang.is_empty() {
        return Err((ERR_INVALID_ARG, "language must not be empty".into()));
    }
    let dir = Path::new(tessdata_dir);
    if !dir.is_dir() {
        return Err((
            ERR_NOT_FOUND,
            format!("tessdata directory not found: '{tessdata_dir}'"),
        ));
    }
    for one in lang.split('+') {
        if one.is_empty() {
            return Err((
                ERR_INVALID_ARG,
                format!("language list contains an empty entry: '{lang}'"),
            ));
        }
        let trained = dir.join(format!("{one}.traineddata"));
        if !trained.is_file() {
            return Err((
                ERR_NOT_FOUND,
                format!("language data not found: '{}'", trained.display()),
            ));
        }
    }
    Ok(())
}

/// Create an engine and register it; returns its handle (never 0).
pub fn create_engine(tessdata_dir: &str, lang: &str) -> CResult<u64> {
    validate_tessdata(tessdata_dir, lang)?;
    let engine = load_engine(tessdata_dir, lang)?;
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    registry()
        .lock()
        .map_err(|_| (ERR_INTERNAL, "engine registry lock poisoned".into()))?
        .insert(handle, Arc::new(Mutex::new(engine)));
    Ok(handle)
}

/// Free an engine. Unknown handles are ERR_NOT_FOUND. If another
/// thread is mid-recognition on this handle, the engine is destroyed
/// when that call finishes; the handle is invalid from here on.
pub fn free_engine(handle: u64) -> CResult<()> {
    let removed = registry()
        .lock()
        .map_err(|_| (ERR_INTERNAL, "engine registry lock poisoned".into()))?
        .remove(&handle);
    match removed {
        Some(_) => Ok(()),
        None => Err((ERR_NOT_FOUND, format!("no such engine handle: {handle}"))),
    }
}

/// Set a Tesseract variable (e.g. "tessedit_pageseg_mode", "6") on an
/// engine. Unknown variable names are ERR_INVALID_ARG.
pub fn set_variable(handle: u64, name: &str, value: &str) -> CResult<()> {
    let arc = lookup(handle)?;
    let engine = arc
        .lock()
        .map_err(|_| (ERR_INTERNAL, "engine lock poisoned".into()))?;
    set_variable_inner(&engine, name, value)
}

/// Recognize an image file (PNG/BMP/PNM per the vendored Leptonica
/// build; see README). Missing file: ERR_NOT_FOUND. Undecodable file:
/// ERR_INVALID_ARG.
pub fn recognize_file(handle: u64, image_path: &str) -> CResult<OcrResult> {
    if !Path::new(image_path).is_file() {
        return Err((
            ERR_NOT_FOUND,
            format!("image file not found: '{image_path}'"),
        ));
    }
    let arc = lookup(handle)?;
    let engine = arc
        .lock()
        .map_err(|_| (ERR_INTERNAL, "engine lock poisoned".into()))?;
    recognize_file_inner(&engine, image_path)
}

/// Recognize PNG bytes in memory. The PNG signature is validated in
/// Rust before the bytes reach Leptonica; garbage is ERR_INVALID_ARG.
pub fn recognize_png(handle: u64, bytes: &[u8]) -> CResult<OcrResult> {
    if !is_png(bytes) {
        return Err((
            ERR_INVALID_ARG,
            "input is not a PNG image (bad signature)".into(),
        ));
    }
    let arc = lookup(handle)?;
    let engine = arc
        .lock()
        .map_err(|_| (ERR_INTERNAL, "engine lock poisoned".into()))?;
    recognize_png_inner(&engine, bytes)
}

// ---------------------------------------------------------------------------
// Engine-absent stubs: everything below reports NOT_SUPPORTED. Handle
// and file validation above has already run, so error codes stay
// precise on a fresh checkout too.
// ---------------------------------------------------------------------------

#[cfg(not(ok_engine))]
fn load_engine(tessdata_dir: &str, lang: &str) -> CResult<Engine> {
    let _ = (tessdata_dir, lang);
    Err((
        ERR_NOT_SUPPORTED,
        "this build has no OCR engine: the vendored Tesseract stack was absent at build time \
         (run fetch.sh / fetch.ps1 and rebuild)"
            .into(),
    ))
}

#[cfg(not(ok_engine))]
fn set_variable_inner(engine: &Engine, name: &str, value: &str) -> CResult<()> {
    let _ = &engine.lang;
    let _ = (name, value);
    Err((
        ERR_NOT_SUPPORTED,
        "this build has no OCR engine (vendored sources absent at build time)".into(),
    ))
}

#[cfg(not(ok_engine))]
fn recognize_file_inner(engine: &Engine, image_path: &str) -> CResult<OcrResult> {
    let _ = &engine.lang;
    let _ = image_path;
    Err((
        ERR_NOT_SUPPORTED,
        "this build has no OCR engine (vendored sources absent at build time)".into(),
    ))
}

#[cfg(not(ok_engine))]
fn recognize_png_inner(engine: &Engine, bytes: &[u8]) -> CResult<OcrResult> {
    let _ = &engine.lang;
    let _ = bytes;
    Err((
        ERR_NOT_SUPPORTED,
        "this build has no OCR engine (vendored sources absent at build time)".into(),
    ))
}

// ---------------------------------------------------------------------------
// Engine-present implementation.
// ---------------------------------------------------------------------------

#[cfg(ok_engine)]
fn load_engine(tessdata_dir: &str, lang: &str) -> CResult<Engine> {
    use std::ffi::CString;
    let c_dir = CString::new(tessdata_dir)
        .map_err(|_| (ERR_INVALID_ARG, "tessdata path contains a NUL byte".into()))?;
    let c_lang =
        CString::new(lang).map_err(|_| (ERR_INVALID_ARG, "language contains a NUL byte".into()))?;
    let raw = unsafe { sys::TessBaseAPICreate() };
    if raw.is_null() {
        return Err((ERR_INTERNAL, "TessBaseAPICreate returned null".into()));
    }
    let rc = unsafe { sys::TessBaseAPIInit3(raw, c_dir.as_ptr(), c_lang.as_ptr()) };
    if rc != 0 {
        unsafe {
            sys::TessBaseAPIDelete(raw);
        }
        return Err((
            crate::ERR_PLATFORM,
            format!(
                "Tesseract failed to initialize (datapath '{tessdata_dir}', language '{lang}')"
            ),
        ));
    }
    Ok(Engine {
        lang: lang.to_string(),
        raw,
    })
}

#[cfg(ok_engine)]
fn set_variable_inner(engine: &Engine, name: &str, value: &str) -> CResult<()> {
    use std::ffi::CString;
    let c_name = CString::new(name)
        .map_err(|_| (ERR_INVALID_ARG, "variable name contains a NUL byte".into()))?;
    let c_value = CString::new(value)
        .map_err(|_| (ERR_INVALID_ARG, "variable value contains a NUL byte".into()))?;
    let ok = unsafe { sys::TessBaseAPISetVariable(engine.raw, c_name.as_ptr(), c_value.as_ptr()) };
    if ok == 0 {
        return Err((
            ERR_INVALID_ARG,
            format!("unknown Tesseract variable: '{name}'"),
        ));
    }
    Ok(())
}

#[cfg(ok_engine)]
fn recognize_file_inner(engine: &Engine, image_path: &str) -> CResult<OcrResult> {
    use std::ffi::CString;
    let c_path = CString::new(image_path)
        .map_err(|_| (ERR_INVALID_ARG, "image path contains a NUL byte".into()))?;
    let pix = unsafe { sys::pixRead(c_path.as_ptr()) };
    if pix.is_null() {
        return Err((
            ERR_INVALID_ARG,
            format!(
                "could not decode image file: '{image_path}' (supported: PNG, BMP, PNM in this build)"
            ),
        ));
    }
    let result = run_on_pix(engine, pix);
    let mut pix = pix;
    unsafe { sys::pixDestroy(&mut pix) };
    result
}

#[cfg(ok_engine)]
fn recognize_png_inner(engine: &Engine, bytes: &[u8]) -> CResult<OcrResult> {
    let pix = unsafe { sys::pixReadMemPng(bytes.as_ptr(), bytes.len()) };
    if pix.is_null() {
        return Err((
            ERR_INVALID_ARG,
            "could not decode PNG bytes (corrupt or truncated PNG)".into(),
        ));
    }
    let result = run_on_pix(engine, pix);
    let mut pix = pix;
    unsafe { sys::pixDestroy(&mut pix) };
    result
}

/// SetImage2 does NOT take ownership of the Pix (per capi.h): the
/// caller keeps it and destroys it after recognition — the callers
/// above do exactly that, after this returns.
#[cfg(ok_engine)]
fn run_on_pix(engine: &Engine, pix: *mut sys::Pix) -> CResult<OcrResult> {
    use std::ffi::CStr;
    unsafe { sys::TessBaseAPISetImage2(engine.raw, pix) };
    let text_ptr = unsafe { sys::TessBaseAPIGetUTF8Text(engine.raw) };
    if text_ptr.is_null() {
        return Err((
            crate::ERR_PLATFORM,
            "Tesseract recognition failed (GetUTF8Text returned null)".into(),
        ));
    }
    let text = unsafe { CStr::from_ptr(text_ptr) }
        .to_string_lossy()
        .into_owned();
    let confidence = unsafe { sys::TessBaseAPIMeanTextConf(engine.raw) };
    unsafe { sys::TessDeleteText(text_ptr) };
    Ok(OcrResult {
        text,
        confidence: confidence.clamp(0, 100),
        lang: engine.lang.clone(),
    })
}

/// Hand-declared bindings for the vendored engine stack.
///
/// Sources of truth (from the pinned trees, vendor/): Tesseract 5.5.3
/// `include/tesseract/capi.h`, Leptonica 1.87.0 `src/allheaders.h`.
/// Signatures are transcribed from those headers — if a pin bump
/// changes one, this block must change with it.
///
/// BOOL in capi.h is a C int; GetUTF8Text's result is freed with
/// TessDeleteText (NOT free()); SetImage2 does not take Pix ownership.
#[cfg(ok_engine)]
mod sys {
    use std::ffi::c_void;
    use std::os::raw::{c_char, c_int};

    /// Leptonica's Pix — opaque; only ever handled as a pointer.
    #[repr(C)]
    pub struct Pix {
        _private: [u8; 0],
    }

    pub type TessBaseAPI = c_void;

    extern "C" {
        // Tesseract C API (capi.h).
        pub fn TessBaseAPICreate() -> *mut TessBaseAPI;
        pub fn TessBaseAPIDelete(handle: *mut TessBaseAPI);
        pub fn TessBaseAPIEnd(handle: *mut TessBaseAPI);
        pub fn TessBaseAPIInit3(
            handle: *mut TessBaseAPI,
            datapath: *const c_char,
            language: *const c_char,
        ) -> c_int;
        pub fn TessBaseAPISetVariable(
            handle: *mut TessBaseAPI,
            name: *const c_char,
            value: *const c_char,
        ) -> c_int;
        pub fn TessBaseAPISetImage2(handle: *mut TessBaseAPI, pix: *mut Pix);
        pub fn TessBaseAPIGetUTF8Text(handle: *mut TessBaseAPI) -> *mut c_char;
        pub fn TessBaseAPIMeanTextConf(handle: *mut TessBaseAPI) -> c_int;
        pub fn TessDeleteText(text: *const c_char);

        // Leptonica image IO (allheaders.h). pixDestroy takes PIX** —
        // it destroys the Pix and nulls the caller's pointer.
        pub fn pixRead(filename: *const c_char) -> *mut Pix;
        pub fn pixReadMemPng(filedata: *const u8, filesize: usize) -> *mut Pix;
        pub fn pixDestroy(ppix: *mut *mut Pix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_signature_validation() {
        assert!(is_png(&PNG_SIGNATURE));
        let mut with_data = PNG_SIGNATURE.to_vec();
        with_data.extend_from_slice(&[0u8; 64]);
        assert!(is_png(&with_data));
        assert!(!is_png(&[]));
        assert!(!is_png(&PNG_SIGNATURE[..7]));
        assert!(!is_png(b"GIF89a\x00\x00"));
        assert!(!is_png(b"\x89PNG\r\n\x1a\x0b")); // last byte wrong
    }

    #[test]
    fn tessdata_validation_errors() {
        // Missing dir -> NOT_FOUND naming the dir.
        let err = validate_tessdata("no/such/tessdata", "eng").unwrap_err();
        assert_eq!(err.0, ERR_NOT_FOUND);
        assert!(err.1.contains("no/such/tessdata"), "{}", err.1);

        // Existing dir without the traineddata -> NOT_FOUND naming it.
        let dir = std::env::temp_dir().join(format!("ok-tessdata-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = validate_tessdata(dir.to_str().unwrap(), "eng").unwrap_err();
        assert_eq!(err.0, ERR_NOT_FOUND);
        assert!(err.1.contains("eng.traineddata"), "{}", err.1);

        // With the file present, validation passes (empty list entry
        // in "eng+" is an INVALID_ARG, checked before file lookups).
        std::fs::write(dir.join("eng.traineddata"), b"dummy").unwrap();
        assert!(validate_tessdata(dir.to_str().unwrap(), "eng").is_ok());
        let err = validate_tessdata(dir.to_str().unwrap(), "eng+").unwrap_err();
        assert_eq!(err.0, ERR_INVALID_ARG);
        let err = validate_tessdata(dir.to_str().unwrap(), "").unwrap_err();
        assert_eq!(err.0, ERR_INVALID_ARG);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
