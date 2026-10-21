#!/bin/sh
# Download the pinned upstream sources into vendor/ and verify SHA-256.
# Sources are NEVER committed; vendor/ is gitignored.
set -eu
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./pins.txt
mkdir -p vendor

sha_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

fetch() {
    url="$1"; file="$2"; want="$3"
    if [ ! -f "vendor/$file" ]; then
        echo "downloading $url"
        curl -fL "$url" -o "vendor/$file"
    fi
    got=$(sha_of "vendor/$file")
    if [ "$got" != "$want" ]; then
        echo "SHA-256 MISMATCH for $file" >&2
        echo "  pinned: $want" >&2
        echo "  actual: $got" >&2
        exit 1
    fi
    echo "verified $file ($got)"
}

# extract <tarball> <inner-dir> <dest-dir>: unpack so that
# vendor/<inner-dir> becomes vendor/<dest-dir>.
extract() {
    tb="$1"; inner="$2"; dest="$3"
    rm -rf "vendor/$dest" "vendor/tmp-extract"
    mkdir -p vendor/tmp-extract
    # --no-same-owner keeps non-root extraction from failing on tarball
    # uid/gid metadata; fall back for tars that reject the flag.
    if ! tar --no-same-owner -xzf "vendor/$tb" -C vendor/tmp-extract 2>/dev/null; then
        rm -rf vendor/tmp-extract
        mkdir -p vendor/tmp-extract
        tar -xzf "vendor/$tb" -C vendor/tmp-extract
    fi
    mv "vendor/tmp-extract/$inner" "vendor/$dest"
    rm -rf vendor/tmp-extract
}

fetch "$ZLIB_URL" "zlib-$ZLIB_VERSION.tar.gz" "$ZLIB_SHA256"
fetch "$LIBPNG_URL" "libpng-$LIBPNG_VERSION.tar.gz" "$LIBPNG_SHA256"
fetch "$LEPTONICA_URL" "leptonica-$LEPTONICA_VERSION.tar.gz" "$LEPTONICA_SHA256"
fetch "$TESSERACT_URL" "tesseract-$TESSERACT_VERSION.tar.gz" "$TESSERACT_SHA256"
fetch "$TESSDATA_ENG_URL" "$TESSDATA_ENG_NAME" "$TESSDATA_ENG_SHA256"

extract "zlib-$ZLIB_VERSION.tar.gz" "$ZLIB_DIR" src-zlib
extract "libpng-$LIBPNG_VERSION.tar.gz" "$LIBPNG_DIR" src-libpng
extract "leptonica-$LEPTONICA_VERSION.tar.gz" "$LEPTONICA_DIR" src-leptonica
extract "tesseract-$TESSERACT_VERSION.tar.gz" "$TESSERACT_DIR" src-tesseract

mkdir -p vendor/tessdata
cp "vendor/$TESSDATA_ENG_NAME" vendor/tessdata/eng.traineddata

# Tar extraction preserves release-date mtimes, which cargo's
# build-script fingerprint can mistake for "unchanged" if a build ran
# while the tree was absent. Stamp the anchor headers build.rs probes.
touch vendor/src-zlib/zlib.h
touch vendor/src-libpng/png.h
touch vendor/src-leptonica/src/allheaders.h
touch vendor/src-tesseract/include/tesseract/capi.h
echo "sources ready under vendor/src-*; test tessdata at vendor/tessdata"
