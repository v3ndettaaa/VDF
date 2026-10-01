//! Locates (or builds) the vendored MuPDF static libraries and links them.
//!
//! Build strategy (MASTER_PLAN.md §15): the pinned submodule at
//! `thirdparty/mupdf` is compiled with its own make system. The resulting
//! `libmupdf.a` / `libmupdf-third.a` persist inside the submodule's build
//! directory, so cargo rebuilds are cheap and `cargo clean` does not trigger
//! a MuPDF rebuild. Override the source location with `MUPDF_DIR`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let default_mupdf = manifest
        .ancestors()
        .nth(2)
        .unwrap()
        .join("thirdparty/mupdf");
    let mupdf = PathBuf::from(
        env::var("MUPDF_DIR").unwrap_or_else(|_| default_mupdf.display().to_string()),
    );

    println!("cargo:rerun-if-env-changed=MUPDF_DIR");
    // The gitlink revision changes when the pinned MuPDF changes; the
    // Makefile is a cheap sentinel for "the source tree changed".
    println!(
        "cargo:rerun-if-changed={}",
        mupdf.join("Makefile").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        mupdf.join("include/mupdf/fitz/version.h").display()
    );
    println!("cargo:rerun-if-changed=shim.c");

    let libs_dir = mupdf.join("build/release");
    let libmupdf = libs_dir.join("libmupdf.a");
    let libthird = libs_dir.join("libmupdf-third.a");

    // Pass the pinned MuPDF version through to the FFI (fz_new_context_imp
    // requires the exact FZ_VERSION string, and it is a compile-time env).
    let version_h = std::fs::read_to_string(mupdf.join("include/mupdf/fitz/version.h"))
        .expect("read MuPDF version.h");
    let version = version_h
        .lines()
        .find_map(|l| l.strip_prefix("#define FZ_VERSION \"")?.strip_suffix('"'))
        .expect("FZ_VERSION in version.h");
    println!("cargo:rustc-env=MUPDF_FZ_VERSION={version}");

    if !libmupdf.exists() || !libthird.exists() {
        build_mupdf(&mupdf);
    }
    if !libmupdf.exists() || !libthird.exists() {
        panic!(
            "MuPDF static libraries not found after build at {}\n\
             Hint: git submodule update --init --recursive thirdparty/mupdf",
            libs_dir.display()
        );
    }

    // The shim is linked before the MuPDF archives so the linker resolves
    // its fz_* references from libmupdf.a (left-to-right resolution).
    cc::Build::new()
        .file("src/shim.c")
        .include(mupdf.join("include"))
        .flag_if_supported("-fPIC")
        .opt_level(2)
        .warnings(false)
        .compile("vdfshim");

    // Symbol sanity check: fail here with a clear message instead of a
    // confusing link error later.
    let probe = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "ar p {} mupdf/fitz/context.o 2>/dev/null | grep -c fz_new_context_imp",
            libmupdf.display()
        ))
        .status();
    if !matches!(probe, Ok(st) if st.success()) {
        println!(
            "cargo:warning=mupdf-sys: libmupdf.a symbol probe failed — the archive may be incomplete"
        );
    }

    println!("cargo:rustc-link-search=native={}", libs_dir.display());
    println!("cargo:rustc-link-lib=static=mupdf");
    println!("cargo:rustc-link-lib=static=mupdf-third");

    // System libraries MuPDF's thirdparty bundle ends up needing.
    if cfg!(target_os = "windows") {
        for lib in ["user32", "gdi32", "advapi32", "ole32", "shell32", "stdc++"] {
            println!("cargo:rustc-link-lib={lib}");
        }
    } else if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=c++");
    } else {
        for lib in ["stdc++", "m", "pthread", "dl"] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
}

fn build_mupdf(mupdf: &Path) {
    let jobs = env::var("NUM_JOBS").unwrap_or_else(|_| {
        std::thread::available_parallelism()
            .map(|n| n.get().to_string())
            .unwrap_or_else(|_| "4".to_string())
    });

    let on_windows = cfg!(target_os = "windows");
    let mut args: Vec<String> = vec![
        "-j".into(),
        jobs,
        "build=release".into(),
        "HAVE_X11=no".into(),
        "HAVE_GLUT=no".into(),
        // The docx/odt extract feature needs unzip at build time and is not
        // used by VDF; disable it for reproducible builds everywhere.
        "extract=no".into(),
        "libs".into(),
    ];
    if on_windows {
        // deskew.c uses SSE4.1 intrinsics unguarded; mingw gcc defaults to
        // baseline x86-64. SSE4.1 is 2008+ hardware — fine per plan targets.
        args.insert(3, "XCFLAGS=-msse4.1".into());
    } else {
        args.insert(3, "XCFLAGS=-fPIC".into());
    }

    let make_candidates: &[&str] = if on_windows {
        // MSYS2 provides `make`; some setups expose `mingw32-make`.
        &["make", "mingw32-make"]
    } else {
        &["make"]
    };
    let mut last_err = None;
    let mut status = None;
    for make in make_candidates {
        match Command::new(make).args(&args).current_dir(mupdf).status() {
            Ok(st) if st.success() => {
                status = Some(st);
                break;
            }
            Ok(st) => {
                last_err = Some(format!("{make} exited with {st}"));
            }
            Err(e) => {
                last_err = Some(format!("{make}: {e}"));
            }
        }
    }
    match status {
        Some(_) => {}
        None => panic!(
            "MuPDF make build failed ({}) — on Windows this requires MSYS2 make; \
             see MASTER_PLAN.md §15 build strategy",
            last_err.unwrap_or_default()
        ),
    }
}
