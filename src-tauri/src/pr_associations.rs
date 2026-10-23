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

pub fn for_session(session: &str, mut additions: Vec<Association>) -> Result<Vec<Association>> {
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
