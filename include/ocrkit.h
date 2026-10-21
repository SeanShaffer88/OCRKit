/* OCRKit.dll — offline OCR behind one clean C ABI.
 *
 * Create an engine from your own tessdata directory + language list,
 * then recognize images with calls that return JSON:
 *
 *     uint64_t eng = ok_engine_create("C:/tessdata", "eng");
 *     char *json = ok_recognize_file(eng, "scan.png");
 *     // {"text": "...", "confidence": 95, "lang": "eng"}
 *     ok_free_string(json);
 *     ok_engine_free(eng);
 *
 * The DLL never bundles language data: tessdata (<lang>.traineddata
 * files, e.g. from the tesseract-ocr tessdata_fast repository) is a
 * runtime input you supply. Everything runs locally; no image or text
 * ever leaves the machine.
 *
 * Strings are UTF-8. Returned char* are caller-owned: free with
 * ok_free_string(). Errors: thread-local ok_last_error() /
 * ok_last_error_code().
 *
 * Threading: distinct engines may recognize in parallel. One engine
 * must be used by one thread at a time — concurrent calls on the same
 * handle are serialized by the library, and freeing a handle while a
 * recognition is in flight on another thread is safe (the engine dies
 * when the call finishes; the handle is invalid immediately).
 *
 * Image formats (the vendored Leptonica build): PNG, BMP, PNM.
 * JPEG and TIFF are NOT in the v0.1 build; ok_recognize_file rejects
 * them as undecodable (OK_ERR_INVALID_ARG) and ok_recognize_png only
 * accepts PNG bytes. */
#ifndef OCRKIT_DLL_H
#define OCRKIT_DLL_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32) && defined(OK_BUILD)
#define OK_API __declspec(dllexport)
#elif defined(_WIN32)
#define OK_API __declspec(dllimport)
#else
#define OK_API
#endif

enum {
    OK_OK = 0,
    OK_ERR_INVALID_ARG = 1,
    OK_ERR_NOT_SUPPORTED = 2,
    OK_ERR_NOT_FOUND = 3,
    OK_ERR_ACCESS_DENIED = 4,
    OK_ERR_PLATFORM = 5,
    OK_ERR_INTERNAL = 6
};

/* "0.1.0" — static string, do not free. */
OK_API const char *ok_version(void);
OK_API uint32_t ok_version_major(void);
OK_API uint32_t ok_version_minor(void);
OK_API uint32_t ok_version_patch(void);

/* Create an OCR engine. tessdata_dir must contain <lang>.traineddata
 * for every language in the '+'-separated list ("eng", "eng+deu").
 * Returns an engine handle (never 0), or 0 on failure:
 * ERR_NOT_FOUND — the directory or a .traineddata file is missing
 *   (the message names the path);
 * ERR_NOT_SUPPORTED — this build has no engine compiled in;
 * ERR_PLATFORM — Tesseract rejected the language data. */
OK_API uint64_t ok_engine_create(const char *tessdata_dir, const char *lang);

/* Free an engine. OK_OK, or OK_ERR_NOT_FOUND for an unknown handle. */
OK_API int ok_engine_free(uint64_t engine);

/* Set a Tesseract variable, e.g. ("tessedit_pageseg_mode", "6").
 * OK_OK; OK_ERR_INVALID_ARG for an unknown variable name;
 * OK_ERR_NOT_FOUND for an unknown handle. */
OK_API int ok_set_variable(uint64_t engine, const char *name, const char *value);

/* Recognize an image file (PNG, BMP, PNM in this build). Returns a
 * caller-owned JSON string {"text", "confidence", "lang"}, or NULL
 * on failure: ERR_NOT_FOUND — no such file; ERR_INVALID_ARG — the
 * file cannot be decoded as a supported image. */
OK_API char *ok_recognize_file(uint64_t engine, const char *image_path);

/* Recognize PNG bytes in memory (bytes/len must stay valid for the
 * duration of the call). Same JSON as ok_recognize_file. Non-PNG
 * input is OK_ERR_INVALID_ARG. */
OK_API char *ok_recognize_png(uint64_t engine, const uint8_t *bytes, size_t len);

OK_API void ok_free_string(char *s);
OK_API const char *ok_last_error(void);
OK_API int ok_last_error_code(void);

#ifdef __cplusplus
}
#endif

#endif /* OCRKIT_DLL_H */
