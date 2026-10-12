//! A chat follows its agent between projects.
//!
//! A chat runs in the store's Chats folder, but its agent can be asked to
//! work anywhere: create a branch in one repo, edit files in another. After
//! each tool call the CLI reports (`PostToolUse`), the folders the call
//! worked in are read off its input (a file it wrote, a `cd`, a `git -C`),
//! and the last one inside a git repo becomes the chat's link: that repo,
//! registered as a project if it is not one yet, and the branch checked out
//! there. A later call in another repo moves the link there.
//!
//! A conversation started in a project's own checkout (no worktree) keeps
//! its project, but records the branch its agent last worked on: the
//! checkout is shared, and another conversation switching its branch must
//! not move this one's.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;

use crate::git;
use crate::store::{self, index::ChatLink, index::SessionEntry};

/// Follow one tool call: the chat's entry when its link changed, else nothing.
pub fn follow(session_id: &str, payload: &Value) -> Result<Option<SessionEntry>> {
    let entry = store::index::get(session_id)?;
    let root = canonical(&store::root()?);
    if canonical(Path::new(&entry.project_path)) != root.join("Chats") {
        if entry.worktree_name.is_some() && !entry.worktree_removed {
            return Ok(None);
        }
        let link = ChatLink { path: entry.project_path.clone(), branch: git::current_branch(Path::new(&entry.cwd)) };
        return record(&entry, link);
    }
    let cwd = payload["cwd"].as_str().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(&entry.cwd));
    let input = &payload["tool_input"];
    let found = targets(input)
        .iter()
        .rev()
        .map(|target| resolve(target, &cwd))
        .filter_map(|path| repo_of(&path).map(|repo| (repo, path)))
        .find(|(repo, _)| !repo.starts_with(&root));
    let link = match (found, &entry.link) {
        (Some((repo, at)), _) => {
            store::projects::ensure(&repo.to_string_lossy())?;
            if at.starts_with(git::worktree_root(&repo)) {
                git::ensure_worktree_dir_ignored(&repo);
            }
            ChatLink { path: repo.to_string_lossy().into_owned(), branch: branch_at(&at) }
        }
        // No folder named, but a git command in the linked repo's shell may
        // still have switched its branch.
        (None, Some(link)) if command_of(input).contains("git") && link.branch == git::current_branch(Path::new(&link.path)) => return Ok(None),
        (None, Some(link)) if command_of(input).contains("git") => ChatLink { path: link.path.clone(), branch: git::current_branch(Path::new(&link.path)) },
        (None, _) => return Ok(None),
    };
    record(&entry, link)
}

/// Store a conversation's link: its entry when that changed it, else nothing.
fn record(entry: &SessionEntry, link: ChatLink) -> Result<Option<SessionEntry>> {
    if entry.link.as_ref() == Some(&link) {
        return Ok(None);
    }
    store::index::update_session(&entry.id, |session| {
        session.link = Some(link);
        Ok(session.clone())
    })
    .map(Some)
}

/// The folders and files a tool call names, in the order it names them.
fn targets(input: &Value) -> Vec<String> {
    let mut out: Vec<String> = ["file_path", "notebook_path", "path"].iter().filter_map(|key| input[*key].as_str().map(String::from)).collect();
    let command = command_of(input);
    let words = words(&command);
    for (index, word) in words.iter().enumerate() {
        let next = words.get(index + 1);
        match word.as_str() {
            "cd" | "pushd" => out.extend(next.cloned()),
            "-C" if index > 0 && words[index - 1] == "git" => out.extend(next.cloned()),
            _ => {}
        }
    }
    // Codex edits by patch: each file it touches heads its own section.
    let patch = input["patch"].as_str().or(input["input"].as_str()).unwrap_or(&command).to_string();
    for line in patch.lines() {
        for marker in ["*** Update File: ", "*** Add File: ", "*** Delete File: "] {
            if let Some(path) = line.strip_prefix(marker) {
                out.push(path.trim().to_string());
            }
        }
    }
    out
}

/// A shell command as one string, whether the CLI gives it whole or as argv.
fn command_of(input: &Value) -> String {
    match &input["command"] {
        Value::String(command) => command.clone(),
        Value::Array(argv) => argv.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

/// A command's words, split at spaces and shell separators, quotes removed.
fn words(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    for ch in command.chars() {
        match (quote, ch) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => quote = Some(ch),
            (None, c) if c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')') => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            (None, c) => word.push(c),
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// A named path made absolute: `~` is the home folder, a relative one is under the call's folder.
fn resolve(target: &str, cwd: &Path) -> PathBuf {
    if let Some(rest) = target.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if target == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    cwd.join(target)
}

/// The repo a path is in, from its nearest folder that exists: the main
/// checkout's root, so work in one of its worktrees is work in the project.
fn repo_of(path: &Path) -> Option<PathBuf> {
    let dir = path.ancestors().find(|candidate| candidate.is_dir())?;
    let top = git::run(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    let top = canonical(Path::new(top.trim()));
    let common = git::run(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok();
    let main = common
        .map(|common| PathBuf::from(common.trim()))
        .filter(|common| common.file_name().is_some_and(|name| name == ".git"))
        .and_then(|common| common.parent().map(canonical));
    Some(main.unwrap_or(top))
}

/// The branch checked out where a path is: in a worktree, the worktree's own.
fn branch_at(path: &Path) -> Option<String> {
    let dir = path.ancestors().find(|candidate| candidate.is_dir())?;
    git::current_branch(dir)
}

/// What a chat's agent is told in the Chats folder (Claude Code reads
/// CLAUDE.md there, Codex AGENTS.md): a new branch is a worktree, so the
/// reader's checkout and every other conversation in it stay where they are.
pub fn instructions() -> String {
    let settings = store::settings::load();
    let dir = settings.worktree_dir.trim_end_matches('/');
    let prefix = settings.branch_prefix;
    format!(
        "# Working in the user's repos\n\n\
This is a TerminalX chat. Its folder only holds this note: do the work in the repo the user names.\n\n\
When asked to start a new branch in a repo, make it a git worktree, never `git checkout -b` or `git switch -c` in the repo's own checkout. \
Other conversations may be working in that checkout, and switching its branch moves them too.\n\n\
```sh\n\
cd <repo>\n\
git worktree add {dir}/<name> -b {prefix}<name>\n\
```\n\n\
Name it after the work, in lowercase-with-dashes. Then work inside `<repo>/{dir}/<name>`, and tell the user the branch and the folder.\n\
Only check out a branch in the repo's own checkout when the user asks for exactly that.\n"
    )
}

/// Write the agents' note into the Chats folder, when it is missing or out of date.
pub fn ensure_instructions(chats: &Path) -> Result<()> {
    let text = instructions();
    for name in ["CLAUDE.md", "AGENTS.md"] {
        let path = chats.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, &text)?;
        }
    }
    Ok(())
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_the_folders_a_call_works_in() {
        assert_eq!(targets(&json!({ "file_path": "/repo/src/a.rs" })), vec!["/repo/src/a.rs"]);
        assert_eq!(
            targets(&json!({ "command": "cd ~/work/solana-school && git checkout -b feature/new-work" })),
            vec!["~/work/solana-school"]
        );
        assert_eq!(targets(&json!({ "command": "git -C \"/a b/repo\" status; ls /tmp" })), vec!["/a b/repo"]);
        assert_eq!(targets(&json!({ "command": ["bash", "-lc", "cd web && npm test"] })), vec!["web"]);
        assert_eq!(targets(&json!({ "input": "*** Begin Patch\n*** Update File: api/src/main.rs\n*** End Patch" })), vec!["api/src/main.rs"]);
        assert!(targets(&json!({ "command": "ls -la" })).is_empty());
    }

    #[test]
    fn resolves_home_and_relative_paths() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(resolve("~/x", Path::new("/c")), home.join("x"));
        assert_eq!(resolve("web", Path::new("/c")), PathBuf::from("/c/web"));
        assert_eq!(resolve("/abs", Path::new("/c")), PathBuf::from("/abs"));
    }

    #[test]
    fn finds_the_repo_of_a_file_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        git::run(dir.path(), &["init", "-q"]).unwrap();
        let repo = repo_of(&dir.path().join("src/new/file.rs")).unwrap();
        assert_eq!(repo, canonical(dir.path()));
        assert!(repo_of(Path::new("/")).is_none());
    }

    #[test]
    fn work_in_a_worktree_is_work_in_its_project_on_the_worktree_branch() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        git::run(repo, &["init", "-q", "-b", "main"]).unwrap();
        git::run(repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "init"]).unwrap();
        let tree = repo.join(".raccoon/worktrees/dev");
        git::run(repo, &["worktree", "add", "-q", tree.to_str().unwrap(), "-b", "raccoon/dev"]).unwrap();
        assert_eq!(repo_of(&tree.join("src/a.rs")).unwrap(), canonical(repo));
        assert_eq!(branch_at(&tree.join("src/a.rs")).as_deref(), Some("raccoon/dev"));
        assert_eq!(branch_at(repo).as_deref(), Some("main"));
    }
}
