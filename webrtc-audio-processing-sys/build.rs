use anyhow::{bail, Context, Error, Result};
use std::path::PathBuf;
use std::env;

fn out_dir() -> PathBuf {
    std::env::var("OUT_DIR").expect("OUT_DIR environment var not set.").into()
}

#[cfg(not(feature = "bundled"))]
mod webrtc {
    use super::*;

    const LIB_NAME: &str = "webrtc-audio-processing-2";
    const LIB_MIN_VERSION: &str = "2.0";

    pub(super) fn get_build_paths() -> Result<(Vec<PathBuf>, Vec<PathBuf>), Error> {
        let (pkgconfig_include_path, pkgconfig_lib_path) = find_pkgconfig_paths()?;

        let include_path = std::env::var("WEBRTC_AUDIO_PROCESSING_INCLUDE")
            .ok()
            .map(|x| x.into())
            .or(pkgconfig_include_path);
        let lib_path = std::env::var("WEBRTC_AUDIO_PROCESSING_LIB")
            .ok()
            .map(|x| x.into())
            .or(pkgconfig_lib_path);

        match (include_path, lib_path) {
            (Some(include_path), Some(lib_path)) => Ok((vec![include_path], vec![lib_path])),
            _ => {
                eprintln!(
                    "Couldn't find either header or lib files for {}>={}.",
                    LIB_NAME, LIB_MIN_VERSION
                );
                eprintln!("See the crate README for installation instructions, or use the 'bundled' feature to statically compile.");
                bail!("Aborting compilation due to linker failure.");
            },
        }
    }

    pub(super) fn build_if_necessary() -> Result<(), Error> {
        Ok(())
    }

    fn find_pkgconfig_paths() -> Result<(Option<PathBuf>, Option<PathBuf>), Error> {
        Ok(pkg_config::Config::new()
            .atleast_version(LIB_MIN_VERSION)
            .probe(LIB_NAME)
            .and_then(|mut lib| Ok((lib.include_paths.pop(), lib.link_paths.pop())))?)
    }
}

#[cfg(feature = "bundled")]
mod webrtc {
    use super::*;
    use std::{path::Path, process::Command};

    const BUNDLED_SOURCE_PATH: &str = "./webrtc-audio-processing";
    const BUNDLED_SOURCE_PATH_ABSEIL: &str = "./abseil-cpp";

    pub(super) fn get_build_paths() -> Result<(Vec<PathBuf>, Vec<PathBuf>), Error> {
        let build_dir = out_dir();
        let install_dir = out_dir();

        // Build abseil-cpp first
        let abseil_build_dir = build_dir.join("abseil-cpp");
        std::fs::create_dir_all(&abseil_build_dir)?;
        cmake::Config::new("abseil-cpp")
            .cxxflag("-std=c++17")
            .build();

        // Build webrtc-audio-processing
        let webrtc_build_dir = build_dir.join("webrtc-audio-processing");
        std::fs::create_dir_all(&webrtc_build_dir)?;
        
        // Add the WebRTC source directory to include paths
        let webrtc_src_dir = PathBuf::from("webrtc-audio-processing");
        
        let mut meson = Command::new("meson");
        let status = meson
            .args(&["--prefix", install_dir.to_str().unwrap()])
            .arg("-Ddefault_library=static")
            .arg(&webrtc_src_dir)
            .arg(&webrtc_build_dir)
            .status()
            .context("Failed to execute meson. Do you have it installed?")?;
        assert!(status.success(), "Command failed: {:?}", &meson);

        let mut ninja = Command::new("ninja");
        let status = ninja
            .args(&["-C", webrtc_build_dir.to_str().unwrap()])
            .arg("install")
            .status()
            .context("Failed to execute ninja. Do you have it installed?")?;
        assert!(status.success(), "Command failed: {:?}", &ninja);

        // Include both the installed headers and the source headers
        let include_paths = vec![
            install_dir.join("include/webrtc-audio-processing-2"),
            install_dir.join("include"),
            webrtc_src_dir.clone(),
            webrtc_src_dir.join("webrtc"),
        ];
        
        let lib_paths = vec![install_dir.join("lib")];
        
        Ok((include_paths, lib_paths))
    }

    pub(super) fn build_if_necessary() -> Result<(), Error> {
        if Path::new(BUNDLED_SOURCE_PATH).read_dir()?.next().is_none() {
            eprintln!("The webrtc-audio-processing source directory is empty.");
            eprintln!("See the crate README for installation instructions.");
            eprintln!("Remember to clone the repo recursively if building from source.");
            bail!("Aborting compilation because bundled source directory is empty.");
        }

        let build_dir = out_dir();
        let install_dir = out_dir();

        let abseil_build_dir = build_dir.join(BUNDLED_SOURCE_PATH_ABSEIL);
        std::fs::create_dir_all(&abseil_build_dir)?;
        cmake::Config::new(BUNDLED_SOURCE_PATH_ABSEIL).cxxflag("-std=c++17").build();

        let webrtc_build_dir = build_dir.join(BUNDLED_SOURCE_PATH);
        let mut meson = Command::new("meson");
        let status = meson
            .args(&["--prefix", install_dir.to_str().unwrap()])
            .arg("-Ddefault_library=static")
            .arg(BUNDLED_SOURCE_PATH)
            .arg(webrtc_build_dir.to_str().unwrap())
            .status()
            .context("Failed to execute meson. Do you have it installed?")?;
        assert!(status.success(), "Command failed: {:?}", &meson);

        let mut ninja = Command::new("ninja");
        let status = ninja
            .args(&["-C", webrtc_build_dir.to_str().unwrap()])
            .arg("install")
            .status()
            .context("Failed to execute ninja. Do you have it installed?")?;
        assert!(status.success(), "Command failed: {:?}", &ninja);

        Ok(())
    }
}

fn main() -> Result<(), Error> {
    let (include_dirs, lib_dirs) = webrtc::get_build_paths()?;

    // Build wrapper
    cc::Build::new()
        .cpp(true)
        .file("src/wrapper.cpp")
        .flag("-std=c++17")
        .flag("-Wno-unused-parameter")
        .includes(&include_dirs)
        .out_dir(&out_dir())
        .compile("webrtc_audio_processing_wrapper");

    println!("cargo:rustc-link-lib=static=webrtc_audio_processing_wrapper");

    // Add include directories to bindgen
    let mut builder = bindgen::Builder::default()
        .header("src/wrapper.hpp")
        .clang_args(&["-x", "c++", "-std=c++17", "-fparse-all-comments"])
        .clang_args(&[
            if cfg!(target_os = "macos") {
                "-DWEBRTC_POSIX"
            } else if cfg!(target_os = "linux") {
                "-DWEBRTC_POSIX"
            } else if cfg!(target_os = "windows") {
                "-DWEBRTC_WIN"
            } else {
                "-DWEBRTC_POSIX"
            },
            "-I.",
            "-Iwebrtc-audio-processing",
        ])
        .generate_comments(true)
        .enable_cxx_namespaces()
        .allowlist_type("webrtc::AudioProcessing_Error")
        .allowlist_type("webrtc::AudioProcessing_Config")
        .allowlist_type("webrtc::AudioProcessing_RealtimeSetting")
        .allowlist_type("webrtc::StreamConfig")
        .allowlist_type("webrtc::ProcessingConfig")
        .allowlist_function("webrtc_audio_processing_wrapper::.*")
        .blocklist_item("webrtc::AudioProcessing_Config_ToString")
        .opaque_type("std::.*")
        .derive_debug(true)
        .derive_default(true);

    for dir in &include_dirs {
        builder = builder.clang_arg(&format!("-I{}", dir.display()));
    }

    // Add library directories to linker
    for dir in &lib_dirs {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }

    if cfg!(feature = "bundled") {
        println!("cargo:rustc-link-lib=static=webrtc-audio-processing-2");
    } else {
        println!("cargo:rustc-link-lib=dylib=webrtc-audio-processing-2");
    }

    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }

    // Generate bindings
    let bindings = builder
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    Ok(())
}
