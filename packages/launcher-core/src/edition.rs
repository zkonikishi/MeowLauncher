//! Compile-time edition of every product built from this tree.
//!
//! `meowopia-custom` is the Meowopia server's own build: its account service,
//! website, content feed, update feed and client distribution source are all
//! compiled in, so it works with no configuration file at all.
//!
//! `sponsor` is the build a server owner hands to his players: it has no
//! compiled-in endpoints and refuses to run until `launcher.public.json`
//! supplies his own HTTPS services. It never silently falls back to Meowopia.
pub const ID: &str = env!("MEOW_LAUNCHER_EDITION");
/// Human label shown in the settings page and required in the update manifest.
pub const NOTES: &str = env!("MEOW_LAUNCHER_EDITION_NOTES");
pub fn is_sponsor() -> bool {
    ID == "sponsor"
}
pub fn is_custom() -> bool {
    !is_sponsor()
}
/// Display branding is separate from the stable update protocol identity.
pub fn display_name() -> &'static str {
    if is_custom() { "MeowLauncher Max" } else { "MeowLauncher" }
}
/// Meowopia's own origin. Only the custom edition may fall back to it.
pub const MEOWOPIA_ORIGIN: &str = "https://launcher.invalid";
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edition_is_one_of_the_two_supported_values() {
        assert!(ID == "meowopia-custom" || ID == "sponsor", "{ID}");
        assert_ne!(is_sponsor(), is_custom());
        assert!(!NOTES.is_empty() && NOTES.len() <= 80);
        if is_sponsor(){assert!(crate::account::AccountClient::new().is_err());assert!(crate::account::AccountClient::legacy_online().is_err());}
    }
}
