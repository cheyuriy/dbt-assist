use std::path::PathBuf;

/// Expands a leading `~` (or `~/...`) to the user's home directory. Paths
/// without a leading tilde, or a `~user` form we can't resolve, are returned
/// unchanged.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~')
        && (rest.is_empty() || rest.starts_with('/'))
        && let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf())
    {
        return home.join(rest.trim_start_matches('/'));
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::expand_tilde;

    #[test]
    fn expand_tilde_leaves_plain_paths_unchanged() {
        assert_eq!(
            expand_tilde("/var/manifest").to_str(),
            Some("/var/manifest")
        );
        assert_eq!(expand_tilde("relative/dir").to_str(), Some("relative/dir"));
    }

    #[test]
    fn expand_tilde_does_not_expand_named_user() {
        assert_eq!(expand_tilde("~other/dir").to_str(), Some("~other/dir"));
    }

    #[test]
    fn expand_tilde_expands_leading_tilde() {
        let home = directories::UserDirs::new()
            .unwrap()
            .home_dir()
            .to_path_buf();
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/manifest"), home.join("manifest"));
    }
}
