use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Resolve the data directory from the env var value and CWD.
///
/// - `None` or empty/whitespace-only → default to `"data"` under `cwd`.
/// - Trailing `/` characters are stripped.
/// - If the stripped string is absolute, return it directly.
/// - Otherwise join under `cwd`.
pub fn resolve_data_dir(env_value: Option<&str>, cwd: &Path) -> PathBuf {
    let trimmed = env_value.map(|v| v.trim()).filter(|v| !v.is_empty());
    let stripped = trimmed.map(strip_trailing_separators);
    match stripped {
        Some(ref s) if Path::new(s).is_absolute() => PathBuf::from(s),
        Some(ref s) if !s.is_empty() => cwd.join(s),
        _ => cwd.join("data"),
    }
}

/// Ensure `path` exists as a directory, creating it (and parents) if needed.
pub fn ensure_data_dir(path: &Path) -> Result<(), AppError> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(AppError::Internal(format!(
            "data directory path is not a directory: {}",
            path.display()
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir_all(path)
            .map_err(|e| {
                AppError::Internal(format!(
                    "failed to create data directory {}: {}",
                    path.display(),
                    e
                ))
            }),
        Err(e) => Err(AppError::Internal(format!(
            "failed to access data directory {}: {}",
            path.display(),
            e
        ))),
    }
}

/// Returns the path to `yummybox.db` inside the given data directory.
pub fn db_path_in(data_dir: &Path) -> PathBuf {
    data_dir.join("yummybox.db")
}

fn strip_trailing_separators(s: &str) -> String {
    s.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn given_no_env_value_when_resolve_then_defaults_to_data_under_cwd() {
        let cwd = PathBuf::from("/home/user/project");
        let result = resolve_data_dir(None, &cwd);
        assert_eq!(result, cwd.join("data"));
    }

    #[test]
    fn given_empty_env_value_when_resolve_then_defaults_to_data_under_cwd() {
        let cwd = PathBuf::from("/home/user/project");
        assert_eq!(resolve_data_dir(Some(""), &cwd), cwd.join("data"));
        assert_eq!(resolve_data_dir(Some("   "), &cwd), cwd.join("data"));
    }

    #[test]
    fn given_trailing_slash_when_resolve_then_strips_separator() {
        let cwd = PathBuf::from("/home/user/project");
        assert_eq!(resolve_data_dir(Some("data/"), &cwd), cwd.join("data"));
        assert_eq!(resolve_data_dir(Some("data///"), &cwd), cwd.join("data"));
    }

    #[test]
    fn given_absolute_path_when_resolve_then_returns_as_absolute() {
        let cwd = PathBuf::from("/home/user/project");
        assert_eq!(
            resolve_data_dir(Some("/opt/yummybox"), &cwd),
            PathBuf::from("/opt/yummybox")
        );
    }

    #[test]
    fn given_absolute_path_with_trailing_slash_when_resolve_then_strips_separator() {
        let cwd = PathBuf::from("/home/user/project");
        assert_eq!(
            resolve_data_dir(Some("/opt/yummybox/"), &cwd),
            PathBuf::from("/opt/yummybox")
        );
    }

    #[test]
    fn given_relative_path_when_resolve_then_joins_under_cwd() {
        let cwd = PathBuf::from("/home/user/project");
        assert_eq!(
            resolve_data_dir(Some("./storage"), &cwd),
            cwd.join("storage")
        );
    }

    #[test]
    fn given_root_separator_only_when_resolve_then_falls_back_to_default() {
        let cwd = PathBuf::from("/home/user/project");
        assert_eq!(resolve_data_dir(Some("/"), &cwd), cwd.join("data"));
        assert_eq!(resolve_data_dir(Some("//"), &cwd), cwd.join("data"));
    }

    #[test]
    fn given_missing_dir_when_ensure_then_creates_it() {
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("new_sub");
        assert!(ensure_data_dir(&new_dir).is_ok());
        assert!(new_dir.is_dir());
    }

    #[test]
    fn given_existing_dir_when_ensure_then_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        // tempdir() already created the directory
        assert!(ensure_data_dir(tmp.path()).is_ok());
    }

    #[test]
    fn given_existing_file_at_path_when_ensure_then_returns_internal_error() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("not_a_dir");
        std::fs::write(&file_path, b"i am a file").unwrap();
        let err = ensure_data_dir(&file_path).unwrap_err();
        match err {
            AppError::Internal(msg) => {
                assert!(msg.contains(&file_path.display().to_string()));
            }
            _ => panic!("expected AppError::Internal"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn given_uncreatable_parent_when_ensure_then_returns_internal_error() {
        let bad_path = PathBuf::from("/dev/null/yummybox-data");
        let err = ensure_data_dir(&bad_path).unwrap_err();
        match err {
            AppError::Internal(msg) => {
                assert!(msg.contains(&bad_path.display().to_string()));
            }
            _ => panic!("expected AppError::Internal"),
        }
    }
}
