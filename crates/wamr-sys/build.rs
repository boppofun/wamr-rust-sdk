/*
 * Copyright (C) 2023 Liquid Reply GmbH. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
 */

extern crate bindgen;
extern crate cmake;

use cmake::Config;
use std::{env, path::Path, path::PathBuf};

const LLVM_LIBRARIES: &[&str] = &[
    // keep alphabet order
    "LLVMOrcJIT",
    "LLVMOrcShared",
    "LLVMOrcTargetProcess",
    "LLVMPasses",
    "LLVMProfileData",
    "LLVMRuntimeDyld",
    "LLVMScalarOpts",
    "LLVMSelectionDAG",
    "LLVMSymbolize",
    "LLVMTarget",
    "LLVMTextAPI",
    "LLVMTransformUtils",
    "LLVMVectorize",
    "LLVMX86AsmParser",
    "LLVMX86CodeGen",
    "LLVMX86Desc",
    "LLVMX86Disassembler",
    "LLVMX86Info",
    "LLVMXRay",
    "LLVMipo",
];

fn check_is_espidf() -> bool {
    let is_espidf = env::var("CARGO_FEATURE_ESP_IDF").is_ok()
        && env::var("CARGO_CFG_TARGET_OS").unwrap() == "espidf";

    if is_espidf
        && (env::var("WAMR_BUILD_PLATFORM").is_ok()
            || env::var("WAMR_SHARED_PLATFORM_CONFIG").is_ok())
    {
        panic!("ESP-IDF build cannot use custom platform build (WAMR_BUILD_PLATFORM) or shared platform config (WAMR_SHARED_PLATFORM_CONFIG)");
    }

    is_espidf
}

fn get_feature_flags() -> (String, String, String, String, String, String) {
    let enable_custom_section = if cfg!(feature = "custom-section") {
        "1"
    } else {
        "0"
    };
    let enable_dump_call_stack = if cfg!(feature = "dump-call-stack") {
        "1"
    } else {
        "0"
    };
    let enable_llvm_jit = if cfg!(feature = "llvmjit") { "1" } else { "0" };
    let enable_multi_module = if cfg!(feature = "multi-module") {
        "1"
    } else {
        "0"
    };
    let enable_name_section = if cfg!(feature = "name-section") {
        "1"
    } else {
        "0"
    };
    let disable_hw_bound_check = if cfg!(feature = "hw-bound-check") {
        "0"
    } else {
        "1"
    };

    (
        enable_custom_section.to_string(),
        enable_dump_call_stack.to_string(),
        enable_llvm_jit.to_string(),
        enable_multi_module.to_string(),
        enable_name_section.to_string(),
        disable_hw_bound_check.to_string(),
    )
}

fn link_llvm_libraries(llvm_cfg_path: &String, enable_llvm_jit: &String) {
    if enable_llvm_jit == "0" {
        return;
    }

    let llvm_cfg_path = PathBuf::from(llvm_cfg_path);
    assert!(llvm_cfg_path.exists());

    let llvm_lib_path = llvm_cfg_path.join("../../../lib").canonicalize().unwrap();
    assert!(llvm_lib_path.exists());

    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=m");
    println!("cargo:rustc-link-lib=dylib=rt");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=z");
    println!("cargo:libdir={}", llvm_lib_path.display());
    println!("cargo:rustc-link-search=native={}", llvm_lib_path.display());

    for &llvm_lib in LLVM_LIBRARIES {
        println!("cargo:rustc-link-lib=static={}", llvm_lib);
    }
}

fn setup_config(
    wamr_root: &PathBuf,
    feature_flags: (String, String, String, String, String, String),
) -> Config {
    let (
        enable_custom_section,
        enable_dump_call_stack,
        enable_llvm_jit,
        enable_multi_module,
        enable_name_section,
        disalbe_hw_bound_check,
    ) = feature_flags;

    let mut cfg = Config::new(wamr_root);
    cfg.define("WAMR_BUILD_AOT", "1")
        .define("WAMR_BUILD_INTERP", "1")
        .define("WAMR_BUILD_FAST_INTERP", "1")
        .define("WAMR_BUILD_JIT", &enable_llvm_jit)
        .define("WAMR_BUILD_BULK_MEMORY", "1")
        .define("WAMR_BUILD_REF_TYPES", "1")
        .define("WAMR_BUILD_SIMD", "1")
        .define("WAMR_BUILD_LIBC_WASI", "1")
        .define("WAMR_BUILD_LIBC_BUILTIN", "0")
        .define("WAMR_DISABLE_HW_BOUND_CHECK", &disalbe_hw_bound_check)
        .define("WAMR_BUILD_MULTI_MODULE", &enable_multi_module)
        .define("WAMR_BUILD_DUMP_CALL_STACK", &enable_dump_call_stack)
        .define("WAMR_BUILD_CUSTOM_NAME_SECTION", &enable_name_section)
        .define("WAMR_BUILD_LOAD_CUSTOM_SECTION", &enable_custom_section);

    // always assume non-empty strings for these environment variables

    if let Ok(platform_name) = env::var("WAMR_BUILD_PLATFORM") {
        cfg.define("WAMR_BUILD_PLATFORM", &platform_name);
    }

    if cfg!(windows) {
        cfg.define("WAMR_BUILD_LIBC_WASI", "0");
        cfg.define("WAMR_BUILD_LIBC_UVWASI", "1")
            .define("LIBUV_BUILD_SHARED", "OFF");
    }

    if let Ok(target_name) = env::var("WAMR_BUILD_TARGET") {
        cfg.define("WAMR_BUILD_TARGET", &target_name);
    }

    if let Ok(platform_config) = env::var("WAMR_SHARED_PLATFORM_CONFIG") {
        cfg.define("SHARED_PLATFORM_CONFIG", &platform_config);
    }

    if let Ok(llvm_cfg_path) = env::var("LLVM_LIB_CFG_PATH") {
        link_llvm_libraries(&llvm_cfg_path, &enable_llvm_jit);
        cfg.define("LLVM_DIR", &llvm_cfg_path);
    }

    // STDIN/STDOUT/STDERR redirect
    if let Ok(bh_vprintf) = env::var("WAMR_BH_VPRINTF") {
        cfg.define("WAMR_BH_VPRINTF", &bh_vprintf);
    }

    cfg
}

// Recursively looks for a static library file under `search_root` whose file
// name (case-insensitively) matches one of `file_names`, in priority order.
//
// This is needed because libuv/uvwasi are pulled in by WAMR's own CMake
// build (via FetchContent, see core/iwasm/libraries/libc-uvwasi/libc_uvwasi.cmake)
// rather than being vendored here, so the exact output directory depends on
// the CMake/generator version and can't be hardcoded reliably.
fn find_static_lib(search_root: &Path, file_names: &[&str]) -> Option<PathBuf> {
    let mut stack = vec![search_root.to_path_buf()];
    let mut candidates = Vec::new();

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if file_names
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(name))
                {
                    candidates.push(path);
                }
            }
        }
    }

    // If both Debug and Release outputs are present (e.g. stale build dir),
    // prefer the one matching the profile we're actually building.
    let config_dir = if cfg!(debug_assertions) {
        "Debug"
    } else {
        "Release"
    };
    candidates
        .iter()
        .find(|path| path.components().any(|c| c.as_os_str() == config_dir))
        .or_else(|| candidates.first())
        .cloned()
}

// Links against a static library found by `find_static_lib`, panicking with
// a descriptive error if the CMake build didn't produce it. Linking a
// missing/renamed library silently would otherwise surface as a much more
// confusing linker error far away from the actual cause.
fn link_static_lib_from_build(build_root: &Path, file_names: &[&str]) {
    let lib_path = find_static_lib(build_root, file_names).unwrap_or_else(|| {
        panic!(
            "Could not find a static library named one of {:?} anywhere under {}. \
             This is expected to be produced by WAMR's own CMake build (see \
             core/iwasm/libraries/libc-uvwasi/libc_uvwasi.cmake) -- check whether \
             the FetchContent target name or output directory layout changed.",
            file_names,
            build_root.display()
        )
    });

    let lib_name = lib_path
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("static library path has no file name");

    println!(
        "cargo:rustc-link-search=native={}",
        lib_path.parent().unwrap().display()
    );
    println!("cargo:rustc-link-lib=static={lib_name}");
}

fn build_wamr_libraries(wamr_root: &PathBuf) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let vmbuild_path = out_dir.join("vmbuild");

    let feature_flags = get_feature_flags();
    let mut cfg = setup_config(wamr_root, feature_flags);
    let dst = cfg.out_dir(vmbuild_path).build_target("vmlib").build();

    let mut iwasm_dir = dst.join("build");

    if cfg!(windows) {
        // libuv/uvwasi are built as static libraries as part of the same
        // CMake build (WAMR always links libuv's static `uv_a` target, never
        // the shared `uv` target -- see libc_uvwasi.cmake). Find whatever
        // CMake actually produced instead of assuming a fixed path, and
        // verify it's a real static archive, not a DLL import library.
        let build_root = dst.join("build");
        link_static_lib_from_build(&build_root, &["uv_a.lib", "uv.lib", "libuv.lib"]);
        link_static_lib_from_build(&build_root, &["uvwasi_a.lib", "uvwasi.lib"]);

        if cfg!(debug_assertions) {
            iwasm_dir.push("Debug");
        } else {
            iwasm_dir.push("Release");
        }
    }

    println!("cargo:rustc-link-search=native={}", iwasm_dir.display());
    println!("cargo:rustc-link-lib=static=iwasm");
}

// Commented out because this did not build correctly in the context of
// rust-analyzer, which would break autocompletion and error checking
// Can be compiled separately.
/*fn build_wamrc(wamr_root: &Path) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let wamrc_build_path = out_dir.join("wamrcbuild");

    let wamr_compiler_path = wamr_root.join("wamr-compiler");
    assert!(wamr_compiler_path.exists());

    Config::new(&wamr_compiler_path)
        .out_dir(wamrc_build_path)
        .define("WAMR_BUILD_WITH_CUSTOM_LLVM", "1")
        .define(
            "LLVM_DIR",
            env::var("LLVM_LIB_CFG_PATH")
                .expect("LLVM_LIB_CFG_PATH isn't specified in config.toml"),
        )
        .build();
}*/

fn generate_bindings(wamr_root: &Path) {
    let wamr_header = wamr_root.join("core/iwasm/include/wasm_export.h");
    assert!(wamr_header.exists());

    let mut builder = bindgen::Builder::default()
        .ctypes_prefix("::core::ffi")
        .use_core()
        .header(wamr_header.into_os_string().into_string().unwrap())
        .derive_default(true);

    if check_is_espidf() {
        // Use the correct gcc to avoid system Clang which does not know xtensa by default
        let gcc = env::var("CC_xtensa_esp32s3_espidf")
            .unwrap_or_else(|_| "xtensa-esp-elf-gcc".to_string());

        // Find out the matching include folder and configure the builder accordingly
        if let Ok(output) = std::process::Command::new(&gcc)
            .arg("--print-sysroot")
            .output()
        {
            let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sysroot.is_empty() {
                builder = builder.clang_arg(format!("-I{sysroot}/include"));
            }
        }
    }

    let bindings = builder.generate().expect("Unable to generate bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_ESP_IDF");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=WAMR_BUILD_PLATFORM");
    println!("cargo:rerun-if-env-changed=WAMR_SHARED_PLATFORM_CONFIG");
    println!("cargo:rerun-if-env-changed=LLVM_LIB_CFG_PATH");
    println!("cargo:rerun-if-env-changed=WAMR_BH_VPRINTF");

    let wamr_root = env::current_dir().unwrap();
    let wamr_root = wamr_root.join("wasm-micro-runtime");
    assert!(wamr_root.exists());

    if !check_is_espidf() {
        // because the ESP-IDF build procedure differs from the regular one
        // (build internally by esp-idf-sys),
        build_wamr_libraries(&wamr_root);
        // Avoid building wamrc, which depends on an old version of LLVM,
        // and breaks Rust-analyzer features for this crate if an up-to-date
        // LLVM version is installed on the system.
        //
        //build_wamrc(&wamr_root);
    }

    generate_bindings(&wamr_root);
}
