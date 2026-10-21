//! OCRKit — pure logic core: error convention, JSON, result model.
//! The Tesseract/Leptonica engine lives in [`engine`], the C ABI in
//! [`ffi`].
//!
//! Design: create an OCR engine handle from a caller-supplied tessdata
//! directory + language list once, then recognize images (files or PNG
//! bytes) many times. Recognition is fully offline — image data never
//! leaves the machine.
//!
//! The engine stack (Tesseract, Leptonica, libpng, zlib) is vendored:
//! pinned in pins.txt, fetched + hash-verified by the fetch scripts,
//! never committed. When it is absent at build time the crate still
//! compiles and its engine entry points report [`ERR_NOT_SUPPORTED`].

pub mod engine;
pub mod ffi;

pub const VERSION: &str = "0.1.0";
pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;

pub const OK: i32 = 0;
pub const ERR_INVALID_ARG: i32 = 1;
pub const ERR_NOT_SUPPORTED: i32 = 2;
pub const ERR_NOT_FOUND: i32 = 3;
pub const ERR_ACCESS_DENIED: i32 = 4;
pub const ERR_PLATFORM: i32 = 5;
pub const ERR_INTERNAL: i32 = 6;

pub type CResult<T> = Result<T, (i32, String)>;

/// The outcome of one recognition.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrResult {
    /// Recognized text (UTF-8; line breaks as in the image layout).
    pub text: String,
    /// Tesseract's mean word confidence, 0–100.
    pub confidence: i32,
    /// The engine's language list, as passed at creation ("eng",
    /// "eng+deu", ...).
    pub lang: String,
}

/// Render a result in the JSON shape the recognize calls return:
/// `{"text": ..., "confidence": ..., "lang": ...}`.
pub fn result_json(r: &OcrResult) -> String {
    let mut out = String::with_capacity(r.text.len() + 64);
    out.push_str("{\"text\": \"");
    out.push_str(&json_escape(&r.text));
    out.push_str("\", \"confidence\": ");
    out.push_str(&r.confidence.to_string());
    out.push_str(", \"lang\": \"");
    out.push_str(&json_escape(&r.lang));
    out.push_str("\"}");
    out
}

pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> OcrResult {
        OcrResult {
            text: "The quick brown fox".into(),
            confidence: 95,
            lang: "eng".into(),
        }
    }

    #[test]
    fn json_shape() {
        assert_eq!(
            result_json(&sample()),
            "{\"text\": \"The quick brown fox\", \"confidence\": 95, \"lang\": \"eng\"}"
        );
    }

    #[test]
    fn json_escapes_quotes_backslash_and_controls() {
        let mut r = sample();
        r.text = "say \"hi\" \\ \u{1}\nnext\tline".into();
        let j = result_json(&r);
        assert!(
            j.contains("say \\\"hi\\\" \\\\ \\u0001\\nnext\\tline"),
            "{j}"
        );
    }

    #[test]
    fn json_empty_text_and_zero_confidence() {
        let r = OcrResult {
            text: String::new(),
            confidence: 0,
            lang: "eng+deu".into(),
        };
        assert_eq!(
            result_json(&r),
            "{\"text\": \"\", \"confidence\": 0, \"lang\": \"eng+deu\"}"
        );
    }
}
