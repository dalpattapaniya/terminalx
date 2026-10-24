//! Local ownership and detach exercise the real runtime and fake provider,
//! with no installed desktop, cloud account or production store.
#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};
use common::agent::{count, texts, AgentWorld, Serve};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use base64::Engine as _;

struct Stream(BufReader<UnixStream>);

impl Stream {
    fn open(serve: &Serve) -> Self {
        let status = serve.control("status", json!({}));
        let socket = UnixStream::connect(status["localRuntime"]["eventSocket"].as_str().unwrap()).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut stream = Self(BufReader::new(socket));
        stream.send(json!({"protocol":1,"token":serve.token}));
        assert_eq!(stream.next()["type"], "ready");
        stream
    }
    fn send(&mut self, value: serde_json::Value) {
        writeln!(self.0.get_mut(), "{value}").unwrap();
    }
    fn next(&mut self) -> serde_json::Value {
        let mut line = String::new();
        assert!(self.0.read_line(&mut line).unwrap() != 0, "stream disconnected");
        serde_json::from_str(&line).unwrap()
    }
    fn pty(&mut self, id: &str) -> Vec<u8> {
        loop {
            let frame = self.next();
            if frame["type"] == "pty" && frame["id"] == id {
                return base64::engine::general_purpose::STANDARD.decode(frame["data"].as_str().unwrap()).unwrap();
            }
        }
    }
}

fn stopped(serve: &mut Serve) {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = serve.child.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        assert!(Instant::now() < until, "runtime failed to exit");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn only_one_local_owner_can_touch_a_home() {
    let world = AgentWorld::new();
    let serve = Serve::start(world.command(), &world.data);
    let before = serve.control("status", json!({}));
    let token = std::fs::read(world.data.join("run/control.token")).unwrap();
    let rejected = world.command().output().unwrap();
    assert!(!rejected.status.success());
    assert_eq!(std::fs::read(world.data.join("run/control.token")).unwrap(), token);
    assert_eq!(serve.control("status", json!({}))["localRuntime"], before["localRuntime"]);
}

#[test]
fn a_lease_holds_an_idle_runtime_and_detach_releases_it() {
    let world = AgentWorld::new();
    let mut command = world.command();
    command.args(["--idle-grace-seconds", "1"]);
    let mut serve = Serve::start(command, &world.data);
    let attachment = serve.control("runtime.attach", json!({"protocol":1}));
    std::thread::sleep(Duration::from_secs(2));
    assert!(serve.child.try_wait().unwrap().is_none());
    serve.control("runtime.heartbeat", attachment.clone());
    serve.control("runtime.detach", attachment);
    stopped(&mut serve);
}

#[test]
fn detached_turn_and_permission_keep_the_same_owner_and_history() {
    let world = AgentWorld::new();
    let mut command = world.command();
    command.args(["--idle-grace-seconds", "1"]);
    let mut serve = Serve::start(command, &world.data);
    let first = serve.control("runtime.attach", json!({"protocol":1}));
    let created = serve.control("sessions.create", json!({
        "project":world.project, "agent":"claude", "prompt":"slow:6:500",
        "useWorktree":false, "onMain":true, "mode":"manual"
    }));
    let tab = created["tabId"].as_str().unwrap();
    serve.control("runtime.detach", first.clone());
    let events = serve.wait_events(tab, "detached turn completion", |events| count(events, "turn_completed") == 1);
    let said = texts(&events, "assistant_text");
    for chunk in 1..=6 { assert!(said.contains(&format!("chunk {chunk} of 6"))); }
    assert_eq!(texts(&events, "user_message"), ["slow:6:500"]);
    let second = serve.control("runtime.attach", json!({"protocol":1}));
    assert_eq!(first["epoch"], second["epoch"]);
    assert_ne!(first["lease"], second["lease"]);
    assert_eq!(serve.control("status", json!({}))["pid"], serve.child.id());
    serve.control("send", json!({"target":tab, "text":"ask:echo approved"}));
    assert_eq!(serve.control("wait", json!({"target":tab, "timeoutSeconds":10}))["reason"], "permission");
    let pending = serve.control("permissions.list", json!({}));
    serve.control("runtime.detach", second);
    std::thread::sleep(Duration::from_secs(2));
    let third = serve.control("runtime.attach", json!({"protocol":1}));
    assert_eq!(first["epoch"], third["epoch"]);
    assert_eq!(pending, serve.control("permissions.list", json!({})));
    let request = &pending["permissions"][0]["requestId"];
    serve.control("permissions.allow", json!({"request":request}));
    let events = serve.wait_events(tab, "reattached approval", |events| count(events, "turn_completed") == 2);
    assert!(texts(&events, "assistant_text").contains(&"allowed: echo approved".into()));
    assert!(!events.iter().any(|event| event["payload"]["type"] == "recovery"));
    serve.control("runtime.stopAll", json!({"epoch":third["epoch"]}));
    stopped(&mut serve);
}

#[test]
fn disconnect_releases_unacknowledged_output_and_reattach_reads_the_retained_tail() {
    let world = AgentWorld::new();
    let mut serve = Serve::start(world.command(), &world.data);
    let created = serve.control("sessions.create", json!({
        "project":world.project, "agent":"claude", "prompt":"echo:ready",
        "useWorktree":false, "onMain":true, "mode":"manual"
    }));
    let id = format!("{}:1", created["sessionId"].as_str().unwrap());
    let mut stream = Stream::open(&serve);
    // Attach before spawn, just like a newly mounted terminal view. Its old
    // acknowledgement debt must not hold the process after the socket closes.
    stream.send(json!({"method":"attach","id":id,"token":"view"}));
    serve.control("runtime.pty.spawn", json!({"id":id,"cwd":world.project,"cols":80,"rows":24,
        "command":"sh -c 'sleep 0.2; head -c 4194304 /dev/zero; printf done-after-detach; sleep 20'"}));
    let pid = serve.control("runtime.pty.output", json!({"id":id}))["pid"].clone();
    let mut bytes = 0;
    while bytes <= 1024 * 1024 { bytes += stream.pty(&id).len(); }
    drop(stream);
    common::agent::wait_until("the disconnected tap to disappear", || {
        (serve.control("runtime.pty.stats", json!({}))["views"] == 0).then_some(())
    });
    let output = common::agent::wait_until("the command to finish printing while detached", || {
        let output = serve.control("runtime.pty.output", json!({"id":id}));
        let bytes = base64::engine::general_purpose::STANDARD.decode(output["data"].as_str().unwrap()).unwrap();
        bytes.ends_with(b"done-after-detach").then_some((output, bytes))
    });
    assert_eq!(output.0["pid"], pid);
    assert_eq!(output.0["running"], true);
    assert!(output.1.len() <= 512 * 1024, "history is bounded");
    let mut stream = Stream::open(&serve);
    stream.send(json!({"method":"attach","id":id,"token":"new-view"}));
    let replay = stream.pty(&id);
    assert_eq!(replay, output.1, "reattach hands over the retained bytes exactly once");
    stream.send(json!({"method":"ack","id":id,"token":"new-view","drawn":replay.len()}));
    common::agent::wait_until("the replay acknowledgement", || {
        (serve.control("runtime.pty.stats", json!({}))["unackedBytes"] == 0).then_some(())
    });
    let epoch = serve.control("status", json!({}))["localRuntime"]["epoch"].clone();
    serve.control("runtime.stopAll", json!({"epoch":epoch}));
    stopped(&mut serve);
    // This is this disposable runtime's shell PID, never a live app process.
    let status = std::process::Command::new("kill").args(["-0", &pid.as_u64().unwrap().to_string()]).output().unwrap();
    assert!(!status.status.success(), "stop-all left the terminal process alive");
}

#[test]
fn event_socket_rejects_invalid_authentication_without_an_attachment() {
    let world = AgentWorld::new();
    let serve = Serve::start(world.command(), &world.data);
    let status = serve.control("status", json!({}));
    for hello in [json!({"protocol":1,"token":"not-the-owner"}), json!({"protocol":2,"token":serve.token})] {
        let socket = UnixStream::connect(status["localRuntime"]["eventSocket"].as_str().unwrap()).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut stream = Stream(BufReader::new(socket));
        stream.send(hello);
        let mut response = String::new();
        assert_eq!(stream.0.read_line(&mut response).unwrap(), 0);
        assert_eq!(serve.control("status", json!({}))["localRuntime"]["attachedClients"], 0);
    }
}

#[test]
fn native_client_pins_its_owner_and_cannot_send_to_a_replacement() {
    let world = AgentWorld::new();
    let mut serve = Serve::start(world.command(), &world.data);
    let client = raccoon_lib::local_runtime::client::Client::connect(&world.data).unwrap();
    let mut events = client.stream().unwrap();
    writeln!(events.writer().unwrap(), "{}", json!({"method":"ping"})).unwrap();
    assert_eq!(events.receive().unwrap()["type"], "pong");
    let status: serde_json::Value = client.call("status", json!({})).unwrap();
    assert_eq!(status["pid"], serve.child.id());
    assert_eq!(events.attachment().epoch, status["localRuntime"]["epoch"]);
    assert!(!events.attachment().lease.is_empty());
    let sessions = raccoon_lib::local_runtime::sessions::Sessions::new(client.clone());
    client.stop_all().unwrap();
    stopped(&mut serve);
    drop(events);
    drop(serve);
    let replacement = Serve::start(world.command(), &world.data);
    assert!(client.call::<serde_json::Value>("status", json!({})).is_err());
    assert!(sessions.running_tabs().is_err(), "disconnect is not an empty running list");
    assert!(sessions.pending_permissions().is_err(), "disconnect must not hide pending asks");
    assert!(sessions.status_of("unknown", "unknown").is_err(), "disconnect is not idle");
    assert!(client.stop_all().is_err(), "stale clients must never follow a rotated discovery token");
    assert_ne!(status["localRuntime"]["epoch"], replacement.control("status", json!({}))["localRuntime"]["epoch"]);
}

#[test]
fn stop_all_cancels_an_already_admitted_long_poll() {
    let world = AgentWorld::new();
    let mut serve = Serve::start(world.command(), &world.data);
    let created = serve.control("sessions.create", json!({
        "project":world.project, "agent":"claude", "prompt":"slow:100:1000",
        "useWorktree":false, "onMain":true, "mode":"manual"
    }));
    let tab = created["tabId"].as_str().unwrap();
    serve.wait_events(tab, "first slow chunk", |events| texts(events, "assistant_text").iter().any(|text| text.contains("chunk 1 of 100")));
    let mut socket = UnixStream::connect(&serve.socket).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    writeln!(socket, "{}", json!({"id":"long-wait", "token":serve.token, "command":"wait", "params":{"target":tab,"timeoutSeconds":86400}})).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let epoch = serve.control("status", json!({}))["localRuntime"]["epoch"].clone();
    serve.control("runtime.stopAll", json!({"epoch":epoch}));
    let mut line = String::new();
    BufReader::new(socket).read_line(&mut line).unwrap();
    let response: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(response["id"], "long-wait");
    assert_eq!(response["error"]["code"], "runtime_stopping");
    stopped(&mut serve);
}

#[test]
fn native_bridge_replays_bytes_and_releases_only_its_own_views() {
    use raccoon_lib::local_runtime::{bridge::Bridge, client::Client};
    use std::sync::Arc;
    let world = AgentWorld::new();
    let serve = Serve::start(world.command(), &world.data);
    let created = serve.control("sessions.create", json!({
        "project":world.project, "agent":"claude", "prompt":"echo:ready",
        "useWorktree":false, "onMain":true, "mode":"manual"
    }));
    let id = format!("{}:7", created["sessionId"].as_str().unwrap());
    serve.control("runtime.pty.spawn", json!({"id":id,"cwd":world.project,"cols":80,"rows":24,
        "command":"sh -c 'printf bridge-replay; sleep 60'"}));
    let output = common::agent::wait_until("retained bridge output", || {
        let output = serve.control("runtime.pty.output", json!({"id":id}));
        let bytes = base64::engine::general_purpose::STANDARD.decode(output["data"].as_str().unwrap()).unwrap();
        bytes.ends_with(b"bridge-replay").then_some(bytes)
    });
    let client = Client::connect(&world.data).unwrap();
    let sink = Arc::new(raccoon_lib::sink::BroadcastSink::new(16));
    let first = Bridge::connect(&client, sink.clone()).unwrap();
    let second = Bridge::connect(&client, sink).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    first.attach(&id, "same-token", Box::new(move |bytes| tx.send(bytes.to_vec()).is_ok())).unwrap();
    let (tx2, rx2) = std::sync::mpsc::channel();
    second.attach(&id, "same-token", Box::new(move |bytes| tx2.send(bytes.to_vec()).is_ok())).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), output);
    assert_eq!(rx2.recv_timeout(Duration::from_secs(5)).unwrap(), output);
    common::agent::wait_until("two independent native views", || {
        (serve.control("runtime.pty.stats", json!({}))["views"] == 2).then_some(())
    });
    drop(first);
    common::agent::wait_until("only the closed window's view to detach", || {
        (serve.control("runtime.pty.stats", json!({}))["views"] == 1).then_some(())
    });
    second.ack(&id, "same-token", output.len() as u64).unwrap();
    common::agent::wait_until("native acknowledgement", || {
        (serve.control("runtime.pty.stats", json!({}))["unackedBytes"] == 0).then_some(())
    });
    second.detach_all().unwrap();
    common::agent::wait_until("native detach", || {
        (serve.control("runtime.pty.stats", json!({}))["views"] == 0).then_some(())
    });
    assert_eq!(serve.control("runtime.pty.output", json!({"id":id}))["running"], true);
}

#[test]
fn launcher_process_fixture() {
    let Some(home) = std::env::var_os("TERMINALX_LAUNCHER_FIXTURE_HOME") else { return };
    let source = std::env::var_os("TERMINALX_LAUNCHER_FIXTURE_BINARY").unwrap();
    let client = raccoon_lib::local_runtime::launch::connect_or_start(std::path::Path::new(&source), std::path::Path::new(&home)).unwrap();
    let _connection = client.stream().unwrap();
    // Deliberately skip all Rust drops: actual UI process exit must release
    // its socket without sending stop-all or owning the runtime's lifetime.
    std::process::exit(0);
}

#[test]
fn installed_runtime_survives_launcher_exit_and_bundle_source_replacement() {
    use raccoon_lib::local_runtime::client::Client;
    let world = AgentWorld::new();
    struct StopPrivate(std::path::PathBuf);
    impl Drop for StopPrivate {
        fn drop(&mut self) {
            if let Ok(client) = Client::connect(&self.0) {
                let status = client.call::<serde_json::Value>("status", json!({})).ok();
                let _ = client.stop_all();
                if let Some(pid) = status.and_then(|status| status["pid"].as_u64()) {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while Instant::now() < deadline {
                        let alive = std::process::Command::new("/bin/kill").args(["-0", &pid.to_string()]).output().is_ok_and(|output| output.status.success());
                        if !alive { break; }
                        std::thread::sleep(Duration::from_millis(25));
                    }
                }
            }
        }
    }
    // Created before the launcher so even an attachment assertion failure
    // asks only this disposable home's owner to stop before fixture deletion.
    let _private_owner = StopPrivate(world.data.clone());
    std::fs::write(world.data.join("projects.json"), serde_json::to_vec(&json!({"projects":[{"path":world.project,"name":"fixture"}]})).unwrap()).unwrap();
    let source = world.home.join("bundled-terminalx-serve");
    std::fs::copy(env!("CARGO_BIN_EXE_terminalx-serve"), &source).unwrap();
    let mut launcher = std::process::Command::new(std::env::current_exe().unwrap());
    launcher.args(["--exact", "launcher_process_fixture", "--nocapture"])
        .env_clear().env("HOME", &world.home)
        .env("PATH", format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", world.bin.display()))
        .env("CLAUDE_CONFIG_DIR", world.home.join(".claude"))
        .env("TERMINALX_HOME", &world.data)
        .env("TERMINALX_LAUNCHER_FIXTURE_HOME", &world.data)
        .env("TERMINALX_LAUNCHER_FIXTURE_BINARY", &source);
    assert!(launcher.status().unwrap().success());
    let client = Client::connect(&world.data).unwrap();
    struct Stop(std::sync::Arc<Client>);
    impl Drop for Stop { fn drop(&mut self) { let _ = self.0.stop_all(); } }
    let cleanup = Stop(client.clone());
    let _connection = client.stream().unwrap();
    let before: serde_json::Value = client.call("status", json!({})).unwrap();
    // Replace the bundle source inode, then remove it altogether. The signed
    // installed image and hook command paths must remain outside that source.
    std::fs::remove_file(&source).unwrap();
    std::fs::write(&source, b"replacement bundle placeholder").unwrap();
    let same = raccoon_lib::local_runtime::launch::connect_or_start(&source, &world.data).unwrap();
    std::fs::remove_file(&source).unwrap();
    let after: serde_json::Value = same.call("status", json!({})).unwrap();
    assert_eq!(before["pid"], after["pid"]);
    assert_eq!(before["localRuntime"]["epoch"], after["localRuntime"]["epoch"]);
    let created: serde_json::Value = client.call("sessions.create", json!({
        "project":world.project,"agent":"claude","prompt":"echo:after-bundle-replacement",
        "useWorktree":false,"onMain":true,"mode":"manual"
    })).unwrap();
    let tab = created["tabId"].as_str().unwrap();
    common::agent::wait_until("provider hooks from the installed helper", || {
        let events: serde_json::Value = client.call("read", json!({"target":tab})).unwrap();
        events["events"].as_array().filter(|events| count(events,"turn_completed") == 1).cloned()
    });
    client.stop_all().unwrap();
    let pid = before["pid"].as_u64().unwrap();
    common::agent::wait_until("the installed owner to stop", || {
        let alive = std::process::Command::new("/bin/kill").args(["-0", &pid.to_string()]).output().unwrap().status.success();
        (!alive).then_some(())
    });
    drop(cleanup);
}

#[test]
fn a_partial_view_request_is_not_discarded_when_an_event_arrives() {
    let world = AgentWorld::new();
    let serve = Serve::start(world.command(), &world.data);
    let created = serve.control("sessions.create", json!({
        "project":world.project,"agent":"claude","prompt":"echo:ready",
        "useWorktree":false,"onMain":true,"mode":"manual"
    }));
    let id = format!("{}:8", created["sessionId"].as_str().unwrap());
    let mut stream = Stream::open(&serve);
    let request = format!("{}\n", json!({"method":"attach","id":id,"token":"fragmented-view"}));
    let split = request.len() / 2;
    stream.0.get_mut().write_all(&request.as_bytes()[..split]).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    serve.control("runtime.pty.spawn", json!({"id":id,"cwd":world.project,"cols":80,"rows":24,
        "command":"sh -c 'printf fragmented-request-survived; sleep 30'"}));
    loop {
        let frame = stream.next();
        if frame["type"] == "event" && frame["event"] == "pty_opened" && frame["payload"]["id"] == id { break; }
    }
    // Reading this event proves select resumed its other branch while the
    // input frame had no newline. The already consumed prefix must survive.
    stream.0.get_mut().write_all(&request.as_bytes()[split..]).unwrap();
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"fragmented-request-survived") { bytes.extend(stream.pty(&id)); }
}

fn scheduled_fixture(world: &AgentWorld, enabled: bool) {
    let automation = json!({
        "id":"scheduled-fixture","name":"Fixture schedule","enabled":enabled,
        "projectPath":world.project,"harness":"claude","mode":"manual",
        "prompt":"echo:scheduled-without-a-window","workspace":"newWorktree",
        "schedule":{"kind":"preset","preset":"hourly","minute":0,"timezone":"UTC","dtstart":"1970-01-01T00:00:00Z"},
        "nextRunAt":"1970-01-01T00:00:00Z","missedRunGraceMinutes":u32::MAX,
        "created":"1970-01-01T00:00:00Z","modified":"1970-01-01T00:00:00Z",
        "pipeline":{"autoMerge":false,"resolveConflicts":false}
    });
    std::fs::write(world.data.join("automations.json"), serde_json::to_vec(&json!({"automations":[automation]})).unwrap()).unwrap();
}

#[test]
fn local_schedule_runs_and_records_completion_without_a_window() {
    let world = AgentWorld::new();
    scheduled_fixture(&world, true);
    let mut serve = Serve::start(world.command(), &world.data);
    let completed = common::agent::wait_until("the local scheduled turn to complete", || {
        let runs = std::fs::read_to_string(world.data.join("automations/scheduled-fixture/runs.jsonl")).ok()?;
        runs.lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|run| run["status"] == "completed")
    });
    let tab = completed["tabId"].as_str().unwrap();
    let events = serve.events(tab);
    assert!(texts(&events, "assistant_text").iter().any(|text| text.contains("scheduled-without-a-window")));
    assert_eq!(serve.control("status", json!({}))["localRuntime"]["attachedClients"], 0);
    let pipelines = std::fs::read_to_string(world.data.join("issue-pipelines.json")).ok()
        .map(|text| serde_json::from_str::<serde_json::Value>(&text).unwrap()).unwrap_or_else(|| json!({"pipelines":[]}));
    assert_eq!(pipelines["pipelines"], json!([]), "both-off automation must not create a pipeline");
    let epoch = serve.control("status", json!({}))["localRuntime"]["epoch"].clone();
    serve.control("runtime.stopAll", json!({"epoch":epoch}));
    stopped(&mut serve);
}

#[test]
fn a_disabled_schedule_does_not_hold_an_idle_runtime() {
    let world = AgentWorld::new();
    scheduled_fixture(&world, false);
    let mut command = world.command();
    command.args(["--idle-grace-seconds", "1"]);
    let mut serve = Serve::start(command, &world.data);
    stopped(&mut serve);
    assert!(!world.data.join("automations/scheduled-fixture/runs.jsonl").exists());
}

#[test]
fn interrupted_automation_is_recovered_before_ready_and_not_on_reattach() {
    let world = AgentWorld::new();
    scheduled_fixture(&world, false);
    let directory = world.data.join("automations/scheduled-fixture");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("runs.jsonl");
    let open = json!({"runId":"interrupted-fixture","runNumber":1,
        "automationId":"scheduled-fixture","trigger":"scheduled","status":"running",
        "startedAt":"1970-01-01T00:00:00Z","futureField":{"retained":true}});
    std::fs::write(&path, format!("{open}\n")).unwrap();
    let serve = Serve::start(world.command(), &world.data);
    let recovered = std::fs::read_to_string(&path).unwrap();
    let last: serde_json::Value = serde_json::from_str(recovered.lines().last().unwrap()).unwrap();
    assert_eq!(last["status"], "failed");
    assert!(last["endedAt"].is_string());
    assert_eq!(last["futureField"], open["futureField"]);
    for _ in 0..2 {
        let stream = Stream::open(&serve);
        drop(stream);
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), recovered,
        "attachment must not rewrite recovered automation history");
}

#[test]
fn typed_session_adapter_reattaches_to_a_pending_turn_and_answers_without_recovery() {
    use raccoon_lib::local_runtime::{client::Client, sessions::Sessions};
    let world = AgentWorld::new();
    let mut serve = Serve::start(world.command(), &world.data);
    let first_client = Client::connect(&world.data).unwrap();
    let first_stream = first_client.stream().unwrap();
    let epoch = first_stream.attachment().epoch.clone();
    let created = serve.control("sessions.create", json!({
        "project":world.project,"agent":"claude","prompt":"ask:echo pending-while-detached",
        "useWorktree":false,"onMain":true,"mode":"manual"
    }));
    let session = created["sessionId"].as_str().unwrap();
    let tab = created["tabId"].as_str().unwrap();
    drop(first_stream);
    drop(first_client);
    assert_eq!(serve.control("wait", json!({"target":tab,"timeoutSeconds":10}))["reason"], "permission");
    let client = Client::connect(&world.data).unwrap();
    let stream = client.stream().unwrap();
    assert_eq!(stream.attachment().epoch, epoch);
    let sessions = Sessions::new(client.clone());
    let pending = sessions.pending_permissions().unwrap();
    let pending = pending.iter().find(|request| request.tab_id == tab).unwrap();
    assert_eq!(pending.session_id, session);
    assert!(sessions.running_tabs().unwrap().iter().any(|running| running.tab_id == tab));
    assert!(sessions.pane_of(session, tab).unwrap().is_some());
    assert!(sessions.queued(session, tab).unwrap().is_empty());
    sessions.load_events(session, tab).unwrap();
    sessions.respond_permission(session, tab, &pending.request_id, "allow").unwrap();
    serve.wait_events(tab, "reattached permission completion", |events| count(events, "turn_completed") == 1);
    let events: Vec<serde_json::Value> = sessions.load_events(session, tab).unwrap().into_iter().map(|event| serde_json::to_value(event).unwrap()).collect();
    assert_eq!(texts(&events, "user_message"), ["ask:echo pending-while-detached"]);
    assert!(!serde_json::to_string(&events).unwrap().contains("Session closed."));
    assert!(sessions.pending_permissions().unwrap().is_empty());
    sessions.send(session, tab, "echo:typed-after-attach".into(), Vec::new()).unwrap();
    serve.wait_events(tab, "typed adapter send", |events| texts(events, "assistant_text").iter().any(|text| text.contains("typed-after-attach")));
    client.stop_all().unwrap();
    stopped(&mut serve);
    assert!(sessions.load_events(session, tab).is_err());
}
