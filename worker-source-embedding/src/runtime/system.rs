use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use super::GpuVendor;

pub(crate) fn read_os_release() -> (Option<String>, Option<String>, Option<String>) {
    let contents = match fs::read_to_string("/etc/os-release") {
        Ok(contents) => contents,
        Err(_) => return (None, None, None),
    };

    let mut distro_id = None;
    let mut distro_name = None;
    let mut distro_version = None;
    for line in contents.lines() {
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let value = raw_value.trim().trim_matches('"').to_string();
        match key.trim() {
            "ID" => distro_id = Some(value),
            "NAME" => distro_name = Some(value),
            "VERSION_ID" => distro_version = Some(value),
            _ => {}
        }
    }

    (distro_id, distro_name, distro_version)
}

pub(crate) fn detect_gpu_vendors() -> Vec<GpuVendor> {
    let mut vendors = Vec::new();
    let drm_dir = match fs::read_dir("/sys/class/drm") {
        Ok(entries) => entries,
        Err(_) => return vendors,
    };

    for entry in drm_dir.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with("card") || file_name.contains('-') {
            continue;
        }

        let vendor_path = entry.path().join("device/vendor");
        let Ok(raw_vendor) = fs::read_to_string(vendor_path) else {
            continue;
        };
        let vendor = match raw_vendor.trim() {
            "0x10de" => GpuVendor::Nvidia,
            "0x1002" => GpuVendor::Amd,
            "0x8086" => GpuVendor::Intel,
            _ => GpuVendor::Other,
        };
        if !vendors.contains(&vendor) {
            vendors.push(vendor);
        }
    }

    vendors
}

pub(crate) fn detect_render_node() -> bool {
    match fs::read_dir("/dev/dri") {
        Ok(entries) => entries
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD")),
        Err(_) => false,
    }
}

pub(crate) fn command_exists(name: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| path.join(name).exists()))
        .unwrap_or(false)
}

pub(crate) fn has_shared_library(name: &str) -> bool {
    for ldconfig in ["ldconfig", "/usr/sbin/ldconfig", "/sbin/ldconfig"] {
        if let Ok(output) = Command::new(ldconfig).arg("-p").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains(name) {
                    return true;
                }
            }
        }
    }

    shared_library_candidates(name)
        .iter()
        .any(|path| Path::new(path).exists())
}

fn shared_library_candidates(name: &str) -> &'static [&'static str] {
    match name {
        "libcuda.so" => &[
            "/lib/x86_64-linux-gnu/libcuda.so",
            "/usr/lib/x86_64-linux-gnu/libcuda.so",
            "/usr/lib/x86_64-linux-gnu/nvidia/current/libcuda.so",
        ],
        "libcuda.so.1" => &[
            "/lib/x86_64-linux-gnu/libcuda.so.1",
            "/usr/lib/x86_64-linux-gnu/libcuda.so.1",
            "/usr/lib/x86_64-linux-gnu/nvidia/current/libcuda.so.1",
        ],
        "libvulkan.so" => &[
            "/lib/x86_64-linux-gnu/libvulkan.so",
            "/usr/lib/x86_64-linux-gnu/libvulkan.so",
        ],
        "libvulkan.so.1" => &[
            "/lib/x86_64-linux-gnu/libvulkan.so.1",
            "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
        ],
        _ => &[],
    }
}
