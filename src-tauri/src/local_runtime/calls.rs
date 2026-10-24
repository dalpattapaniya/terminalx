//! Native desktop operations on the existing session and PTY owners. This is
//! a local authenticated surface, not a new cloud authority or generic invoke.

use std::sync::Arc;
use anyhow::{bail, Context, Result};
use base64::Engine as _;
use serde_json::{json, Value};
use crate::session::SessionManager;
use crate::sink::EventSink;

fn string<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params[key].as_str().with_context(|| format!("{key} must be a string"))
}

pub fn execute(manager: &SessionManager, sink: &Arc<dyn EventSink>, command: &str, params: Value) -> Result<Value> {
    if let Some(method) = command.strip_prefix("runtime.session.") {
        if method == "running" { return Ok(serde_json::to_value(manager.running_tabs())?); }
        if method == "permissions" { return Ok(serde_json::to_value(manager.pending_permissions())?); }
        let session = string(&params, "sessionId")?;
        let tab = string(&params, "tabId")?;
        let value = match method {
            "ensureStarted" => { manager.ensure_started(session, tab)?; Value::Null }
            "events" => serde_json::to_value(manager.load_events(session, tab)?)?,
            "status" => serde_json::to_value(manager.status_of(session, tab))?,
            "pane" => serde_json::to_value(manager.pane_of(session, tab))?,
            "queued" => serde_json::to_value(manager.queued(session, tab))?,
            "sendConfirmed" => serde_json::to_value(manager.send_confirmed(session, tab, string(&params, "text")?.into())?)?,
            "prepareContinuation" => serde_json::to_value(crate::continuation::prepare(session, tab, manager.tracked_transcript(session, tab))?)?,
            "send" => serde_json::to_value(manager.send(session, tab, string(&params, "text")?.into(),
                serde_json::from_value(params.get("images").cloned().unwrap_or_else(|| json!([])))?)?)?,
            "interrupt" => json!(manager.interrupt(session, tab)?),
            "stop" => { manager.stop(session, tab)?; Value::Null }
            "sendQueuedNow" => { manager.send_queued_now(session, tab, params["seq"].as_u64().context("seq is required")?)?; Value::Null }
            "cancelQueued" => serde_json::to_value(manager.cancel_queued(session, tab, string(&params, "messageId")?)?)?,
            "permission" => { manager.respond_permission(session, tab, string(&params, "requestId")?, string(&params, "optionId")?)?; Value::Null }
            "questions" => { manager.answer_questions(session, tab, string(&params, "requestId")?, serde_json::from_value(params["answers"].clone())?)?; Value::Null }
            "model" => { manager.set_model(session, tab, string(&params, "model")?)?; Value::Null }
            "mode" => { manager.set_permission_mode(session, tab, string(&params, "mode")?)?; Value::Null }
            "effort" => { manager.set_effort(session, tab, params["effort"].as_str())?; Value::Null }
            "markRead" => { manager.mark_read(session, tab)?; Value::Null }
            "handoff" => serde_json::to_value(manager.handoff(session, tab)?)?,
            _ => bail!("unsupported local session operation"),
        };
        return Ok(value);
    }
    if let Some(method) = command.strip_prefix("runtime.pty.") {
        let terminals = manager.terminals();
        if method == "shells" { return Ok(serde_json::to_value(terminals.shells())?); }
        if method == "stats" { return Ok(serde_json::to_value(terminals.stats())?); }
        let id = string(&params, "id")?;
        if id.is_empty() || id.len() > 256 { bail!("invalid pane id"); }
        let size = || -> Result<(u16, u16)> {
            let cols = u16::try_from(params["cols"].as_u64().context("cols is required")?)?.max(2);
            let rows = u16::try_from(params["rows"].as_u64().context("rows is required")?)?.max(1);
            Ok((cols, rows))
        };
        match method {
            "spawn" => {
                // Agent panes are exclusively SessionManager's to launch.
                if !crate::pty::is_shell_id(id) { bail!("shell pane id must name its session"); }
                let session = crate::pty::shell_session(id).context("session is required")?;
                crate::store::index::get(session)?;
                let (cols, rows) = size()?;
                let cwd = string(&params, "cwd")?;
                terminals.spawn(sink.clone(), id, crate::pty::PaneSpec { cwd, cols, rows, command: params["command"].as_str(), env: &[] })?;
                if let Some(title) = params["title"].as_str().filter(|title| !title.trim().is_empty()) { terminals.set_title(id, title); }
                if let Some(shell) = terminals.shell(id) { sink.emit("pty_opened", &shell); }
                Ok(Value::Null)
            }
            "write" => {
                let data = string(&params, "data")?;
                if data.len() > 1024 * 1024 { bail!("terminal input is too large"); }
                terminals.write(id, &base64::engine::general_purpose::STANDARD.decode(data)?)?;
                Ok(Value::Null)
            }
            "resize" => { let (cols, rows) = size()?; terminals.resize(id, cols, rows)?; Ok(Value::Null) }
            "busy" => Ok(json!(terminals.is_busy(id))),
            "output" => Ok(json!({"data":terminals.read_output(id).map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
                "running":terminals.is_running(id), "pid":terminals.pid(id)})),
            "rename" => {
                terminals.rename(id, string(&params, "title")?);
                if let Some(shell) = terminals.shell(id) { sink.emit("pty_renamed", &shell); }
                Ok(Value::Null)
            }
            "kill" => {
                terminals.kill(id);
                if crate::pty::is_shell_id(id) { sink.emit("pty_closed", id); }
                Ok(Value::Null)
            }
            _ => bail!("unsupported local terminal operation"),
        }
    } else {
        bail!("unsupported local runtime operation")
    }
}
