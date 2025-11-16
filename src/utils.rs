use std::path::{Path, PathBuf};

use dirs::home_dir;

pub fn expand_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(stripped) = text.strip_prefix("~") {
        if let Some(home) = home_dir() {
            return home.join(stripped.trim_start_matches(['/', '\\']));
        }
    }
    path.to_path_buf()
}
