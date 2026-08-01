use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let plugin_manifest = crate_dir.join("../../.codex-plugin/plugin.json");
    println!("cargo:rerun-if-changed={}", plugin_manifest.display());

    let package_version = env::var("CARGO_PKG_VERSION").unwrap();
    let version = if plugin_manifest.is_file() {
        let bytes = fs::read(&plugin_manifest).unwrap_or_else(|error| {
            panic!(
                "cannot read plugin manifest {}: {error}",
                plugin_manifest.display()
            )
        });
        let manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!(
                "cannot parse plugin manifest {}: {error}",
                plugin_manifest.display()
            )
        });
        manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "plugin manifest {} has no non-empty version",
                    plugin_manifest.display()
                )
            })
            .to_string()
    } else {
        package_version
    };

    if !version
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
    {
        panic!("plugin version contains unsupported characters: {version:?}");
    }
    println!("cargo:rustc-env=TREEWORK_BUILD_VERSION={version}");
}
