//! Typed session adapter for an attached desktop. Reads are fallible too: a
//! disconnected owner is never represented as an idle tab or an empty queue.
//! The client pins one owner and never retries a mutation after a disconnect.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::client::Client;
use crate::events::AgentEvent;
use crate::session::{HandoffInfo, ImageInput, PendingPermission, QueuedMessage, RunningTab, SendOutcome, TabPtyEvent};
use crate::store::index::TabStatus;

#[derive(Clone)]
pub struct Sessions(Backend);

#[derive(Clone)]
enum Backend {
    Owner(Arc<Client>),
    // Transitional desktop ownership, removed with the final startup cutover.
    // Both paths use the same admitted native operations and fallible reads.
    #[cfg(feature = "desktop")]
    InProcess(crate::session::SessionManager),
}

impl Sessions {
    pub fn new(client: Arc<Client>) -> Self { Self(Backend::Owner(client)) }

    #[cfg(feature = "desktop")]
    pub(crate) fn in_process(manager: crate::session::SessionManager) -> Self { Self(Backend::InProcess(manager)) }

    fn call<T: DeserializeOwned>(&self, command: &str, params: Value) -> Result<T> {
        match &self.0 {
            Backend::Owner(client) => client.call(command, params),
            #[cfg(feature = "desktop")]
            Backend::InProcess(manager) => {
                let method = command.strip_prefix("runtime.session.").ok_or_else(|| anyhow::anyhow!("unsupported desktop session operation"))?;
                Ok(serde_json::from_value(super::calls::session(manager, method, params)?)?)
            }
        }
    }

    fn tab<T: DeserializeOwned>(&self, method: &str, session: &str, tab: &str, mut params: Value) -> Result<T> {
        params["sessionId"] = json!(session);
        params["tabId"] = json!(tab);
        self.call(&format!("runtime.session.{method}"), params)
    }

    pub fn automations(&self) -> Result<Vec<crate::automations::Automation>> {
        self.call("runtime.session.automations", json!({}))
    }

    pub fn automation_runs(&self, id: &str) -> Result<Vec<crate::automations::AutomationRun>> {
        self.call("runtime.session.automationRuns", json!({"id":id}))
    }

    pub fn automation_issues(&self) -> Result<Vec<crate::automations::AutomationIssueState>> {
        self.call("runtime.session.automationIssues", json!({}))
    }

    pub fn save_automation(&self, id: Option<&str>, input: crate::automations::AutomationInput) -> Result<crate::automations::Automation> {
        self.call("runtime.session.automationSave", json!({"id":id,"input":input}))
    }

    pub fn delete_automation(&self, id: &str) -> Result<()> {
        self.call("runtime.session.automationDelete", json!({"id":id}))
    }

    pub fn run_automation(&self, id: &str) -> Result<crate::automations::AutomationRun> {
        self.call("runtime.session.automationRun", json!({"id":id}))
    }

    pub fn usage_snapshot(&self) -> Result<crate::status::usage::UsageSnapshot> {
        self.call("runtime.session.usage", json!({}))
    }

    pub fn refresh_usage(&self, manual: bool) -> Result<crate::status::usage::UsageSnapshot> {
        self.call("runtime.session.refreshUsage", json!({"manual":manual}))
    }

    pub fn reset_codex_usage(&self) -> Result<crate::status::usage::UsageSnapshot> {
        self.call("runtime.session.resetCodexUsage", json!({}))
    }

    pub fn activity_summary(&self) -> Result<crate::store::activity::Summary> {
        self.call("runtime.session.activity", json!({}))
    }

    pub fn add_tab(&self, session: &str, tab: &crate::session_ops::NewTab) -> Result<crate::store::index::TabEntry> {
        self.call("runtime.session.addTab", json!({"sessionId":session, "tab":{
            "harness":tab.harness,"model":tab.model,"effort":tab.effort,"permissionMode":tab.permission_mode
        }}))
    }

    pub fn pane_agents(&self) -> Result<Vec<crate::session::follow::PaneAgent>> {
        self.call("runtime.session.paneAgents", json!({}))
    }

    pub fn resume_conversation(&self, session: &str, tab: &str, harness: &str, provider_session_id: &str) -> Result<()> {
        self.tab("resumeConversation", session, tab, json!({"harness":harness,"providerSessionId":provider_session_id}))
    }

    pub fn running_tabs(&self) -> Result<Vec<RunningTab>> {
        self.call("runtime.session.running", json!({}))
    }

    pub fn pending_permissions(&self) -> Result<Vec<PendingPermission>> {
        self.call("runtime.session.permissions", json!({}))
    }

    pub fn load_events(&self, session: &str, tab: &str) -> Result<Vec<AgentEvent>> {
        self.tab("events", session, tab, json!({}))
    }

    pub fn status_of(&self, session: &str, tab: &str) -> Result<TabStatus> {
        self.tab("status", session, tab, json!({}))
    }

    pub fn pane_of(&self, session: &str, tab: &str) -> Result<Option<TabPtyEvent>> {
        self.tab("pane", session, tab, json!({}))
    }

    pub fn queued(&self, session: &str, tab: &str) -> Result<Vec<QueuedMessage>> {
        self.tab("queued", session, tab, json!({}))
    }

    pub fn ensure_started(&self, session: &str, tab: &str) -> Result<()> {
        self.tab("ensureStarted", session, tab, json!({}))
    }

    pub fn send(&self, session: &str, tab: &str, text: String, images: Vec<ImageInput>) -> Result<SendOutcome> {
        self.tab("send", session, tab, json!({"text":text,"images":images}))
    }

    pub fn send_confirmed(&self, session: &str, tab: &str, text: String) -> Result<SendOutcome> {
        self.tab("sendConfirmed", session, tab, json!({"text":text}))
    }

    pub fn prepare_continuation(&self, session: &str, tab: &str) -> Result<crate::continuation::Context> {
        self.tab("prepareContinuation", session, tab, json!({}))
    }

    pub fn interrupt(&self, session: &str, tab: &str) -> Result<bool> {
        self.tab("interrupt", session, tab, json!({}))
    }

    pub fn stop(&self, session: &str, tab: &str) -> Result<()> {
        self.tab("stop", session, tab, json!({}))
    }

    pub fn close_tab(&self, session: &str, tab: &str) -> Result<()> {
        self.tab("closeTab", session, tab, json!({}))
    }


    pub fn send_queued_now(&self, session: &str, tab: &str, seq: u64) -> Result<()> {
        self.tab("sendQueuedNow", session, tab, json!({"seq":seq}))
    }

    pub fn cancel_queued(&self, session: &str, tab: &str, message: &str) -> Result<Option<QueuedMessage>> {
        self.tab("cancelQueued", session, tab, json!({"messageId":message}))
    }

    pub fn respond_permission(&self, session: &str, tab: &str, request: &str, option: &str) -> Result<()> {
        self.tab("permission", session, tab, json!({"requestId":request,"optionId":option}))
    }

    pub fn answer_questions(&self, session: &str, tab: &str, request: &str, answers: HashMap<String, String>) -> Result<()> {
        self.tab("questions", session, tab, json!({"requestId":request,"answers":answers}))
    }

    pub fn set_model(&self, session: &str, tab: &str, model: &str) -> Result<()> {
        self.tab("model", session, tab, json!({"model":model}))
    }

    pub fn set_permission_mode(&self, session: &str, tab: &str, mode: &str) -> Result<()> {
        self.tab("mode", session, tab, json!({"mode":mode}))
    }

    pub fn set_effort(&self, session: &str, tab: &str, effort: Option<&str>) -> Result<()> {
        self.tab("effort", session, tab, json!({"effort":effort}))
    }

    pub fn mark_read(&self, session: &str, tab: &str) -> Result<()> {
        self.tab("markRead", session, tab, json!({}))
    }

    pub fn handoff(&self, session: &str, tab: &str) -> Result<HandoffInfo> {
        self.tab("handoff", session, tab, json!({}))
    }
}
