use std::env;
use std::path::{Path, PathBuf};

pub fn onnxruntime_dylib_name() -> &'static str {
    match env::consts::OS {
        "macos" => "libonnxruntime.dylib",
        _ => "libonnxruntime.so",
    }
}

pub(crate) fn ort_dylib_candidates(explicit_ort_dylib_path: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit_ort_dylib_path {
        candidates.push(path.to_path_buf());
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join("lib").join(onnxruntime_dylib_name()));
            candidates.push(parent.join(onnxruntime_dylib_name()));
        }
    }

    for candidate in onnxruntime_system_candidates() {
        candidates.push(PathBuf::from(candidate));
    }

    dedupe_paths(candidates)
}

fn onnxruntime_system_candidates() -> &'static [&'static str] {
    match env::consts::OS {
        "macos" => &[
            "/opt/homebrew/lib/libonnxruntime.dylib",
            "/usr/local/lib/libonnxruntime.dylib",
            "/usr/lib/libonnxruntime.dylib",
        ],
        _ => &[
            "/usr/lib/manifeed/embedding/runtime/lib/libonnxruntime.so",
            "/usr/lib/libonnxruntime.so",
            "/usr/lib64/libonnxruntime.so",
            "/usr/local/lib/libonnxruntime.so",
            "/usr/local/lib64/libonnxruntime.so",
            "/lib/x86_64-linux-gnu/libonnxruntime.so",
            "/lib/aarch64-linux-gnu/libonnxruntime.so",
        ],
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if unique.iter().any(|existing| existing == &path) {
            continue;
        }
        unique.push(path);
    }
    unique
}
