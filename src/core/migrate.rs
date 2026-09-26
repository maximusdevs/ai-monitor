//! One-time carry-over from the project's former name, `ai-usagebar`.
//!
//! Each binary calls [`migrate_legacy_dirs`] first. For every directory the
//! old name owned (config, data, cache, and the `~/.config` path macOS also
//! honors), a missing `ai-monitor` counterpart is filled with a copy of the old
//! one. It is a copy, not a move: an `ai-usagebar` still installed alongside
//! keeps working. Paths inside the copied `config.toml` that point into an old
//! directory (account dirs, credential files) are rewritten to the new one.
//!
//! Best-effort and silent: the widget's stdout is Waybar's JSON, and a failed
//! migration only means the user configures again. Each copy is assembled in a
//! temporary sibling and renamed into place, so two Waybar instances starting
//! together never see a half-copied directory.

use std::fs;
use std::path::{Path, PathBuf};

const LEGACY_NAME: &str = "ai-usagebar";
const NAME: &str = "ai-monitor";

/// Copy each legacy directory whose new counterpart does not exist yet.
pub fn migrate_legacy_dirs() {
    let pairs = legacy_pairs();
    for (old, new) in &pairs {
        let _ = migrate_dir(old, new, &pairs);
    }
}

fn legacy_pairs() -> Vec<(PathBuf, PathBuf)> {
    let mut pairs = Vec::new();
    if let (Some(old), Some(new)) = (
        directories::ProjectDirs::from("", "", LEGACY_NAME),
        directories::ProjectDirs::from("", "", NAME),
    ) {
        pairs.push((
            old.config_dir().to_path_buf(),
            new.config_dir().to_path_buf(),
        ));
        pairs.push((old.data_dir().to_path_buf(), new.data_dir().to_path_buf()));
    }
    if let Some(base) = directories::BaseDirs::new() {
        pairs.push((
            base.cache_dir().join(LEGACY_NAME),
            base.cache_dir().join(NAME),
        ));
        let config = base.home_dir().join(".config");
        pairs.push((config.join(LEGACY_NAME), config.join(NAME)));
    }
    // On macOS the config and data dirs coincide; keep the first of each.
    let mut seen = Vec::new();
    pairs.retain(|(old, _)| {
        let fresh = !seen.contains(old);
        seen.push(old.clone());
        fresh
    });
    pairs
}

/// Copy `old` to `new` when `new` is absent, rewriting any `rewrites` prefix
/// found in the copied `config.toml`. Returns whether a copy was made.
pub fn migrate_dir(
    old: &Path,
    new: &Path,
    rewrites: &[(PathBuf, PathBuf)],
) -> std::io::Result<bool> {
    if new.exists() || !old.is_dir() {
        return Ok(false);
    }
    let parent = new
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent"))?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".migrate.")
        .tempdir_in(parent)?;
    copy_tree(old, staging.path())?;
    rewrite_config(&staging.path().join("config.toml"), rewrites);
    let staged = staging.keep();
    if let Err(error) = fs::rename(&staged, new) {
        // Another process finished first; its copy is as good as ours.
        let _ = fs::remove_dir_all(&staged);
        return if new.exists() { Ok(false) } else { Err(error) };
    }
    Ok(true)
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::set_permissions(to, fs::metadata(from)?.permissions())?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            fs::create_dir(&target)?;
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            // `fs::copy` carries the mode, so 0600 credential files stay 0600.
            fs::copy(entry.path(), &target)?;
        }
        // Symlinks and sockets are left behind: a lock socket or a link into
        // the old tree is not something the new directory should inherit.
    }
    Ok(())
}

fn rewrite_config(path: &Path, rewrites: &[(PathBuf, PathBuf)]) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let rewritten = rewrite_paths(&text, rewrites);
    if rewritten != text {
        let _ = fs::write(path, rewritten);
    }
}

/// Replace each old directory prefix with its new one, in both the literal
/// spelling (TOML `'…'` strings) and the escaped one (`"C:\\…"` on Windows).
pub fn rewrite_paths(text: &str, rewrites: &[(PathBuf, PathBuf)]) -> String {
    let mut text = text.to_string();
    for (old, new) in rewrites {
        let (old, new) = (old.to_string_lossy(), new.to_string_lossy());
        if old.is_empty() {
            continue;
        }
        let escaped_old = old.replace('\\', "\\\\");
        if escaped_old != old {
            text = text.replace(&escaped_old, &new.replace('\\', "\\\\"));
        }
        text = text.replace(old.as_ref(), &new);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_the_old_tree_and_rewrites_config_paths() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("ai-usagebar");
        let new = root.path().join("ai-monitor");
        fs::create_dir_all(old.join("accounts/work")).unwrap();
        fs::write(old.join("accounts/work/.credentials.json"), "{}").unwrap();
        let account = old.join("accounts/work");
        fs::write(
            old.join("config.toml"),
            format!(
                "[[anthropic.accounts]]\nconfig_dir = \"{}\"\n",
                account.display()
            ),
        )
        .unwrap();

        let pairs = vec![(old.clone(), new.clone())];
        assert!(migrate_dir(&old, &new, &pairs).unwrap());

        assert!(new.join("accounts/work/.credentials.json").is_file());
        let config = fs::read_to_string(new.join("config.toml")).unwrap();
        assert!(config.contains(&new.join("accounts/work").display().to_string()));
        assert!(!config.contains("ai-usagebar"));
        // A copy: the old tree is untouched.
        assert!(old.join("config.toml").is_file());
        // No staging directory is left behind.
        let leftovers: Vec<_> = fs::read_dir(root.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".migrate."))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn an_existing_new_directory_is_never_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        let new = root.path().join("new");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("config.toml"), "old").unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("config.toml"), "new").unwrap();

        assert!(!migrate_dir(&old, &new, &[]).unwrap());
        assert_eq!(fs::read_to_string(new.join("config.toml")).unwrap(), "new");
    }

    #[test]
    fn a_missing_old_directory_is_a_no_op() {
        let root = tempfile::tempdir().unwrap();
        let new = root.path().join("new");
        assert!(!migrate_dir(&root.path().join("absent"), &new, &[]).unwrap());
        assert!(!new.exists());
    }

    #[cfg(unix)]
    #[test]
    fn private_files_stay_private() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        let new = root.path().join("new");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("config.toml"), "").unwrap();
        fs::set_permissions(old.join("config.toml"), fs::Permissions::from_mode(0o600)).unwrap();

        migrate_dir(&old, &new, &[]).unwrap();
        let mode = fs::metadata(new.join("config.toml"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn rewrites_escaped_windows_paths() {
        let pairs = vec![(
            PathBuf::from(r"C:\Users\x\AppData\Roaming\ai-usagebar\config"),
            PathBuf::from(r"C:\Users\x\AppData\Roaming\ai-monitor\config"),
        )];
        let text = r#"dir = "C:\\Users\\x\\AppData\\Roaming\\ai-usagebar\\config\\accounts""#;
        assert_eq!(
            rewrite_paths(text, &pairs),
            r#"dir = "C:\\Users\\x\\AppData\\Roaming\\ai-monitor\\config\\accounts""#
        );
    }
}
