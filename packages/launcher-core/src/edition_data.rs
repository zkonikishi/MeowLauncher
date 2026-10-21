//! Edition-specific persistent state. Never import another edition's sessions.
use crate::launch_contract::Edition;
use std::path::{Path,PathBuf};
pub fn directory(package:&Path,edition:Edition)->PathBuf {
    package.join("data").join(match edition{Edition::Beta=>"beta",Edition::Max=>"max"})
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn separate_roots_do_not_select_old_shared_state(){let base=Path::new("package");let beta=directory(base,Edition::Beta);let max=directory(base,Edition::Max);assert_ne!(beta,max);assert_eq!(beta,base.join("data/beta"));assert_eq!(max,base.join("data/max"));assert_ne!(beta,base.join("data"));}
}
