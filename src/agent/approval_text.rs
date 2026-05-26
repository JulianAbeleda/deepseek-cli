//! User-facing approval request text shared by interactive and docked agent paths.

pub(super) fn shell_summary(cwd: &str, reason: &str, command: &str) -> String {
    format!(
        "approval required: run_shell\ncwd: {cwd}\nreason: {reason}\ncommand: {command}\nType yes run to approve, n to deny.\n"
    )
}

pub(super) fn patch_summary(path: &str, reason: &str, find: &str, replace: &str) -> String {
    format!(
        "approval required: propose_patch\npath: {path}\nreason: {reason}\n--- find ---\n{find}\n--- replace ---\n{replace}\nType yes apply to approve, n to deny.\n"
    )
}

pub(super) fn create_file_summary(path: &str, reason: &str, content: &str) -> String {
    format!(
        "approval required: create_file\npath: {path}\nreason: {reason}\n--- content ---\n{content}\nType yes apply to approve, n to deny.\n"
    )
}

pub(super) fn with_root_note(summary: String, root_note: Option<&str>) -> String {
    match root_note.filter(|note| !note.trim().is_empty()) {
        Some(note) => format!("workspace route: {note}\n{summary}"),
        None => summary,
    }
}

#[cfg(test)]
mod tests {
    use super::{create_file_summary, patch_summary, shell_summary, with_root_note};

    #[test]
    fn shell_summary_keeps_modal_preview_fields() {
        let summary = shell_summary(".", "run tests", "cargo test --offline");

        assert_eq!(
            summary,
            "approval required: run_shell\ncwd: .\nreason: run tests\ncommand: cargo test --offline\nType yes run to approve, n to deny.\n"
        );
    }

    #[test]
    fn patch_summary_keeps_find_replace_sections() {
        let summary = patch_summary("README.md", "docs", "old", "new");

        assert_eq!(
            summary,
            "approval required: propose_patch\npath: README.md\nreason: docs\n--- find ---\nold\n--- replace ---\nnew\nType yes apply to approve, n to deny.\n"
        );
    }

    #[test]
    fn create_file_summary_keeps_content_preview() {
        let summary = create_file_summary("notes/new.md", "docs", "# New\n");

        assert_eq!(
            summary,
            "approval required: create_file\npath: notes/new.md\nreason: docs\n--- content ---\n# New\n\nType yes apply to approve, n to deny.\n"
        );
    }

    #[test]
    fn root_note_prefixes_approval_summary_when_present() {
        let summary = with_root_note(
            shell_summary(".", "run tests", "cargo test"),
            Some("matched: pkosv2 -> pkos_v0.2"),
        );

        assert!(summary.starts_with(
            "workspace route: matched: pkosv2 -> pkos_v0.2\napproval required: run_shell\n"
        ));
    }
}
