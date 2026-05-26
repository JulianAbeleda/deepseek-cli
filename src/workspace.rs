use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RootSource {
    Exact,
    Fuzzy { requested: String, matched: String },
    Selected,
    Cwd,
}

impl RootSource {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Exact => "explicit",
            Self::Fuzzy { .. } => "fuzzy",
            Self::Selected => "explicit",
            Self::Cwd => "cwd",
        }
    }

    pub(crate) fn fuzzy_note(&self) -> Option<String> {
        match self {
            Self::Fuzzy { requested, matched } if matched.is_empty() => {
                Some(format!("matched: {requested}"))
            }
            Self::Fuzzy { requested, matched } => {
                Some(format!("matched: {requested} -> {matched}"))
            }
            _ => None,
        }
    }

    fn merge(self, next: RootSource) -> RootSource {
        match (self, next) {
            (
                RootSource::Fuzzy { requested, matched },
                RootSource::Fuzzy {
                    requested: next_requested,
                    matched: next_matched,
                },
            ) => RootSource::Fuzzy {
                requested: format!("{requested} -> {matched}; {next_requested} -> {next_matched}"),
                matched: String::new(),
            },
            (RootSource::Fuzzy { requested, matched }, _) => {
                RootSource::Fuzzy { requested, matched }
            }
            (_, RootSource::Fuzzy { requested, matched }) => {
                RootSource::Fuzzy { requested, matched }
            }
            (_, source) => source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedRoot {
    pub(crate) path: PathBuf,
    pub(crate) source: RootSource,
}

impl ResolvedRoot {
    fn exact(path: PathBuf) -> Self {
        Self {
            path,
            source: RootSource::Exact,
        }
    }
}

fn workspace_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if home.as_ref().is_some_and(|home| paths_equal(&cwd, home)) {
        return None;
    }
    Some(cwd)
}

pub(crate) fn effective_workspace_root(selected_root: Option<&Path>) -> Option<PathBuf> {
    selected_root.map(Path::to_path_buf).or_else(workspace_root)
}

pub(crate) fn effective_workspace_root_with_source(
    selected_root: Option<&Path>,
) -> Option<ResolvedRoot> {
    selected_root
        .map(|path| ResolvedRoot {
            path: path.to_path_buf(),
            source: RootSource::Selected,
        })
        .or_else(|| {
            workspace_root().map(|path| ResolvedRoot {
                path,
                source: RootSource::Cwd,
            })
        })
}

#[cfg(test)]
pub(crate) fn infer_natural_root(prompt: &str) -> Option<PathBuf> {
    infer_natural_root_with_source(prompt).map(|resolved| resolved.path)
}

pub(crate) fn infer_natural_root_with_source(prompt: &str) -> Option<ResolvedRoot> {
    if let Ok(Some(root)) = parse_arrow_chain_root(prompt) {
        return Some(root);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let lowered = prompt.to_lowercase();
    if lowered.contains("desktop") {
        return Some(ResolvedRoot::exact(home.join("Desktop")));
    }
    if lowered.contains("downloads") {
        return Some(ResolvedRoot::exact(home.join("Downloads")));
    }
    if lowered.contains("documents") {
        return Some(ResolvedRoot::exact(home.join("Documents")));
    }
    if lowered.contains("env folder")
        || lowered.contains("env directory")
        || lowered.contains("my env")
    {
        return Some(ResolvedRoot::exact(home.join("env")));
    }
    None
}

#[cfg(test)]
pub(crate) fn parse_navigation_request(prompt: &str) -> Result<Option<PathBuf>, String> {
    parse_navigation_request_from(prompt, None)
}

#[cfg(test)]
pub(crate) fn parse_navigation_request_from(
    prompt: &str,
    base_root: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    parse_navigation_request_with_source(prompt, base_root).map(|root| root.map(|root| root.path))
}

pub(crate) fn parse_navigation_request_with_source(
    prompt: &str,
    base_root: Option<&Path>,
) -> Result<Option<ResolvedRoot>, String> {
    if has_arrow_chain_trailing_task(prompt) {
        return Ok(None);
    }
    if let Some(parsed) = parse_arrow_chain_root_with_task(prompt, base_root)? {
        return Ok((!parsed.has_trailing_task).then_some(ResolvedRoot {
            path: parsed.root,
            source: parsed.source,
        }));
    }
    let prompt = prompt.trim();
    let lowered = prompt.to_lowercase();
    let Some((target, explicit_path)) = navigation_target(prompt, &lowered) else {
        return Ok(None);
    };
    let target = clean_navigation_target(target);
    if target.is_empty() {
        return Ok(None);
    }
    if let Some(root) = navigation_alias_root(target) {
        return canonical_dir(root).map(|path| Some(ResolvedRoot::exact(path)));
    }
    let path = expand_path(target, base_root);
    if path.is_dir() {
        return canonical_dir(path).map(|path| Some(ResolvedRoot::exact(path)));
    }
    if !explicit_path && !looks_like_path_target(target) {
        if let Some(base) = base_root {
            match fuzzy_child_dir(base, target) {
                Ok(root) => return Ok(Some(root)),
                Err(err) if err.contains("ambiguous") => return Err(err),
                Err(_) => {}
            }
        }
    }
    if explicit_path || looks_like_path_target(target) {
        return Err(format!("{} is not a directory", path.display()));
    }
    Ok(None)
}

pub(crate) fn parse_root_command(prompt: &str) -> Option<Option<&str>> {
    if prompt == "/root" {
        return Some(None);
    }
    prompt
        .strip_prefix("/root ")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Some)
}

pub(crate) fn update_selected_root(root_arg: &str) -> Result<Option<PathBuf>, String> {
    if matches!(root_arg, "clear" | "reset" | "cwd") {
        return Ok(None);
    }
    let path = PathBuf::from(root_arg);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|err| err.to_string())?
            .join(path)
    };
    let root = path
        .canonicalize()
        .map_err(|err| format!("{}: {err}", path.display()))?;
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    Ok(Some(root))
}

pub(crate) fn update_selected_root_from(
    root_arg: &str,
    base_root: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    if matches!(root_arg, "clear" | "reset" | "cwd") {
        return Ok(None);
    }
    let path = expand_path(root_arg, base_root);
    canonical_dir(path).map(Some)
}

fn navigation_target<'a>(prompt: &'a str, lowered: &str) -> Option<(&'a str, bool)> {
    for verb in [
        "cd", "go", "navigate", "switch", "move", "change", "enter", "open",
    ] {
        if lowered == verb {
            return None;
        }
        if let Some(rest) = lowered
            .strip_prefix(verb)
            .and_then(|rest| rest.strip_prefix(' '))
        {
            let offset = prompt.len() - rest.len();
            return Some((&prompt[offset..], verb == "cd"));
        }
    }
    None
}

fn clean_navigation_target(target: &str) -> &str {
    let mut target = target.trim();
    for separator in [" and ", " then "] {
        if let Some(index) = target.to_lowercase().find(separator) {
            target = &target[..index];
        }
    }
    target = trim_navigation_punctuation(target.trim());
    for prep in ["into ", "inside ", "from ", "to ", "in "] {
        if target.to_lowercase().starts_with(prep) {
            target = &target[prep.len()..];
            break;
        }
    }
    target = trim_navigation_punctuation(target.trim());
    if target.to_lowercase().starts_with("the ") {
        target = &target[4..];
    }
    for suffix in [" folder", " directory", " repo", " repository"] {
        if let Some(stripped) = target.strip_suffix(suffix) {
            return stripped.trim();
        }
    }
    target
}

fn trim_navigation_punctuation(target: &str) -> &str {
    let target = target.trim_matches(|ch: char| matches!(ch, ',' | ';' | ':' | '"' | '\''));
    if target == "." || target == ".." || looks_like_path_target(target) {
        return target;
    }
    target.trim_matches('.')
}

fn expand_path(path: &str, base_root: Option<&Path>) -> PathBuf {
    if path == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            return home.join(rest);
        }
    }
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else if let Some(base_root) = base_root {
        base_root.join(path)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn navigation_alias_root(target: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    match target.to_lowercase().as_str() {
        "desktop" => Some(home.join("Desktop")),
        "downloads" => Some(home.join("Downloads")),
        "documents" => Some(home.join("Documents")),
        "env" | "my env" => Some(home.join("env")),
        _ => None,
    }
}

fn looks_like_path_target(target: &str) -> bool {
    target == "~"
        || target.starts_with("~/")
        || target == "."
        || target == ".."
        || target.starts_with('/')
        || target.starts_with("./")
        || target.starts_with("../")
        || target.contains('/')
}

pub(crate) fn has_arrow_chain_trailing_task(prompt: &str) -> bool {
    if !prompt.contains("->") {
        return false;
    }
    let parts = prompt
        .split("->")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some(last) = parts.last() else {
        return false;
    };
    if is_trailing_instruction_segment(last) {
        return true;
    }
    last.split_once(". ")
        .is_some_and(|(_, instruction)| is_trailing_instruction_segment(instruction))
}

struct ArrowChainRoot {
    root: PathBuf,
    has_trailing_task: bool,
    source: RootSource,
}

fn parse_arrow_chain_root(prompt: &str) -> Result<Option<ResolvedRoot>, String> {
    parse_arrow_chain_root_with_task(prompt, None).map(|parsed| {
        parsed.map(|parsed| ResolvedRoot {
            path: parsed.root,
            source: parsed.source,
        })
    })
}

fn parse_arrow_chain_root_with_task(
    prompt: &str,
    base_root: Option<&Path>,
) -> Result<Option<ArrowChainRoot>, String> {
    if !prompt.contains("->") {
        return Ok(None);
    }
    let parts = prompt
        .split("->")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some(first) = parts.first() else {
        return Ok(None);
    };
    let lowered = first.to_lowercase();
    let Some((target, _)) = navigation_target(first, &lowered) else {
        return Ok(None);
    };
    let target = clean_navigation_target(target);
    if target.is_empty() {
        return Ok(None);
    }

    let mut source = RootSource::Exact;
    let mut root = if let Some(root) = navigation_alias_root(target) {
        canonical_dir(root)?
    } else {
        let path = expand_path(target, base_root);
        if path.is_dir() {
            canonical_dir(path)?
        } else if !looks_like_path_target(target) {
            let base = base_root
                .map(Path::to_path_buf)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let resolved = fuzzy_child_dir(&base, target)?;
            source = source.merge(resolved.source);
            resolved.path
        } else {
            canonical_dir(path)?
        }
    };
    let mut has_task = false;

    for (index, part) in parts.iter().enumerate().skip(1) {
        let part = trim_navigation_punctuation(part.trim());
        if part.is_empty() {
            continue;
        }
        if index + 1 == parts.len() {
            if let Some((next_root, trailing_task)) =
                split_child_with_trailing_instruction(&root, part)?
            {
                root = next_root;
                if trailing_task {
                    has_task = true;
                    break;
                }
            }
        }
        let child = clean_navigation_target(part);
        if child.is_empty() {
            continue;
        }
        let child_path = root.join(child);
        if child_path.is_dir() {
            root = canonical_dir(child_path)?;
            continue;
        }
        if !looks_like_path_target(child) {
            match fuzzy_child_dir(&root, child) {
                Ok(resolved) => {
                    root = resolved.path;
                    source = source.merge(resolved.source);
                    continue;
                }
                Err(err) if index + 1 != parts.len() || !is_trailing_instruction_segment(part) => {
                    return Err(err);
                }
                Err(_) => {}
            }
        }
        if index + 1 == parts.len() && is_trailing_instruction_segment(part) {
            has_task = true;
            break;
        }
        return Err(format!("{} is not a directory", child_path.display()));
    }

    Ok(Some(ArrowChainRoot {
        root,
        has_trailing_task: has_task,
        source,
    }))
}

fn split_child_with_trailing_instruction(
    root: &Path,
    part: &str,
) -> Result<Option<(PathBuf, bool)>, String> {
    let Some((child, instruction)) = part.split_once(". ") else {
        return Ok(None);
    };
    let child = clean_navigation_target(child);
    if child.is_empty() || instruction.trim().is_empty() {
        return Ok(None);
    }
    let child_path = root.join(child);
    if child_path.is_dir() && is_trailing_instruction_segment(instruction) {
        return Ok(Some((canonical_dir(child_path)?, true)));
    }
    Ok(None)
}

fn is_trailing_instruction_segment(part: &str) -> bool {
    let part = part.trim();
    if part.is_empty() || looks_like_path_target(part) {
        return false;
    }
    let lowered = part.to_lowercase();
    if is_single_word_task_instruction(&lowered) {
        return true;
    }
    if [" folder", " directory", " repo", " repository"]
        .iter()
        .any(|suffix| lowered.ends_with(suffix))
    {
        return false;
    }
    part.ends_with('?') || part.split_whitespace().nth(1).is_some()
}

fn is_single_word_task_instruction(part: &str) -> bool {
    matches!(
        part,
        "analyze" | "audit" | "inspect" | "list" | "read" | "review" | "scan" | "summarize"
    )
}

fn canonical_dir(path: PathBuf) -> Result<PathBuf, String> {
    let root = path
        .canonicalize()
        .map_err(|err| format!("{}: {err}", path.display()))?;
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    Ok(root)
}

fn fuzzy_child_dir(root: &Path, requested: &str) -> Result<ResolvedRoot, String> {
    let requested = clean_navigation_target(requested).trim();
    if requested.len() < 3 || requested.split_whitespace().nth(1).is_some() {
        return Err(format!(
            "{} is not a directory",
            root.join(requested).display()
        ));
    }
    let root = canonical_dir(root.to_path_buf())?;
    let requested_key = fuzzy_key(requested);
    if requested_key.len() < 3 {
        return Err(format!(
            "{} is not a directory",
            root.join(requested).display()
        ));
    }
    let mut candidates = std::fs::read_dir(&root)
        .map_err(|err| format!("{}: {err}", root.display()))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let score = fuzzy_dir_score(&requested_key, &fuzzy_key(&name));
            (score >= 0.82).then_some((score, name, entry.path()))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.1.cmp(&right.1))
    });
    let Some((score, matched, path)) = candidates.first().cloned() else {
        return Err(format!(
            "{} is not a directory",
            root.join(requested).display()
        ));
    };
    if candidates.get(1).is_some_and(|next| score - next.0 < 0.08) {
        return Err(format!(
            "{} is ambiguous; closest matches include {} and {}",
            requested, matched, candidates[1].1
        ));
    }
    Ok(ResolvedRoot {
        path: canonical_dir(path)?,
        source: RootSource::Fuzzy {
            requested: requested.to_string(),
            matched,
        },
    })
}

fn fuzzy_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn fuzzy_dir_score(requested: &str, candidate: &str) -> f64 {
    if requested == candidate {
        return 1.0;
    }
    strsim::normalized_levenshtein(requested, candidate)
}

pub(crate) fn root_status(root: Option<&Path>, explicit: bool) -> String {
    match root {
        Some(root) => format!(
            "root: {}\nroot-source: {}\n",
            root.display(),
            if explicit { "explicit" } else { "cwd" }
        ),
        None => "root: unset\nroot-source: none\nUse /root <path> before running workspace tasks from $HOME.\n".to_string(),
    }
}

pub(crate) fn root_status_with_source(root: Option<&ResolvedRoot>) -> String {
    match root {
        Some(root) => {
            let mut status = format!(
                "root: {}\nroot-source: {}\n",
                root.path.display(),
                root.source.label()
            );
            if let Some(note) = root.source.fuzzy_note() {
                status.push_str(&note);
                status.push('\n');
            }
            status
        }
        None => "root: unset\nroot-source: none\nUse /root <path> before running workspace tasks from $HOME.\n".to_string(),
    }
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

pub(crate) fn path_boundary_clarify_text(root: &Path, path: &Path) -> String {
    let suggested_root = path.parent().unwrap_or(root);
    format!(
        "route: unclear\nReferenced path is outside the selected workspace root.\nroot: {}\npath: {}\nSuggested root: {}\nType /root {} to choose that workspace, or /chat to discuss.\n",
        root.display(),
        path.display(),
        suggested_root.display(),
        suggested_root.display()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        has_arrow_chain_trailing_task, infer_natural_root, infer_natural_root_with_source,
        parse_navigation_request, parse_navigation_request_from,
        parse_navigation_request_with_source, parse_root_command, path_boundary_clarify_text,
        root_status, update_selected_root_from, RootSource,
    };
    use crate::test_support::env_lock;
    use std::fs;
    use std::path::Path;

    #[test]
    fn parses_root_slash_command() {
        assert_eq!(parse_root_command("/root"), Some(None));
        assert_eq!(parse_root_command("/root   .  "), Some(Some(".")));
        assert_eq!(parse_root_command("/root clear"), Some(Some("clear")));
        assert_eq!(parse_root_command("root ."), None);
    }

    #[test]
    fn root_status_reports_source() {
        let root = Path::new("/tmp/workspace");
        assert!(root_status(Some(root), true).contains("root-source: explicit"));
        assert!(root_status(Some(root), false).contains("root-source: cwd"));
        assert!(root_status(None, false).contains("root: unset"));
    }

    #[test]
    fn infers_natural_roots_from_prompt() {
        let _guard = env_lock();
        use super::infer_natural_root;
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap();
        assert_eq!(
            infer_natural_root("read my files on my desktop"),
            Some(home.join("Desktop"))
        );
        assert_eq!(
            infer_natural_root("scan my downloads"),
            Some(home.join("Downloads"))
        );
        assert_eq!(
            infer_natural_root("inspect documents"),
            Some(home.join("Documents"))
        );
        assert_eq!(
            infer_natural_root("go to my env folder"),
            Some(home.join("env"))
        );
        assert_eq!(infer_natural_root("switch to the deepseek repo"), None);
        assert_eq!(infer_natural_root("fix main.rs"), None);
    }

    #[test]
    fn parses_navigation_requests_as_persistent_roots() {
        let _guard = env_lock();
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap();
        let env_root = home.join("env");
        let isolated_root = tempfile::tempdir().unwrap();
        if env_root.is_dir() {
            assert_eq!(
                parse_navigation_request("go to my env folder and stay there")
                    .unwrap()
                    .as_deref(),
                Some(env_root.canonicalize().unwrap().as_path())
            );
            assert_eq!(
                parse_navigation_request("navigate into my env folder")
                    .unwrap()
                    .as_deref(),
                Some(env_root.canonicalize().unwrap().as_path())
            );
            assert_eq!(
                parse_navigation_request("cd into my env folder")
                    .unwrap()
                    .as_deref(),
                Some(env_root.canonicalize().unwrap().as_path())
            );
            assert_eq!(
                parse_navigation_request("go into the env folder")
                    .unwrap()
                    .as_deref(),
                Some(env_root.canonicalize().unwrap().as_path())
            );
            assert_eq!(
                parse_navigation_request_from(
                    "enter the deepseek repo",
                    Some(isolated_root.path())
                )
                .unwrap(),
                None
            );
            assert_eq!(
                parse_navigation_request_from("navigate into deepseek", Some(isolated_root.path()))
                    .unwrap(),
                None
            );
            assert_eq!(
                parse_navigation_request_from("open the minimax repo", Some(isolated_root.path()))
                    .unwrap(),
                None
            );
            assert!(
                parse_navigation_request_from("cd into minimax", Some(isolated_root.path()))
                    .is_err()
            );
        }
        assert_eq!(
            parse_navigation_request("go through downloads").unwrap(),
            None
        );
        assert_eq!(parse_navigation_request("fix this repo").unwrap(), None);
        assert_eq!(
            parse_navigation_request("switch to main branch").unwrap(),
            None
        );
        assert_eq!(parse_navigation_request("stay in touch").unwrap(), None);
        assert_eq!(parse_navigation_request("open a ticket").unwrap(), None);
        assert!(parse_navigation_request("go to /definitely/not/here").is_err());
        assert!(parse_navigation_request("cd into definitely-not-here").is_err());
    }

    #[test]
    fn relative_navigation_uses_selected_root_as_base() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("sample_project");
        fs::create_dir_all(&child).unwrap();

        assert_eq!(
            parse_navigation_request_from("cd into sample_project", Some(root.path()))
                .unwrap()
                .as_deref(),
            Some(child.canonicalize().unwrap().as_path())
        );
    }

    #[test]
    fn arrow_chain_navigation_can_leave_trailing_task_for_agent() {
        let _guard = env_lock();
        let root = tempfile::tempdir().unwrap();
        let structure = root.path().join("tinygrad").join("structure");
        let home_structure = root.path().join("env").join("tinygrad").join("structure");
        let sibling_root = root.path().join("env").join("tinygrad-arkey");
        fs::create_dir_all(&structure).unwrap();
        fs::create_dir_all(&home_structure).unwrap();
        fs::create_dir_all(&sibling_root).unwrap();
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", root.path());

        assert_eq!(
            parse_navigation_request_from("go to tinygrad -> structure", Some(root.path()))
                .unwrap()
                .as_deref(),
            Some(structure.canonicalize().unwrap().as_path())
        );

        assert_eq!(
            parse_navigation_request_from(
                "go to tinygrad -> structure -> find your purpose",
                Some(root.path())
            )
            .unwrap(),
            None
        );
        assert_eq!(
            parse_navigation_request_from(
                "go to tinygrad -> structure -> what is your role?",
                Some(root.path())
            )
            .unwrap(),
            None
        );
        assert_eq!(
            parse_navigation_request_from(
                "go to tinygrad -> structure. find your purpose",
                Some(root.path())
            )
            .unwrap(),
            None
        );
        assert!(has_arrow_chain_trailing_task(
            "go to my env -> tinygrad -> structure. find your purpose"
        ));
        assert!(has_arrow_chain_trailing_task(
            "go to my env -> tinygrad -> structure -> find your purpose"
        ));
        assert!(!has_arrow_chain_trailing_task(
            "go to my env -> tinygrad -> structure"
        ));
        assert_eq!(
            parse_navigation_request_from(
                "go to my env -> tinygrad -> structure. find your purpose",
                Some(&sibling_root)
            )
            .unwrap(),
            None
        );
        assert_eq!(
            parse_navigation_request_from(
                "go to missing -> foo. find your purpose",
                Some(&sibling_root)
            )
            .unwrap(),
            None
        );
        assert_eq!(
            parse_navigation_request_from("go to tinygrad -> structure. list", Some(root.path()))
                .unwrap(),
            None
        );
        assert_eq!(
            parse_navigation_request_from("go to tinygrad -> structure -> list", Some(root.path()))
                .unwrap(),
            None
        );

        assert_eq!(
            infer_natural_root("go to my env -> tinygrad -> structure -> find your purpose")
                .unwrap()
                .as_path(),
            home_structure.canonicalize().unwrap().as_path()
        );
        assert_eq!(
            infer_natural_root("go to my env -> tinygrad -> structure. find your purpose")
                .unwrap()
                .as_path(),
            home_structure.canonicalize().unwrap().as_path()
        );

        assert!(
            parse_navigation_request_from("go to tinygrad -> missing", Some(root.path())).is_err()
        );
        assert!(parse_navigation_request_from(
            "go to tinygrad -> missing folder",
            Some(root.path())
        )
        .is_err());

        if let Some(previous_home) = previous_home {
            std::env::set_var("HOME", previous_home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn arrow_chain_task_can_fuzzy_match_directory_segments() {
        let _guard = env_lock();
        let root = tempfile::tempdir().unwrap();
        let structure = root.path().join("env").join("pkos_v0.2").join("structure");
        fs::create_dir_all(&structure).unwrap();
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", root.path());

        let resolved = infer_natural_root_with_source(
            "go to my env -> pkosv2 -> structure. find your purpose",
        )
        .unwrap();

        assert_eq!(
            resolved.path.as_path(),
            structure.canonicalize().unwrap().as_path()
        );
        assert_eq!(
            resolved.source,
            RootSource::Fuzzy {
                requested: "pkosv2".to_string(),
                matched: "pkos_v0.2".to_string(),
            }
        );

        if let Some(previous_home) = previous_home {
            std::env::set_var("HOME", previous_home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn fuzzy_navigation_rejects_ambiguous_matches() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("pkos-v2")).unwrap();
        fs::create_dir_all(root.path().join("pkos_v2")).unwrap();

        let err =
            parse_navigation_request_with_source("go to pkosv2", Some(root.path())).unwrap_err();

        assert!(err.contains("ambiguous"), "{err}");
    }

    #[test]
    fn explicit_navigation_and_root_commands_do_not_fuzzy_match() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("pkos_v0.2")).unwrap();

        assert!(parse_navigation_request_from("cd pkosv2", Some(root.path())).is_err());
        assert!(update_selected_root_from("pkosv2", Some(root.path())).is_err());
    }

    #[test]
    fn relative_navigation_supports_parent_and_current_directory() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("parent");
        let child = parent.join("child");
        let sibling = parent.join("sibling");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&sibling).unwrap();

        assert_eq!(
            parse_navigation_request_from("cd ..", Some(&child))
                .unwrap()
                .as_deref(),
            Some(parent.canonicalize().unwrap().as_path())
        );
        assert_eq!(
            parse_navigation_request_from("cd ../sibling", Some(&child))
                .unwrap()
                .as_deref(),
            Some(sibling.canonicalize().unwrap().as_path())
        );
        assert_eq!(
            parse_navigation_request_from("cd .", Some(&child))
                .unwrap()
                .as_deref(),
            Some(child.canonicalize().unwrap().as_path())
        );
        assert_eq!(
            update_selected_root_from("../sibling", Some(&child))
                .unwrap()
                .as_deref(),
            Some(sibling.canonicalize().unwrap().as_path())
        );
    }

    #[test]
    fn outside_root_clarify_suggests_parent_root() {
        let text = path_boundary_clarify_text(
            Path::new("/tmp/workspace"),
            Path::new("/Users/example/.ssh/config"),
        );
        assert!(text.contains("Suggested root: /Users/example/.ssh"));
        assert!(text.contains("Type /root /Users/example/.ssh"));
    }
}
