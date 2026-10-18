//! One source tree produces two products: the Meowopia custom build and the
//! generic build a server owner configures himself. The edition is fixed at
//! build time and never read from a file, an environment variable or the
//! network at run time, so a shipped EXE cannot be talked into the other
//! edition's defaults.
fn main() {
    println!("cargo:rerun-if-env-changed=MEOW_LAUNCHER_EDITION");
    let (id, notes) = match std::env::var("MEOW_LAUNCHER_EDITION").as_deref() {
        Ok("sponsor") => ("sponsor", "MeowLauncher 通用版"),
        Ok("meowopia-custom") | Err(_) => (
            "meowopia-custom",
            "MeowLauncher为Meowopia服务器定制版",
        ),
        Ok(other) => panic!(
            "MEOW_LAUNCHER_EDITION must be meowopia-custom or sponsor, got {other:?}"
        ),
    };
    println!("cargo:rustc-env=MEOW_LAUNCHER_EDITION={id}");
    println!("cargo:rustc-env=MEOW_LAUNCHER_EDITION_NOTES={notes}");
}
