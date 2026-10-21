//! C ABI surface — everything declared in include/ocrkit.h.
//! Conventions (house standard): string inputs are const char* UTF-8;
//! `*mut c_char` results are caller-owned (free with ok_free_string);
//! failures return an ERR_* code (or null / 0) + thread-local last
//! error; nothing unwinds across the boundary.

// The exported functions ARE the raw-pointer C ABI declared in
// include/ocrkit.h; pointer validity is the documented contract there.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use crate::{engine, result_json, ERR_INTERNAL, ERR_INVALID_ARG, OK};

thread_local! {
    static LAST_ERROR: RefCell<(i32, CString)> =
        RefCell::new((OK, CString::new("").unwrap()));
}

fn set_error(code: i32, msg: &str) {
    let clean: String = msg.chars().filter(|&c| c != '\0').collect();
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = (
            code,
            CString::new(clean).unwrap_or_else(|_| CString::new("error").unwrap()),
        );
    });
}

fn clear_error() {
    set_error(OK, "");
}

fn guard<T, F: FnOnce() -> T>(f: F) -> std::thread::Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
}

fn input(ptr: *const c_char, what: &str) -> Option<String> {
    if ptr.is_null() {
        set_error(ERR_INVALID_ARG, &format!("{what} must not be null"));
        return None;
    }
    match unsafe { CStr::from_ptr(ptr) }.to_str() {
        Ok(s) => Some(s.to_string()),
        Err(_) => {
            set_error(ERR_INVALID_ARG, &format!("{what} is not valid UTF-8"));
            None
        }
    }
}

/// Move a Rust String across the boundary as a caller-owned C string.
fn string_out(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => {
            clear_error();
            c.into_raw()
        }
        Err(_) => {
            set_error(ERR_INTERNAL, "result contained an interior NUL byte");
            std::ptr::null_mut()
        }
    }
}

fn fail<T>(r: crate::CResult<T>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err((c, m)) => {
            set_error(c, &m);
            None
        }
    }
}

fn code(r: crate::CResult<()>) -> c_int {
    match r {
        Ok(()) => {
            clear_error();
            OK
        }
        Err((c, m)) => {
            set_error(c, &m);
            c
        }
    }
}

#[no_mangle]
pub extern "C" fn ok_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

#[no_mangle]
pub extern "C" fn ok_version_major() -> u32 {
    crate::VERSION_MAJOR
}

#[no_mangle]
pub extern "C" fn ok_version_minor() -> u32 {
    crate::VERSION_MINOR
}

#[no_mangle]
pub extern "C" fn ok_version_patch() -> u32 {
    crate::VERSION_PATCH
}

/// Create an OCR engine from a tessdata directory and a language list
/// ("eng", "eng+deu"). Returns an engine handle (never 0), or 0 on
/// failure: NOT_FOUND names the missing directory or .traineddata
/// file; NOT_SUPPORTED means this build has no engine compiled in.
#[no_mangle]
pub extern "C" fn ok_engine_create(tessdata_dir: *const c_char, lang: *const c_char) -> u64 {
    match guard(|| {
        let Some(dir) = input(tessdata_dir, "tessdata dir") else {
            return 0;
        };
        let Some(lang) = input(lang, "language") else {
            return 0;
        };
        match fail(engine::create_engine(&dir, &lang)) {
            Some(h) => {
                clear_error();
                h
            }
            None => 0,
        }
    }) {
        Ok(h) => h,
        Err(_) => {
            set_error(ERR_INTERNAL, "internal panic caught at the FFI boundary");
            0
        }
    }
}

/// Free an engine. Returns OK, or ERR_NOT_FOUND for a bad handle. If
/// another thread is mid-recognition on this handle, the engine is
/// destroyed when that call finishes; the handle is invalid from this
/// call on either way.
#[no_mangle]
pub extern "C" fn ok_engine_free(handle: u64) -> c_int {
    match guard(|| code(engine::free_engine(handle))) {
        Ok(c) => c,
        Err(_) => {
            set_error(ERR_INTERNAL, "internal panic caught at the FFI boundary");
            ERR_INTERNAL
        }
    }
}

/// Set a Tesseract variable on an engine, e.g.
/// ok_set_variable(h, "tessedit_pageseg_mode", "6").
/// Returns OK; ERR_INVALID_ARG for an unknown variable name;
/// ERR_NOT_FOUND for a bad handle.
#[no_mangle]
pub extern "C" fn ok_set_variable(handle: u64, name: *const c_char, value: *const c_char) -> c_int {
    match guard(|| {
        let Some(name) = input(name, "variable name") else {
            return ERR_INVALID_ARG;
        };
        let Some(value) = input(value, "variable value") else {
            return ERR_INVALID_ARG;
        };
        code(engine::set_variable(handle, &name, &value))
    }) {
        Ok(c) => c,
        Err(_) => {
            set_error(ERR_INTERNAL, "internal panic caught at the FFI boundary");
            ERR_INTERNAL
        }
    }
}

/// Recognize an image file. Returns a caller-owned JSON string (free
/// with ok_free_string), or null on failure:
///   {"text": "...", "confidence": 95, "lang": "eng"}
/// Formats are what the vendored Leptonica build decodes: PNG, BMP,
/// PNM (JPEG/TIFF are not in the v0.1 build — see README).
#[no_mangle]
pub extern "C" fn ok_recognize_file(handle: u64, image_path: *const c_char) -> *mut c_char {
    match guard(|| {
        let Some(path) = input(image_path, "image path") else {
            return std::ptr::null_mut();
        };
        match fail(engine::recognize_file(handle, &path)) {
            Some(r) => string_out(result_json(&r)),
            None => std::ptr::null_mut(),
        }
    }) {
        Ok(p) => p,
        Err(_) => {
            set_error(ERR_INTERNAL, "internal panic caught at the FFI boundary");
            std::ptr::null_mut()
        }
    }
}

/// Recognize PNG bytes in memory; same JSON as ok_recognize_file.
/// The PNG signature is validated before decoding; non-PNG input is
/// ERR_INVALID_ARG.
#[no_mangle]
pub extern "C" fn ok_recognize_png(handle: u64, bytes: *const u8, len: usize) -> *mut c_char {
    match guard(|| {
        if bytes.is_null() || len == 0 {
            set_error(ERR_INVALID_ARG, "image bytes must not be null or empty");
            return std::ptr::null_mut();
        }
        // SAFETY: caller contract — `bytes` points to `len` readable
        // bytes for the duration of the call (see include/ocrkit.h).
        let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
        match fail(engine::recognize_png(handle, slice)) {
            Some(r) => string_out(result_json(&r)),
            None => std::ptr::null_mut(),
        }
    }) {
        Ok(p) => p,
        Err(_) => {
            set_error(ERR_INTERNAL, "internal panic caught at the FFI boundary");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn ok_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = guard(|| unsafe {
            drop(CString::from_raw(ptr));
        });
    }
}

#[no_mangle]
pub extern "C" fn ok_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().1.as_ptr())
}

#[no_mangle]
pub extern "C" fn ok_last_error_code() -> c_int {
    LAST_ERROR.with(|e| e.borrow().0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ERR_NOT_FOUND;
    use std::ptr;

    fn cs(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    /// A temp dir holding a dummy eng.traineddata — enough to pass
    /// tessdata validation on every build flavor.
    #[cfg(not(ok_engine))]
    fn dummy_tessdata() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ok-ffi-tessdata-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("eng.traineddata"), b"dummy").unwrap();
        dir
    }

    #[test]
    fn ffi_contract() {
        assert!(!ok_version().is_null());
        assert_eq!(ok_version_major(), 0);
        assert_eq!(ok_version_minor(), 1);
        assert_eq!(ok_version_patch(), 0);

        // Null args -> INVALID_ARG, no handle.
        assert_eq!(ok_engine_create(ptr::null(), cs("eng").as_ptr()), 0);
        assert_eq!(ok_last_error_code(), ERR_INVALID_ARG);
        assert_eq!(ok_engine_create(cs(".").as_ptr(), ptr::null()), 0);
        assert_eq!(ok_last_error_code(), ERR_INVALID_ARG);

        // Missing tessdata dir -> NOT_FOUND on every build flavor.
        assert_eq!(
            ok_engine_create(cs("no/such/tessdata").as_ptr(), cs("eng").as_ptr()),
            0
        );
        assert_eq!(ok_last_error_code(), ERR_NOT_FOUND);
        assert!(unsafe { CStr::from_ptr(ok_last_error()) }
            .to_str()
            .unwrap()
            .contains("no/such/tessdata"));

        // Existing dir, missing language file -> NOT_FOUND.
        let empty = std::env::temp_dir().join(format!("ok-ffi-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(
            ok_engine_create(cs(empty.to_str().unwrap()).as_ptr(), cs("eng").as_ptr()),
            0
        );
        assert_eq!(ok_last_error_code(), ERR_NOT_FOUND);
        let _ = std::fs::remove_dir_all(&empty);

        // Bad handles -> NOT_FOUND everywhere.
        assert_eq!(ok_engine_free(424_242), ERR_NOT_FOUND as c_int);
        assert_eq!(
            ok_set_variable(
                424_242,
                cs("tessedit_pageseg_mode").as_ptr(),
                cs("6").as_ptr()
            ),
            ERR_NOT_FOUND as c_int
        );
        assert!(ok_recognize_file(424_242, cs("no/such/image.png").as_ptr()).is_null());
        assert_eq!(ok_last_error_code(), ERR_NOT_FOUND);

        // PNG entry: null bytes -> INVALID_ARG; garbage bytes with a
        // bad handle -> INVALID_ARG (signature checked before lookup).
        assert!(ok_recognize_png(424_242, ptr::null(), 8).is_null());
        assert_eq!(ok_last_error_code(), ERR_INVALID_ARG);
        let garbage = b"this is not a png at all";
        assert!(ok_recognize_png(424_242, garbage.as_ptr(), garbage.len()).is_null());
        assert_eq!(ok_last_error_code(), ERR_INVALID_ARG);

        // Valid signature + bad handle -> NOT_FOUND.
        let sig = crate::engine::PNG_SIGNATURE;
        assert!(ok_recognize_png(424_242, sig.as_ptr(), sig.len()).is_null());
        assert_eq!(ok_last_error_code(), ERR_NOT_FOUND);

        // Null variable name -> INVALID_ARG.
        assert_eq!(
            ok_set_variable(1, ptr::null(), cs("6").as_ptr()),
            ERR_INVALID_ARG as c_int
        );

        ok_free_string(ptr::null_mut());
    }

    #[cfg(not(ok_engine))]
    #[test]
    fn engine_absent_reports_not_supported() {
        use crate::ERR_NOT_SUPPORTED;
        // A valid-looking tessdata dir + language passes validation
        // and reaches the engine stub.
        let dir = dummy_tessdata();
        assert_eq!(
            ok_engine_create(cs(dir.to_str().unwrap()).as_ptr(), cs("eng").as_ptr()),
            0
        );
        assert_eq!(ok_last_error_code(), ERR_NOT_SUPPORTED);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
