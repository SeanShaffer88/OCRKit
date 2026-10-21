//! Build integration for the vendored OCR engine stack:
//! zlib -> libpng -> Leptonica -> Tesseract, each built with CMake as
//! STATIC libraries and installed into one private prefix under
//! target/ocrkit-engine/prefix.
//!
//! Modes:
//! - Any vendor/src-* tree ABSENT: build the pure-Rust crate only. The
//!   FFI surface compiles and unit-tests; engine calls report
//!   OK_ERR_NOT_SUPPORTED. This keeps the host gate hermetic on a fresh
//!   checkout (no ~40 MB of sources needed to lint and test).
//! - All four trees PRESENT and host == target: configure + build the
//!   stack, link it plus the C++ runtime, and set `cfg(ok_engine)`.
//! - Cross builds (host != target) take the no-engine path; on Windows
//!   CI host == target, so the real MSVC link happens there.
//!
//! The engine root lives under the target dir (stable across cargo
//! invocations, unlike OUT_DIR) so repeat builds are incremental: once
//! the completion marker exists, build.rs only re-emits the recorded
//! link lines.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// (vendor subdir, anchor file cargo should watch inside it)
const TREES: [(&str, &str); 4] = [
    ("src-zlib", "zlib.h"),
    ("src-libpng", "png.h"),
    ("src-leptonica", "src/allheaders.h"),
    ("src-tesseract", "include/tesseract/capi.h"),
];

fn main() {
    println!("cargo:rustc-check-cfg=cfg(ok_engine)");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    for (dir, anchor) in TREES {
        let tree = vendor.join(dir);
        println!(
            "cargo:rerun-if-changed={}",
            tree.join("CMakeLists.txt").display()
        );
        println!("cargo:rerun-if-changed={}", tree.join(anchor).display());
    }

    let missing: Vec<&str> = TREES
        .iter()
        .filter(|(dir, _)| !vendor.join(dir).join("CMakeLists.txt").is_file())
        .map(|(dir, _)| *dir)
        .collect();
    if !missing.is_empty() {
        println!(
            "cargo:warning=ocrkit: vendored engine sources missing ({}) — building WITHOUT \
             the engine. Run fetch.sh / fetch.ps1, then rebuild. ok_engine_create will \
             report OK_ERR_NOT_SUPPORTED.",
            missing.join(", ")
        );
        return;
    }

    let host = env::var("HOST").expect("HOST");
    let target = env::var("TARGET").expect("TARGET");
    if host != target {
        println!(
            "cargo:warning=ocrkit: cross build ({host} -> {target}) — engine not built by \
             build.rs for foreign targets; building without the engine."
        );
        return;
    }

    println!("cargo:rustc-cfg=ok_engine");
    build_stack(&vendor, &target);
}

fn run(cmd: &mut Command, what: &str) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {what} (is CMake installed?): {e}"));
    if !status.success() {
        panic!("{what} failed with {status}");
    }
}

fn cmake_build(src: &Path, build_dir: &Path, prefix: &Path, extra_defs: &[String], what: &str) {
    fs::create_dir_all(build_dir).expect("create cmake build dir");
    if !build_dir.join("CMakeCache.txt").is_file() {
        let mut cfg = Command::new("cmake");
        cfg.arg("-S").arg(src).arg("-B").arg(build_dir);
        cfg.args([
            "-DCMAKE_BUILD_TYPE=Release".to_string(),
            format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()),
            format!("-DCMAKE_PREFIX_PATH={}", prefix.display()),
            // Everything installs into <prefix>/lib on every generator
            // (no lib64 / Debian multiarch detours).
            "-DCMAKE_INSTALL_LIBDIR=lib".to_string(),
            // Static libs get linked into a cdylib — need PIC.
            "-DCMAKE_POSITION_INDEPENDENT_CODE=ON".to_string(),
        ]);
        for d in extra_defs {
            cfg.arg(d);
        }
        run(&mut cfg, &format!("cmake configure ({what})"));
    }
    let mut build = Command::new("cmake");
    build.arg("--build").arg(build_dir).args([
        "--config",
        "Release",
        "--target",
        "install",
        "--parallel",
    ]);
    run(&mut build, &format!("cmake build+install ({what})"));
}

fn build_stack(vendor: &Path, target: &str) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out — walk up to
    // the target dir so the engine tree survives across cargo commands
    // (and across debug/release, at the cost of one Release stack).
    let root = out_dir
        .ancestors()
        .nth(4)
        .map(|t| t.join("ocrkit-engine"))
        .unwrap_or_else(|| out_dir.join("ocrkit-engine"));
    let prefix = root.join("prefix");
    let stamp = root.join("link-libs.txt");
    let marker = prefix.join(".ocrkit-engine-done");

    if marker.is_file() {
        let recorded = fs::read_to_string(&stamp).unwrap_or_else(|e| {
            panic!(
                "engine marker present but {} unreadable: {e}",
                stamp.display()
            )
        });
        let libs: Vec<(String, PathBuf)> = recorded
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(n, d)| (n.to_string(), PathBuf::from(d)))
            .collect();
        emit_link(&libs, target);
        return;
    }

    fs::create_dir_all(&prefix).expect("create engine prefix");
    let builds = root.join("build");
    let defs = |pairs: &[(&str, &str)]| -> Vec<String> {
        pairs.iter().map(|(k, v)| format!("-D{k}={v}")).collect()
    };

    // 1. zlib — static only.
    cmake_build(
        &vendor.join("src-zlib"),
        &builds.join("zlib"),
        &prefix,
        &defs(&[
            ("ZLIB_BUILD_TESTING", "OFF"),
            ("ZLIB_BUILD_SHARED", "OFF"),
            ("ZLIB_BUILD_STATIC", "ON"),
        ]),
        "zlib",
    );
    // 2. libpng — static only, against the prefix zlib.
    cmake_build(
        &vendor.join("src-libpng"),
        &builds.join("libpng"),
        &prefix,
        &defs(&[
            ("PNG_SHARED", "OFF"),
            ("PNG_STATIC", "ON"),
            ("PNG_TESTS", "OFF"),
            ("PNG_TOOLS", "OFF"),
        ]),
        "libpng",
    );
    // 3. Leptonica — static, PNG via prefix libpng/zlib, every other
    //    external format library OFF (BMP/PNM readers are built in).
    //    SW_BUILD defaults ON under WIN32 in 1.87 and would try to
    //    bootstrap a package manager; pin it OFF.
    cmake_build(
        &vendor.join("src-leptonica"),
        &builds.join("leptonica"),
        &prefix,
        &defs(&[
            ("SW_BUILD", "OFF"),
            ("BUILD_PROG", "OFF"),
            ("BUILD_SHARED_LIBS", "OFF"),
            ("ENABLE_GIF", "OFF"),
            ("ENABLE_JPEG", "OFF"),
            ("ENABLE_TIFF", "OFF"),
            ("ENABLE_WEBP", "OFF"),
            ("ENABLE_OPENJPEG", "OFF"),
        ]),
        "leptonica",
    );
    // 4. Tesseract — static library only: no training tools, no
    //    libarchive/libcurl/libtiff optional dependencies, no OpenMP
    //    (it would drag a libgomp/vcomp runtime into the DLL).
    cmake_build(
        &vendor.join("src-tesseract"),
        &builds.join("tesseract"),
        &prefix,
        &defs(&[
            ("BUILD_SHARED_LIBS", "OFF"),
            ("BUILD_TRAINING_TOOLS", "OFF"),
            ("BUILD_TESTS", "OFF"),
            ("DISABLE_ARCHIVE", "ON"),
            ("DISABLE_CURL", "ON"),
            ("DISABLE_TIFF", "ON"),
            ("OPENMP_BUILD", "OFF"),
        ]),
        "tesseract",
    );

    let mut libs: Vec<(i32, String, PathBuf)> = Vec::new();
    collect_libs(&prefix.join("lib"), &mut libs);
    for needed in ["tesseract", "lept", "png", "z"] {
        if !libs.iter().any(|(_, n, _)| n.contains(needed)) {
            panic!(
                "engine stack built but no '*{needed}*' static library found under {}",
                prefix.join("lib").display()
            );
        }
    }
    libs.sort();
    let recorded: Vec<(String, PathBuf)> =
        libs.into_iter().map(|(_, name, dir)| (name, dir)).collect();
    let stamp_text: String = recorded
        .iter()
        .map(|(n, d)| format!("{n}={}\n", d.display()))
        .collect();
    fs::write(&stamp, stamp_text).expect("write engine link stamp");
    fs::write(&marker, "ok\n").expect("write engine marker");
    emit_link(&recorded, target);
}

/// Find the stack's static libraries under the install prefix lib dir
/// (also scans one level down for generators that add a config
/// subdir). Classification is by file name: tesseract first, then
/// Leptonica, then libpng, then zlib — dependents before dependencies,
/// as static linkers require. Anything else is not ours; skip it.
fn collect_libs(dir: &Path, out: &mut Vec<(i32, String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_libs(&path, out);
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "a" && ext != "lib" {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let order = if stem.contains("tesseract") {
            0
        } else if stem.contains("lept") {
            1
        } else if stem.contains("png") {
            2
        } else if stem.contains('z')
            && (stem.contains("zlib") || stem.trim_start_matches("lib") == "z")
        {
            3
        } else {
            continue;
        };
        out.push((order, stem, dir.to_path_buf()));
    }
}

/// Emit the linker directives for the recorded libraries. The link
/// NAME rustc wants is the file stem; on Windows/MSVC the stem is the
/// literal file name (e.g. `libpng16.lib` -> `libpng16`), while the
/// Unix archives carry a `lib` prefix the linker strips itself
/// (`libpng16.a` -> `-lpng16`).
fn emit_link(libs: &[(String, PathBuf)], target: &str) {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for (_, dir) in libs {
        if !dirs.contains(dir) {
            dirs.push(dir.clone());
        }
    }
    for d in &dirs {
        println!("cargo:rustc-link-search=native={}", d.display());
    }
    let msvc = target.contains("windows-msvc");
    for (stem, _) in libs {
        let name = if msvc {
            stem.clone()
        } else {
            stem.strip_prefix("lib").unwrap_or(stem).to_string()
        };
        println!("cargo:rustc-link-lib=static={name}");
    }
    // C++ runtime + platform libs for the engine stack.
    if target.contains("windows-msvc") {
        // MSVC: the C++ runtime is part of the default link set.
    } else if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
    } else {
        // Linux and other ELF targets (incl. windows-gnu via MinGW).
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=m");
    }
}
