fn main() {
    println!("cargo:rerun-if-env-changed=MEOW_LAUNCHER_EDITION");
    let custom = std::env::var("MEOW_LAUNCHER_EDITION").unwrap_or_else(|_| "meowopia-custom".into()) != "sponsor";
    let mut config: serde_json::Value = std::env::var("TAURI_CONFIG").ok()
        .map(|s| serde_json::from_str(&s).expect("valid TAURI_CONFIG"))
        .unwrap_or_else(|| serde_json::json!({}));
    config["productName"] = serde_json::json!(if custom { "MeowLauncher Max" } else { "MeowLauncher" });
    let config = config.to_string();
    std::env::set_var("TAURI_CONFIG", &config);
    println!("cargo:rustc-env=TAURI_CONFIG={config}");
    let icon = "icons/site-favicon.ico";
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new().window_icon_path(icon)),
    )
    .expect("launcher Windows resources");
}
