use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub(super) struct Workspace {
    pub(super) root: PathBuf,
    pub(super) root_note: Option<String>,
}

impl Workspace {
    pub(super) fn new(root: PathBuf) -> Result<Self, String> {
        Self::new_with_root_note(root, None)
    }

    pub(super) fn new_with_root_note(
        root: PathBuf,
        root_note: Option<String>,
    ) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|err| err.to_string())?;
        Ok(Self { root, root_note })
    }

    pub(super) fn resolve_existing(&self, requested: &str) -> Result<PathBuf, String> {
        let requested_path = Path::new(requested);
        reject_unsafe_relative_path(requested_path)?;
        let joined = self.root.join(requested_path);
        let resolved = joined.canonicalize().map_err(|err| err.to_string())?;
        if !resolved.starts_with(&self.root) {
            return Err("path escapes workspace root".to_string());
        }
        Ok(resolved)
    }

    pub(super) fn resolve_new_file(&self, requested: &str) -> Result<PathBuf, String> {
        let requested_path = Path::new(requested);
        reject_unsafe_relative_path(requested_path)?;
        if requested_path.as_os_str().is_empty() {
            return Err("missing non-empty `path`".to_string());
        }
        let parent = requested_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent = self.resolve_existing(parent.to_string_lossy().as_ref())?;
        if !parent.is_dir() {
            return Err("parent path is not a directory".to_string());
        }
        let Some(file_name) = requested_path.file_name() else {
            return Err("path must name a file".to_string());
        };
        let path = parent.join(file_name);
        if path.exists() {
            return Err("path already exists".to_string());
        }
        if !path.starts_with(&self.root) {
            return Err("path escapes workspace root".to_string());
        }
        Ok(path)
    }

    pub(super) fn display_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .ok()
            .and_then(|path| path.to_str())
            .filter(|path| !path.is_empty())
            .unwrap_or(".")
            .to_string()
    }

    pub(super) fn contains_existing(&self, path: &Path) -> bool {
        path.canonicalize()
            .map(|path| path.starts_with(&self.root))
            .unwrap_or(false)
    }
}

fn reject_unsafe_relative_path(path: &Path) -> Result<(), String> {
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        )
    }) {
        return Err("path must stay inside workspace root".to_string());
    }
    Ok(())
}
