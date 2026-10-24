//! One desktop attachment and independent PTY views for each of its windows.
//! All connections pin the same owner. A closed view is not a process stop,
//! and a failed owner connection is never replaced behind a pending request.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;

use super::{bridge::Bridge, client::{AttachmentIdentity, Client}, sessions::Sessions};
use crate::{pty::Tap, sink::EventSink};

pub struct Desktop {
    client: Arc<Client>,
    events: Bridge,
    sink: Arc<dyn EventSink>,
    windows: Mutex<HashMap<String, Bridge>>,
}

impl Desktop {
    pub fn connect(client: Arc<Client>, sink: Arc<dyn EventSink>) -> Result<Self> {
        let events = Bridge::connect(&client, sink.clone())?;
        Ok(Self { client, events, sink, windows: Mutex::new(HashMap::new()) })
    }

    pub fn attachment(&self) -> Result<AttachmentIdentity> { self.events.attachment() }

    pub fn sessions(&self) -> Result<Sessions> {
        self.attachment()?;
        Ok(Sessions::new(self.client.clone()))
    }

    pub fn terminals(&self) -> Result<super::terminals::Terminals> {
        self.attachment()?;
        Ok(super::terminals::Terminals::new(self.client.clone()))
    }

    pub fn stop_all(&self) -> Result<()> { self.client.stop_all() }

    pub fn attach(&self, window: &str, id: &str, token: &str, tap: Tap) -> Result<()> {
        self.attachment()?;
        let mut windows = self.windows.lock().unwrap();
        if !windows.contains_key(window) {
            windows.insert(window.into(), Bridge::connect_views(&self.client, self.sink.clone())?);
        }
        windows[window].attach(id, token, tap)
    }

    pub fn ack(&self, window: &str, id: &str, token: &str, drawn: u64) -> Result<()> {
        if let Some(bridge) = self.windows.lock().unwrap().get(window) {
            bridge.ack(id, token, drawn)?;
        }
        Ok(())
    }

    pub fn detach(&self, window: &str, id: &str, token: &str) -> Result<()> {
        if let Some(bridge) = self.windows.lock().unwrap().get(window) {
            bridge.detach(id, token)?;
        }
        Ok(())
    }

    /// Page reload or window destruction releases just that window's lease and
    /// outstanding bytes. A new page obtains a fresh connection and replay.
    pub fn detach_window(&self, window: &str) {
        self.windows.lock().unwrap().remove(window);
    }
}
