use std::path::Path;

fn rerun_ui_assets(path: &Path) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            rerun_ui_assets(&entry_path);
        } else {
            println!("cargo:rerun-if-changed={}", entry_path.display());
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=SLINT_LIVE_PREVIEW");
    rerun_ui_assets(Path::new("ui"));

    let config = slint_build::CompilerConfiguration::new()
        .with_style("cupertino-light".into())
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);

    slint_build::compile_with_config("ui/app.slint", config).unwrap();
}
