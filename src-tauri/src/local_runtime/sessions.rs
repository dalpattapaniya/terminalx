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
pub struct Sessions(Arc<Client>);

impl Sessions {
    pub fn new(client: Arc<Client>) -> Self { Self(client) }

    fn tab<T: DeserializeOwned>(&self, method: &str, session: &str, tab: &str, mut params: Value) -> Result<T> {
        params["sessionId"] = json!(session);
        params["tabId"] = json!(tab);
        self.0.call(&format!("runtime.session.{method}"), params)
    }

    pub fn running_tabs(&self) -> Result<Vec<RunningTab>> {
        self.0.call("runtime.session.running", json!({}))
    }

    pub fn pending_permissions(&self) -> Result<Vec<PendingPermission>> {
        self.0.call("runtime.session.permissions", json!({}))
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
