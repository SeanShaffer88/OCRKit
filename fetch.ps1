# Download the pinned upstream sources into vendor/ and verify SHA-256.
# Sources are NEVER committed; vendor/ is gitignored.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$pins = @{}
Get-Content pins.txt | Where-Object { $_ -match '=' -and $_ -notmatch '^#' } | ForEach-Object {
    $k, $v = $_ -split '=', 2; $pins[$k.Trim()] = $v.Trim()
}
New-Item -ItemType Directory -Force vendor | Out-Null

function Fetch([string]$Url, [string]$File, [string]$Want) {
    $path = Join-Path vendor $File
    if (-not (Test-Path $path)) {
        Write-Host "downloading $Url"
        Invoke-WebRequest -Uri $Url -OutFile $path
    }
    $got = (Get-FileHash $path -Algorithm SHA256).Hash.ToLower()
    if ($got -ne $Want) {
        throw "SHA-256 MISMATCH for $File`n  pinned: $Want`n  actual: $got"
    }
    Write-Host "verified $File ($got)"
}

function Extract([string]$Tarball, [string]$Inner, [string]$Dest) {
    Remove-Item -Recurse -Force "vendor/$Dest", vendor/tmp-extract -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force vendor/tmp-extract | Out-Null
    # Windows 10 1803+ ships tar.exe, which handles .tar.gz
    # (Expand-Archive cannot).
    tar -xzf "vendor/$Tarball" -C vendor/tmp-extract
    if ($LASTEXITCODE -ne 0) { throw "tar extraction failed for $Tarball" }
    Move-Item "vendor/tmp-extract/$Inner" "vendor/$Dest"
    Remove-Item -Recurse -Force vendor/tmp-extract
}

Fetch $pins['ZLIB_URL'] "zlib-$($pins['ZLIB_VERSION']).tar.gz" $pins['ZLIB_SHA256']
Fetch $pins['LIBPNG_URL'] "libpng-$($pins['LIBPNG_VERSION']).tar.gz" $pins['LIBPNG_SHA256']
Fetch $pins['LEPTONICA_URL'] "leptonica-$($pins['LEPTONICA_VERSION']).tar.gz" $pins['LEPTONICA_SHA256']
Fetch $pins['TESSERACT_URL'] "tesseract-$($pins['TESSERACT_VERSION']).tar.gz" $pins['TESSERACT_SHA256']
Fetch $pins['TESSDATA_ENG_URL'] $pins['TESSDATA_ENG_NAME'] $pins['TESSDATA_ENG_SHA256']

Extract "zlib-$($pins['ZLIB_VERSION']).tar.gz" $pins['ZLIB_DIR'] src-zlib
Extract "libpng-$($pins['LIBPNG_VERSION']).tar.gz" $pins['LIBPNG_DIR'] src-libpng
Extract "leptonica-$($pins['LEPTONICA_VERSION']).tar.gz" $pins['LEPTONICA_DIR'] src-leptonica
Extract "tesseract-$($pins['TESSERACT_VERSION']).tar.gz" $pins['TESSERACT_DIR'] src-tesseract

New-Item -ItemType Directory -Force vendor/tessdata | Out-Null
Copy-Item "vendor/$($pins['TESSDATA_ENG_NAME'])" vendor/tessdata/eng.traineddata -Force

# Tar extraction preserves release-date mtimes, which cargo's
# build-script fingerprint can mistake for "unchanged" if a build ran
# while the tree was absent. Stamp the anchor headers build.rs probes.
(Get-Item vendor/src-zlib/zlib.h).LastWriteTime = Get-Date
(Get-Item vendor/src-libpng/png.h).LastWriteTime = Get-Date
(Get-Item vendor/src-leptonica/src/allheaders.h).LastWriteTime = Get-Date
(Get-Item vendor/src-tesseract/include/tesseract/capi.h).LastWriteTime = Get-Date
Write-Host "sources ready under vendor/src-*; test tessdata at vendor/tessdata"
