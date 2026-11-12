//! `yate skill`: instructions for AI agents that configure yate, and
//! `yate skill --init`, which installs them as an agent skill (`/yate`).
//!
//! The skill goes to the git root found from the working directory up to the
//! home directory, so the agent sees it in every folder of the repository.
//! Outside a repository it goes to the home directory, for every project.

use std::path::{Path, PathBuf};

const SKILL: &str = include_str!("SKILL.md");
const AGENT_INSTRUCTIONS: &str = include_str!("agent-instructions.txt");
const CONFIGURATION: &str = include_str!("../../docs/configuration.md");
const PLUGINS: &str = include_str!("../../docs/plugins.md");
const SKILL_NAME: &str = "yate";
const SKILL_DIRS: [(&str, &str); 2] = [
    (".agents/skills", "Codex, Cursor"),
    (".claude/skills", "Claude Code, Cursor"),
];

/// What `yate skill` prints: a preamble for agents, the `yate.json` and
/// plugin references, and the template with every setting and its default.
pub fn instructions() -> String {
    format!(
        "{AGENT_INSTRUCTIONS}\n{CONFIGURATION}\n{PLUGINS}\n# Every setting with its default (yate init)\n\n```jsonc\n{}```\n",
        crate::settings::template()
    )
}

/// Writes the skill for `yate skill --init` and returns the lines to print.
/// Refuses, writing nothing, when one of the skill files exists.
pub fn init(cwd: &Path, home: Option<&Path>) -> Result<Vec<String>, String> {
    let (root, in_repository) = match git_root(cwd, home) {
        Some(root) => (root, true),
        None => (
            home.ok_or("not in a git repository and the home directory is unknown (set HOME)")?
                .to_path_buf(),
            false,
        ),
    };
    let files: Vec<(PathBuf, &str)> = SKILL_DIRS
        .iter()
        .map(|(dir, read_by)| (Path::new(dir).join(SKILL_NAME).join("SKILL.md"), *read_by))
        .collect();
    if let Some((existing, _)) = files.iter().find(|(path, _)| root.join(path).exists()) {
        return Err(format!(
            "{} already exists; delete it to create the skill again",
            root.join(existing).display()
        ));
    }
    for (path, _) in &files {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("{}: {err}", parent.display()))?;
        }
        std::fs::write(&file, SKILL).map_err(|err| format!("{}: {err}", file.display()))?;
    }
    let mut lines = vec![if in_repository {
        format!("Project root: {} (git root)", root.display())
    } else {
        format!(
            "Not in a git repository: installed for all projects in {}",
            root.display()
        )
    }];
    lines.extend(
        files
            .iter()
            .map(|(path, read_by)| format!("Created {}  ({read_by})", path.display())),
    );
    lines.push(String::new());
    lines.push("Try: /yate use the gruvbox-dark theme and a tab size of 4".to_string());
    Ok(lines)
}

fn git_root(cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let mut dir = cwd;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        if Some(dir) == home {
            return None;
        }
        dir = dir.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("yate-skill-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn instructions_hold_the_references_and_the_template() {
        let text = instructions();
        assert!(text.starts_with("yate - instructions for AI agents"));
        assert!(text.contains("# Configuration: yate.json"));
        assert!(text.contains("# Plugins and configuration"));
        assert!(text.contains(&crate::settings::template()));
    }

    #[test]
    fn init_writes_the_skill_at_the_git_root_once() {
        let home = temp_dir("repo");
        let repo = home.join("code/app");
        let nested = repo.join("src/lib");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let lines = init(&nested, Some(&home)).unwrap();
        assert_eq!(
            lines[0],
            format!("Project root: {} (git root)", repo.display())
        );
        for dir in [".agents/skills", ".claude/skills"] {
            let skill = std::fs::read_to_string(repo.join(dir).join("yate/SKILL.md")).unwrap();
            assert_eq!(skill, SKILL);
        }
        assert!(!nested.join(".claude").exists());
        assert!(!home.join(".claude").exists());

        let err = init(&repo, Some(&home)).unwrap_err();
        assert!(err.ends_with(
            ".agents/skills/yate/SKILL.md already exists; delete it to create the skill again"
        ));
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn init_outside_a_repository_writes_the_skill_to_the_home_directory() {
        let home = temp_dir("home");
        let notes = home.join("notes");
        std::fs::create_dir_all(&notes).unwrap();

        let lines = init(&notes, Some(&home)).unwrap();
        assert!(lines[0].starts_with("Not in a git repository"));
        assert!(home.join(".agents/skills/yate/SKILL.md").exists());
        assert!(home.join(".claude/skills/yate/SKILL.md").exists());
        assert!(!notes.join(".claude").exists());
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn the_skill_names_itself_yate() {
        assert!(SKILL.starts_with("---\nname: yate\ndescription: "));
    }
}
