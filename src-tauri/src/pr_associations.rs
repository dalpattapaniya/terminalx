//! Session PR identities, independent of the session index and checkout lifetime.
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Scope {
    Local { cwd: String },
    Cloud { workspace: String, repo: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Association {
    pub url: String,
    pub number: u64,
    pub head: String,
    pub scope: Scope,
}

/// A canonical URL is both repository identity and PR identity (numbers alone aren't).
pub fn identity(raw: &str) -> Result<String> {
    let url = url::Url::parse(raw)?;
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some()
        || parts.len() != 4 || parts[2] != "pull" || parts[3].parse::<u64>().ok().filter(|n| *n > 0).is_none()
        || parts[..2].iter().any(|s| s.is_empty()) || url.query().is_some() || url.fragment().is_some() {
        bail!("Expected an HTTPS pull request URL (owner/repository/pull/number)");
    }
    Ok(format!("{}/{}/{}/pull/{}", url.origin().ascii_serialization(), parts[0].to_ascii_lowercase(), parts[1].to_ascii_lowercase(), parts[3].parse::<u64>()?))
}

type Associations = BTreeMap<String, BTreeMap<String, Association>>;

fn update(path: &Path, session: &str, additions: Vec<Association>) -> Result<Vec<Association>> {
    if session.is_empty() { bail!("A session is required to associate a pull request"); }
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut all: Associations = crate::store::read_json(path)?.unwrap_or_default();
    let saved = all.entry(session.to_string()).or_default();
    let mut changed = false;
    for mut entry in additions {
        entry.url = identity(&entry.url)?;
        if entry.url.rsplit('/').next().and_then(|n| n.parse::<u64>().ok()) != Some(entry.number) {
            bail!("Pull request number does not match its URL");
        }
        // Keep the original transport locator: pipeline migration may name the
        // project root whereas the panel already knows the live workspace.
        if let std::collections::btree_map::Entry::Vacant(slot) = saved.entry(entry.url.clone()) {
            slot.insert(entry);
            changed = true;
        }
    }
    let result = saved.values().cloned().collect();
    if changed { crate::store::write_json(path, &all)?; }
    Ok(result)
}

pub fn record_local(session: &str, cwd: &Path, url: &str, head: &str) -> Result<()> {
    let url = identity(url)?;
    let number = url.rsplit('/').next().unwrap().parse()?;
    update(&crate::store::root()?.join("session-pr-associations.json"), session, vec![Association {
        url, number, head: head.into(), scope: Scope::Local { cwd: cwd.to_string_lossy().into_owned() },
    }])?;
    Ok(())
}

/// The normalized journal is written even with no panel mounted. Recover only
/// successful creation tool calls, never PR links in prose, reviews or reads.
/// Reading it here also repairs associations after a crash or an older version.
fn creation_evidence(events: impl IntoIterator<Item = crate::events::AgentEvent>, session: &str, cwd: &str) -> Vec<Association> {
    use crate::events::{Payload, ToolType};
    let mut calls = BTreeMap::new();
    let mut found = Vec::new();
    for event in events {
        if event.session_id != session { continue; }
        let key = |id: &str| (event.tab_id.clone(), event.subagent.as_ref().map(|s| s.id.clone()), id.to_string());
        match event.payload {
            Payload::ToolCallStarted { call_id, tool_type: ToolType::Shell, input, .. } => {
                let command = input.get("cmd").or_else(|| input.get("command")).and_then(|v| v.as_str()).unwrap_or_default();
                if creates_pr(command) { calls.insert(key(&call_id), ()); }
            }
            Payload::ToolCallCompleted { call_id, result } if calls.remove(&key(&call_id)).is_some() && !result.is_error && result.exit_code.is_none_or(|code| code == 0) => {
                // gh prints the created URL on its own line. Do not extract
                // arbitrary links from a command's echoed arguments or prose.
                let urls: std::collections::BTreeSet<_> = result.text.lines().filter_map(|line| identity(line.trim()).ok()).collect();
                if urls.len() == 1 {
                    let url = urls.into_iter().next().unwrap();
                    found.push(Association { number: url.rsplit('/').next().unwrap().parse().unwrap(), url, head: String::new(), scope: Scope::Local { cwd: cwd.into() } });
                }
            }
            _ => {}
        }
    }
    found
}

/// Recognize a shell command, not quoted documentation or an echo of one.
/// Deliberately leave substitutions and heredocs to explicit association.
fn creates_pr(command: &str) -> bool {
    if command.contains("<<") || command.contains('`') || command.contains("$(") { return false; }
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut found = false;
    let matches = |words: &[String]| words.len() >= 3 && words[0].rsplit('/').next() == Some("gh") && words[1] == "pr" && words[2] == "create";
    for c in command.chars().chain(std::iter::once(';')) {
        if escaped { word.push(c); escaped = false; continue; }
        if c == '\\' && quote != Some('\'') { escaped = true; continue; }
        if let Some(q) = quote {
            if c == q { quote = None; } else { word.push(c); }
            continue;
        }
        if c == '\'' || c == '"' { quote = Some(c); continue; }
        if c.is_whitespace() || matches!(c, ';' | '&' | '|') {
            if !word.is_empty() { words.push(std::mem::take(&mut word)); }
            if matches!(c, ';' | '&' | '|' | '\n') {
                found |= matches(&words);
                words.clear();
            }
        } else { word.push(c); }
    }
    found && quote.is_none()
}

fn journal_associations(session: &str) -> Result<Vec<Association>> {
    let Ok(entry) = crate::store::index::get(session) else { return Ok(Vec::new()); };
    let mut found = Vec::new();
    for file in std::fs::read_dir(crate::store::session_dir(session)?)? {
        let path = file?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") { continue; }
        let events: Vec<crate::events::AgentEvent> = crate::store::read_lines(&path)?;
        found.extend(creation_evidence(events, session, &entry.cwd));
    }
    Ok(found)
}

pub fn for_session(session: &str, mut additions: Vec<Association>) -> Result<Vec<Association>> {
    additions.extend(journal_associations(session)?);
    // Read existing author evidence, never infer authorship from a shared cwd
    // or import the reviewer's PRs. Retain it beyond pipeline history pruning.
    for pipeline in crate::issue_pipeline::store::list()?.into_iter().filter(|p| p.author.session_id == session) {
        let urls = pipeline.pr.iter().map(|pr| pr.url.as_str()).chain(
            pipeline.events.iter().filter(|event| event.kind == "pullRequest").filter_map(|event| event.url.as_deref()),
        );
        for url in urls {
            let canonical = identity(url)?;
            let number = canonical.rsplit('/').next().unwrap().parse()?;
            additions.push(Association {
                url: canonical, number, head: pipeline.branch.clone(),
                scope: Scope::Local { cwd: pipeline.project_path.clone() },
            });
        }
    }
    update(&crate::store::root()?.join("session-pr-associations.json"), session, additions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(repo: &str, number: u64, head: &str) -> Association {
        Association { url: format!("https://github.com/acme/{repo}/pull/{number}"), number, head: head.into(), scope: Scope::Local { cwd: "/fixture".into() } }
    }

    #[test]
    fn recovers_creation_from_durable_tools_without_branch_or_panel_state() {
        use crate::events::{AgentEvent, Payload, ToolResult, ToolType};
        let event = |session: &str, payload| AgentEvent { id: "event".into(), session_id: session.into(), tab_id: "tab".into(), harness: "codex".into(), seq: 1, ts: String::new(), message_ts: None, subagent: None, payload };
        let call = |id: &str, command: &str| Payload::ToolCallStarted { call_id: id.into(), name: "exec_command".into(), tool_type: ToolType::Shell, input: serde_json::json!({ "cmd": command }), title: None };
        let result = |id: &str, text: &str, is_error| Payload::ToolCallCompleted { call_id: id.into(), result: ToolResult { text: text.into(), is_error, exit_code: Some(if is_error { 1 } else { 0 }), ..Default::default() } };
        let url = "https://github.com/acme/widgets/pull/2";
        let events = vec![
            event("author", call("create", "cd /fixture && gh pr create --head followup --title 'Fix'")),
            event("author", result("create", url, false)),
            event("author", call("read", "gh pr view 99")),
            event("author", result("read", "https://github.com/acme/widgets/pull/99", false)),
            event("author", call("failed", "gh pr create")),
            event("author", result("failed", "https://github.com/acme/widgets/pull/98", true)),
            event("other", call("other", "gh pr create")),
            event("other", result("other", "https://github.com/acme/widgets/pull/97", false)),
            event("author", Payload::AssistantText { block: None, text: "Review https://github.com/acme/widgets/pull/96".into() }),
        ];
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("tab.jsonl");
        for e in &events { crate::store::append_line(&journal, &serde_json::to_string(e).unwrap()).unwrap(); }
        let recovered = creation_evidence(crate::store::read_lines::<AgentEvent>(&journal).unwrap(), "author", "/different-checkout");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].url, url);
        let registry = dir.path().join("associations.json");
        update(&registry, "author", recovered).unwrap();
        std::fs::remove_file(journal).unwrap();
        assert_eq!(update(&registry, "author", vec![]).unwrap()[0].url, url);
        assert!(update(&registry, "other", vec![]).unwrap().is_empty());
    }

    #[test]
    fn creation_commands_are_not_quoted_mentions_or_read_commands() {
        for command in ["gh pr create --title 'a; gh pr view 3'", "cd /repo && gh pr create", "git switch followup\ngh pr create --draft"] {
            assert!(creates_pr(command), "{command}");
        }
        for command in ["echo 'gh pr create'", "printf '%s' 'example; gh pr create'", "gh pr view 3", "cat <<EOF\ngh pr create\nEOF"] {
            assert!(!creates_pr(command), "{command}");
        }
    }

    #[test]
    fn retains_followups_across_reloads_without_crossing_sessions_or_repositories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prs.json");
        update(&path, "one", vec![entry("a", 1, "initial"), entry("a", 2, "initial")]).unwrap();
        update(&path, "one", vec![entry("a", 3, "followup"), entry("b", 1, "followup")]).unwrap();
        update(&path, "two", vec![entry("c", 1, "initial")]).unwrap();
        let mut duplicate = entry("a", 1, "initial");
        duplicate.url = "https://github.com/ACME/A/pull/1/".into();
        assert_eq!(update(&path, "one", vec![duplicate]).unwrap().len(), 4);
        assert_eq!(update(&path, "one", vec![]).unwrap().len(), 4);
        assert_eq!(update(&path, "two", vec![]).unwrap().len(), 1);
    }

    #[test]
    fn concurrent_additions_are_unioned_and_invalid_updates_do_not_replace_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prs.json");
        std::thread::scope(|s| {
            for number in 1..=8 {
                let path = &path;
                s.spawn(move || update(path, "one", vec![entry("a", number, "topic")]).unwrap());
            }
        });
        assert_eq!(update(&path, "one", vec![]).unwrap().len(), 8);
        let mut invalid = entry("a", 9, "topic");
        invalid.number = 10;
        assert!(update(&path, "one", vec![invalid]).is_err());
        assert_eq!(update(&path, "one", vec![]).unwrap().len(), 8);
        for url in ["file:///a/b/pull/1", "https://github.com/a/b/issues/1", "https://user:secret@github.com/a/b/pull/1"] {
            assert!(identity(url).is_err());
        }
    }
}
