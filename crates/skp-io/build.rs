use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=SKETCHUP_SDK_DIR");
    if env::var_os("CARGO_FEATURE_SDK").is_none() {
        return;
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        panic!("the sdk feature is only wired for the Windows SketchUp SDK layout");
    }
    let sdk = PathBuf::from(env::var("SKETCHUP_SDK_DIR").unwrap_or_else(|_| {
        panic!("the sdk feature needs SKETCHUP_SDK_DIR set to the unpacked SketchUp SDK")
    }));
    let lib_dir = [
        sdk.clone(),
        sdk.join("binaries").join("sketchup").join("x64"),
    ]
    .into_iter()
    .find(|dir| dir.join("SketchUpAPI.lib").is_file())
    .unwrap_or_else(|| panic!("no SketchUpAPI.lib under {}", sdk.display()));

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=SketchUpAPI");
    println!(
        "cargo:rerun-if-changed={}",
        lib_dir.join("SketchUpAPI.lib").display()
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR sits three levels under the profile directory");
    for target in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
        copy_dlls(&lib_dir, &target);
    }
}

fn copy_dlls(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("profile directory is writable");
    let entries = fs::read_dir(from).expect("SDK library directory is readable");
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dll = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dll"));
        if is_dll {
            let dest = to.join(entry.file_name());
            fs::copy(&path, &dest)
                .unwrap_or_else(|e| panic!("copying {} failed: {e}", path.display()));
        }
    }
}
