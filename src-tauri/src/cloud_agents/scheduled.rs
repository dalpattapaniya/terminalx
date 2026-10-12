//! Scheduled runs (PRO-37, terminalx-saas contract §24.4): the runtime takes
//! the run its workspace was woken for, starts one agent turn with the
//! schedule's prompt, says what became of it, and ends the turn itself when
//! it outlasts the schedule's limit.
//!
//! The server hands a run out once: the claim moves it to `working`, so a
//! claim whose answer is lost is never answered again and the run times out
//! there. What this side adds is `scheduled-run.json`, written before the
//! agent is touched, so a runtime that restarts in the middle of a run
//! reports it as failed instead of leaving the server to wait. The record
//! holds ids only; the prompt lives in memory and in the agent's transcript.
//!
//! A scheduled run has nobody watching it. It starts in the mode the
//! schedule stores and never in bypass permissions, and its prompt is held
//! to the rules of a person who may drive but not configure
//! (`slash::check`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::api::{CallError, HttpMailboxApi};
use super::launch::{valid_agent, StartError, MAX_PROMPT_BYTES};
use crate::events::TurnStatus;

const FILE: &str = "scheduled-run.json";
const CLAIM_PATH: &str = "/v1/cloud-workspace-bootstrap/scheduled-run/claim";
const REPORT_PATH: &str = "/v1/cloud-workspace-bootstrap/scheduled-run/report";
/// The modes a schedule may store: every launch mode but bypass permissions.
const MODES: [&str; 4] = ["plan", "manual", "auto", "acceptEdits"];
/// How often a report is tried again before it is left for the next pass.
const REPORT_ATTEMPTS: u32 = 5;

const TIMED_OUT: &str = "timed-out";
const CANCELED: &str = "canceled";
const RUNTIME_ERROR: &str = "runtime-error";
const AGENT_FAILED: &str = "agent-failed";
const AGENT_EXITED: &str = "agent-exited";
const AGENT_START_FAILED: &str = "agent-start-failed";
const AGENT_UNAVAILABLE: &str = "agent-unavailable";
const CREDENTIAL_REQUIRED: &str = "agent-credential-required";
const PROJECT_UNAVAILABLE: &str = "project-unavailable";

/// `run` of a claim response. No `Debug`: the prompt stays out of logs.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub run_id: String,
    #[serde(default)]
    pub name: String,
    pub agent: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    pub prompt: String,
    pub max_run_minutes: u32,
}

/// What one claim answered.
pub enum Claimed {
    Run(Box<Run>),
    /// A run was handed out but this runtime cannot read it.
    Unreadable(String),
    None,
    /// This server has no scheduled runs for this runtime: an older server
    /// or the feature turned off (`404`), or a credential it does not
    /// accept right now (`401`).
    NotOffered,
    /// Asked too often.
    Wait(Duration),
}

/// What the runtime reports, and keeps in `scheduled-run.json` to report again.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub run_id: String,
    /// `started`, `completed` or `failed`.
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Reported {
    Accepted,
    /// The server ended the run itself (canceled, timed out, already
    /// reported): `409`. Or `404`: it no longer knows the run, or scheduled
    /// runs were turned off.
    Settled(String),
}

pub trait ScheduledApi: Send + Sync {
    fn claim(&self) -> Result<Claimed, CallError>;
    fn report(&self, report: &Report) -> Result<Reported, CallError>;
}

impl ScheduledApi for HttpMailboxApi {
    fn claim(&self) -> Result<Claimed, CallError> {
        match self.call("POST", CLAIM_PATH, Some(json!({ "v": 1 }))) {
            Ok((200, body)) => match body.get("run") {
                None | Some(Value::Null) => Ok(Claimed::None),
                Some(run) => match serde_json::from_value::<Run>(run.clone()) {
                    Ok(run) => Ok(Claimed::Run(Box::new(run))),
                    Err(error) => match run.get("runId").and_then(Value::as_str) {
                        Some(run_id) => {
                            log::warn!("scheduled run {run_id}: unreadable: {error}");
                            Ok(Claimed::Unreadable(run_id.to_string()))
                        }
                        None => Err(CallError::Transient(anyhow!("scheduled run claim: unreadable response: {error}"))),
                    },
                },
            },
            Ok((404, _)) | Err(CallError::Rejected) => Ok(Claimed::NotOffered),
            Ok((429, _)) => Ok(Claimed::Wait(Duration::from_secs(60))),
            Ok((status, body)) => Err(CallError::Transient(anyhow!("scheduled run claim: HTTP {status} {}", error_code(&body)))),
            Err(error) => Err(error),
        }
    }

    fn report(&self, report: &Report) -> Result<Reported, CallError> {
        let mut body = serde_json::to_value(report).map_err(|error| CallError::Transient(anyhow!("scheduled run report: {error}")))?;
        body["v"] = json!(1);
        let state = |body: &Value| body.get("state").and_then(Value::as_str).unwrap_or("settled").to_string();
        match self.call("POST", REPORT_PATH, Some(body))? {
            (200, _) => Ok(Reported::Accepted),
            (409, body) => Ok(Reported::Settled(state(&body))),
            (404, _) => Ok(Reported::Settled("not-found".into())),
            (status, body) => Err(CallError::Transient(anyhow!("scheduled run report: HTTP {status} {}", error_code(&body)))),
        }
    }
}

fn error_code(body: &Value) -> &str {
    body.get("error").and_then(Value::as_str).unwrap_or("")
}

/// Where the run's turn stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Turn {
    /// The prompt was sent; the agent has not opened its turn yet.
    Pending,
    Running,
    Ended(TurnStatus),
    /// The agent process is gone and no turn end was recorded.
    Exited,
    /// The tab or its session was removed.
    Gone,
}

/// The agent tab of a run. The real one is [`ManagerAgent`]; tests stand in.
pub trait Agent: Send + Sync {
    /// Whether `agent` is offered and installed here, and can run `model`.
    fn available(&self, agent: &str, model: &str) -> bool;
    /// Whether `agent` would start with no way to sign in (PRO-78).
    fn sign_in_required(&self, _agent: &str) -> bool {
        false
    }
    /// Create the session and tab and send the prompt. Returns
    /// `(session id, tab id)`.
    fn start(&self, run: &Run, title: &str) -> Result<(String, String), StartError>;
    fn turn(&self, session_id: &str, tab_id: &str) -> Turn;
    /// Ask the agent to stop its turn.
    fn interrupt(&self, session_id: &str, tab_id: &str);
    /// End the agent process.
    fn stop(&self, session_id: &str, tab_id: &str);
}

/// `scheduled-run.json`: the run this runtime holds, if any.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "stage", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum Record {
    /// Claimed; the agent may or may not have been given the prompt.
    Claimed { run_id: String },
    Started { run_id: String, session_id: String, tab_id: String },
    /// Over; `report` is still owed to the server.
    Done { report: Report },
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn open(dir: &Path) -> Self {
        Self { path: dir.join(FILE) }
    }

    fn get(&self) -> Option<Record> {
        serde_json::from_slice(&std::fs::read(&self.path).ok()?).ok()
    }

    fn put(&self, record: &Record) -> anyhow::Result<()> {
        crate::cloud_bootstrap::write_durable(&self.path, &serde_json::to_vec(record)?)
    }

    fn clear(&self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("remove the scheduled run record: {error}");
            }
        }
    }
}

/// The waits of the runner; tests shorten them.
#[derive(Debug, Clone)]
pub struct Timing {
    /// Between claims while there is nothing to run.
    pub claim_every: Duration,
    /// Between claims of a server that offers no scheduled runs.
    pub not_offered_every: Duration,
    /// One of a run's `maxRunMinutes`.
    pub minute: Duration,
    /// How long an interrupted turn has to end before the agent is stopped.
    pub interrupt_grace: Duration,
    /// How long a sent prompt has to open a turn.
    pub start_grace: Duration,
    /// How often a running run asks the server whether it is still wanted.
    pub heartbeat: Duration,
    pub tick: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            claim_every: Duration::from_secs(30),
            not_offered_every: Duration::from_secs(5 * 60),
            minute: Duration::from_secs(60),
            interrupt_grace: Duration::from_secs(30),
            start_grace: Duration::from_secs(2 * 60),
            heartbeat: Duration::from_secs(60),
            tick: Duration::from_secs(1),
        }
    }
}

pub struct Runner {
    pub api: Arc<dyn ScheduledApi>,
    pub agent: Arc<dyn Agent>,
    pub store: Store,
    /// The project folder a run's agent starts in.
    pub root: PathBuf,
    pub timing: Timing,
    /// The runtime is about to be archived: take no new run.
    pub paused: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Counts as work for the idle monitor until the guard it returns
    /// drops ([`working`]); tests stand in.
    pub hold: Arc<dyn Fn() -> Box<dyn Send> + Send + Sync>,
}

/// The real [`Runner::hold`]: reported as a running turn, like a launch.
pub fn working() -> Box<dyn Send> {
    Box::new(crate::cloud_activity::launching())
}

impl Runner {
    /// Claim and run, for as long as the runtime lives.
    pub fn run(&self) {
        let mut failures = 0u32;
        loop {
            let wait = match self.pass() {
                Ok(wait) => {
                    failures = 0;
                    wait
                }
                Err(error) => {
                    failures = failures.saturating_add(1);
                    log::warn!("scheduled run: {error}");
                    crate::remote::protocol::backoff(failures, 0.5).max(Duration::from_secs(1))
                }
            };
            std::thread::sleep(wait);
        }
    }

    /// One pass: report what an earlier pass or process left, then claim
    /// and run. Returns how long to wait before the next one.
    pub fn pass(&self) -> Result<Duration, CallError> {
        self.settle_left_over()?;
        if (self.paused)() {
            return Ok(self.timing.claim_every);
        }
        match self.api.claim()? {
            Claimed::Run(run) => {
                self.execute(&run);
                // The next run of this workspace, if one is waiting.
                Ok(Duration::ZERO)
            }
            Claimed::Unreadable(run_id) => {
                self.finish(failed(&run_id, RUNTIME_ERROR, None));
                Ok(Duration::ZERO)
            }
            Claimed::None => Ok(self.timing.claim_every),
            Claimed::NotOffered => Ok(self.timing.not_offered_every),
            Claimed::Wait(wait) => Ok(wait),
        }
    }

    /// A record from before this pass: a run this process could not report,
    /// or one a restart cut short. That agent is gone with its process and
    /// the prompt may have reached it, so it is failed, never sent again.
    fn settle_left_over(&self) -> Result<(), CallError> {
        let report = match self.store.get() {
            None => return Ok(()),
            Some(Record::Done { report }) => report,
            Some(Record::Claimed { run_id }) => failed(&run_id, RUNTIME_ERROR, None),
            Some(Record::Started { run_id, session_id, tab_id }) => failed(&run_id, RUNTIME_ERROR, Some((session_id, tab_id))),
        };
        match self.api.report(&report) {
            Ok(outcome) => {
                log::info!("scheduled run {}: reported {} from its record: {outcome:?}", report.run_id, report.outcome);
                self.store.clear();
                Ok(())
            }
            // The credential is not accepted right now: the record waits,
            // and the claim below says how long.
            Err(CallError::Rejected) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn execute(&self, run: &Run) {
        let deadline = Instant::now() + self.timing.minute * run.max_run_minutes.max(1);
        // Counted as work until the turn is seen running: nobody is attached,
        // and a suspend between the claim and the turn would lose the run.
        let launching = (self.hold)();
        if let Err(error) = self.store.put(&Record::Claimed { run_id: run.run_id.clone() }) {
            // Without the durable mark a restart could not tell the server
            // what happened, so the agent is not touched.
            log::error!("record the scheduled run: {error:#}");
            return self.finish(failed(&run.run_id, RUNTIME_ERROR, None));
        }
        if let Some(category) = self.refusal(run) {
            log::warn!("scheduled run {}: not started: {category}", run.run_id);
            return self.finish(failed(&run.run_id, category, None));
        }
        let (session_id, tab_id) = match self.agent.start(run, &title_of(run)) {
            Ok(ids) => ids,
            Err(StartError::NotStarted(error)) => {
                log::warn!("scheduled run {}: start the agent: {error:#}", run.run_id);
                return self.finish(failed(&run.run_id, AGENT_START_FAILED, None));
            }
            Err(StartError::SendFailed { session_id, tab_id, error }) => {
                log::warn!("scheduled run {}: send the prompt: {error:#}", run.run_id);
                self.agent.stop(&session_id, &tab_id);
                return self.finish(failed(&run.run_id, AGENT_START_FAILED, Some((session_id, tab_id))));
            }
        };
        let ids = Some((session_id.clone(), tab_id.clone()));
        if let Err(error) = self.store.put(&Record::Started { run_id: run.run_id.clone(), session_id: session_id.clone(), tab_id: tab_id.clone() }) {
            log::warn!("record the scheduled run's tab: {error:#}");
        }
        let started = Report { run_id: run.run_id.clone(), outcome: "started".into(), category: None, session_id: Some(session_id.clone()), tab_id: Some(tab_id.clone()) };
        let mut launching = Some(launching);
        let sent = Instant::now();
        let mut next_beat = sent;
        loop {
            let now = Instant::now();
            if now >= next_beat {
                next_beat = now + self.timing.heartbeat;
                match self.api.report(&started) {
                    // The run is over on the server (the schedule was paused
                    // or deleted, or it gave up waiting): the turn ends here.
                    Ok(Reported::Settled(state)) => {
                        log::info!("scheduled run {}: {state} on the server; ending its turn", run.run_id);
                        self.end_turn(&session_id, &tab_id);
                        return self.store.clear();
                    }
                    Ok(Reported::Accepted) => {}
                    Err(error) => log::warn!("scheduled run {}: report started: {error}", run.run_id),
                }
            }
            let category = match self.agent.turn(&session_id, &tab_id) {
                Turn::Running => {
                    launching = None;
                    None
                }
                Turn::Pending if sent.elapsed() < self.timing.start_grace => None,
                Turn::Pending => {
                    self.agent.stop(&session_id, &tab_id);
                    Some(AGENT_START_FAILED)
                }
                Turn::Ended(TurnStatus::Ok) => {
                    return self.finish(Report { outcome: "completed".into(), ..started });
                }
                Turn::Ended(TurnStatus::Error) => Some(AGENT_FAILED),
                // Someone stopped the turn, or removed its tab.
                Turn::Ended(TurnStatus::Aborted) | Turn::Gone => Some(CANCELED),
                Turn::Exited => Some(AGENT_EXITED),
            };
            if let Some(category) = category {
                return self.finish(failed(&run.run_id, category, ids));
            }
            if Instant::now() >= deadline {
                log::warn!("scheduled run {}: over its {} minutes; ending its turn", run.run_id, run.max_run_minutes);
                self.end_turn(&session_id, &tab_id);
                return self.finish(failed(&run.run_id, TIMED_OUT, ids));
            }
            drop(launching.take_if(|_| sent.elapsed() >= self.timing.start_grace));
            std::thread::sleep(self.timing.tick);
        }
    }

    /// Why `run` is not started, as the category the server is told.
    fn refusal(&self, run: &Run) -> Option<&'static str> {
        if !valid_agent(&run.agent) || run.prompt.trim().is_empty() || run.prompt.len() > MAX_PROMPT_BYTES || run.prompt.contains('\0') {
            return Some(RUNTIME_ERROR);
        }
        // A tab started without a mode runs in bypass permissions, so a run
        // that names none, or one this runtime does not know, is not started.
        if !run.mode.as_deref().is_some_and(|mode| MODES.contains(&mode)) {
            return Some(RUNTIME_ERROR);
        }
        if !self.root.is_dir() {
            return Some(PROJECT_UNAVAILABLE);
        }
        if let Err(refusal) = super::slash::check(&run.prompt, &run.agent, Some(&self.root)) {
            return Some(refusal.category());
        }
        if !self.agent.available(&run.agent, run.model.as_deref().unwrap_or_default()) {
            return Some(AGENT_UNAVAILABLE);
        }
        // A prompt sent to an agent at its sign-in screen is never read.
        if self.agent.sign_in_required(&run.agent) {
            return Some(CREDENTIAL_REQUIRED);
        }
        None
    }

    /// Interrupt the turn; an agent that has not ended it after the grace
    /// is stopped.
    fn end_turn(&self, session_id: &str, tab_id: &str) {
        self.agent.interrupt(session_id, tab_id);
        let asked = Instant::now();
        while matches!(self.agent.turn(session_id, tab_id), Turn::Running | Turn::Pending) {
            if asked.elapsed() >= self.timing.interrupt_grace {
                self.agent.stop(session_id, tab_id);
                return;
            }
            std::thread::sleep(self.timing.tick.min(self.timing.interrupt_grace));
        }
    }

    /// Record the end durably, then report it. A report that does not get
    /// through stays on record for the next pass.
    fn finish(&self, report: Report) {
        if let Err(error) = self.store.put(&Record::Done { report: report.clone() }) {
            log::error!("record the scheduled run's end: {error:#}");
        }
        for attempt in 1..=REPORT_ATTEMPTS {
            match self.api.report(&report) {
                Ok(outcome) => {
                    log::info!("scheduled run {}: {} {}: {outcome:?}", report.run_id, report.outcome, report.category.as_deref().unwrap_or(""));
                    return self.store.clear();
                }
                Err(error) => {
                    log::warn!("scheduled run {}: report {}: {error}", report.run_id, report.outcome);
                    if attempt < REPORT_ATTEMPTS {
                        std::thread::sleep(self.timing.tick * attempt);
                    }
                }
            }
        }
    }
}

fn failed(run_id: &str, category: &str, ids: Option<(String, String)>) -> Report {
    let (session_id, tab_id) = ids.unzip();
    Report { run_id: run_id.into(), outcome: "failed".into(), category: Some(category.into()), session_id, tab_id }
}

fn title_of(run: &Run) -> String {
    let line = run.name.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        "Scheduled task".into()
    } else if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line.to_string()
    }
}

/// The tab a run starts: the schedule's agent, model, effort and mode.
pub(crate) fn new_tab(run: &Run) -> crate::session_ops::NewTab {
    crate::session_ops::NewTab {
        harness: run.agent.clone(),
        model: run.model.clone().unwrap_or_default(),
        effort: run.effort.clone(),
        permission_mode: run.mode.clone(),
    }
}

/// The tab of the run in hand, and how its turn ended once it has.
type Watched = Arc<Mutex<Option<(String, Option<TurnStatus>)>>>;

/// The agent tab, created the way `session.create` creates one.
pub struct ManagerAgent {
    manager: crate::session::SessionManager,
    sink: Arc<dyn crate::sink::EventSink>,
    root: String,
    agents: std::sync::Weak<super::CloudAgents>,
    watched: Watched,
}

impl ManagerAgent {
    pub fn new(manager: crate::session::SessionManager, sink: Arc<dyn crate::sink::EventSink>, root: String, agents: std::sync::Weak<super::CloudAgents>) -> Self {
        let watched = Watched::default();
        let seen = watched.clone();
        sink.listen(
            "agent_event",
            Box::new(move |payload| {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Event {
                    tab_id: String,
                    payload: Tagged,
                }
                #[derive(Deserialize)]
                struct Tagged {
                    #[serde(rename = "type")]
                    kind: String,
                    #[serde(default)]
                    status: Option<TurnStatus>,
                }
                // Listeners run on the emitting thread: only note the end.
                let Ok(event) = serde_json::from_str::<Event>(payload) else { return };
                if event.payload.kind != "turn_completed" {
                    return;
                }
                let mut watched = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some((tab_id, ended @ None)) = watched.as_mut() {
                    if *tab_id == event.tab_id {
                        *ended = Some(event.payload.status.unwrap_or(TurnStatus::Error));
                    }
                }
            }),
        );
        Self { manager, sink, root, agents, watched }
    }
}

impl Agent for ManagerAgent {
    fn available(&self, agent: &str, model: &str) -> bool {
        crate::harness::offered().into_iter().any(|harness| harness.id == agent && harness.available) && crate::harness::claude::models::validate(agent, model).is_ok()
    }

    fn sign_in_required(&self, agent: &str) -> bool {
        crate::cloud_grants::sign_in_required_at_launch(agent).is_some()
    }

    fn start(&self, run: &Run, title: &str) -> Result<(String, String), StartError> {
        let entry = crate::session_ops::create_session_blocking(
            &*self.sink,
            crate::session_ops::NewSession {
                project_path: self.root.clone(),
                title: Some(title.to_string()),
                use_worktree: false,
                on_main: true,
                base_ref: None,
                worktree_name: None,
                issue: None,
                automation: None,
                cwd: None,
                tab: Some(new_tab(run)),
            },
        )
        .map_err(|error| StartError::NotStarted(anyhow!("{error}")))?;
        let tab = entry.tabs.first().cloned().ok_or_else(|| StartError::NotStarted(anyhow!("the new session has no tab")))?;
        *self.watched.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((tab.id.clone(), None));
        if let Err(error) = self.manager.send(&entry.id, &tab.id, run.prompt.clone(), Vec::new()) {
            return Err(StartError::SendFailed { session_id: entry.id, tab_id: tab.id, error });
        }
        if let Some(agents) = self.agents.upgrade() {
            agents.changed(Some(&tab.id), true);
        }
        Ok((entry.id, tab.id))
    }

    fn turn(&self, session_id: &str, tab_id: &str) -> Turn {
        let ended = self.watched.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().filter(|(watched, _)| watched == tab_id).and_then(|(_, ended)| *ended);
        if let Some(status) = ended {
            return Turn::Ended(status);
        }
        if !crate::store::index::get(session_id).is_ok_and(|entry| entry.tab(tab_id).is_some()) {
            return Turn::Gone;
        }
        if self.manager.turn_open(session_id, tab_id) {
            Turn::Running
        } else if self.manager.is_running(session_id, tab_id) {
            Turn::Pending
        } else {
            Turn::Exited
        }
    }

    fn interrupt(&self, session_id: &str, tab_id: &str) {
        if let Err(error) = self.manager.interrupt(session_id, tab_id) {
            log::warn!("interrupt the scheduled run's turn: {error:#}");
        }
    }

    fn stop(&self, session_id: &str, tab_id: &str) {
        if let Err(error) = self.manager.stop(session_id, tab_id) {
            log::warn!("stop the scheduled run's agent: {error:#}");
        }
    }
}

#[cfg(test)]
#[path = "scheduled_tests.rs"]
mod tests;
