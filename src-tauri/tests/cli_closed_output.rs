//! The `terminalx` CLI when whoever reads its output stops early (PRO-108).
//!
//! `terminalx --help | head -1` closes the pipe while the CLI is still
//! writing. `println!` panics on that write, and the release profile's
//! `panic = "abort"` turned the panic into SIGABRT, exit status 134 and a
//! macOS crash report for TerminalX. Agents pipe the CLI through `head` all
//! the time.
//!
//! Each run here gives the built binary an output pipe that nobody reads, so
//! its first write fails with a broken pipe however fast it runs, and checks
//! that it ends with the command's own status and no panic. This build
//! unwinds rather than aborts, but a panic fails the check either way: 101
//! here, SIGABRT in a release build.
//!
//! The last tests keep the panicking print macros out of the CLI's sources.

use std::path::Path;

use syn::punctuated::Punctuated;
use syn::visit::Visit;

#[cfg(all(unix, feature = "desktop"))]
mod closed_pipe {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::os::unix::process::ExitStatusExt;
    use std::path::PathBuf;
    use std::process::{Command, Output, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use serde_json::{json, Value};

    /// Which of the run's streams nobody reads. Standard error is otherwise
    /// kept, to look for a panic message in it.
    #[derive(Clone, Copy, Debug)]
    enum Unread {
        Stdout,
        Both,
    }

    /// A pipe whose read end is already closed: every write to it fails.
    fn unread() -> Stdio {
        let (reader, writer) = std::io::pipe().expect("pipe");
        drop(reader);
        writer.into()
    }

    /// Runs the binary with `args` and only `env`: this suite may itself run
    /// in a TerminalX tab, whose app must not be the one answering.
    fn run(args: &[&str], streams: Unread, env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_raccoon"));
        command.args(args).env_clear().envs(env.iter().copied()).stdin(Stdio::null()).stdout(unread());
        if matches!(streams, Unread::Both) {
            command.stderr(unread());
        }
        command.output().expect("run the binary")
    }

    fn assert_exit(output: &Output, code: i32, args: &[&str]) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(code),
            "{args:?} ended with {:?} (signal {:?}); stderr: {stderr}",
            output.status,
            output.status.signal()
        );
        assert!(!stderr.contains("panicked"), "{args:?} panicked: {stderr}");
    }

    #[test]
    fn local_output_into_a_closed_pipe_ends_quietly_with_status_zero() {
        let cases: [&[&str]; 6] = [
            &["terminalx", "--help"],
            &["terminalx", "--help", "--json"],
            &["terminalx", "--version"],
            &["terminalx", "skills", "get", "terminalx-cli", "--full"],
            &["terminalx", "skills", "get", "computer-use", "--json"],
            &["tnx", "--help"],
        ];
        for args in cases {
            let output = run(args, Unread::Stdout, &[]);
            assert_exit(&output, 0, args);
            assert!(output.stderr.is_empty(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    #[test]
    fn errors_into_a_closed_pipe_keep_their_status() {
        // A usage error: on standard error, or standard output with --json.
        assert_exit(&run(&["terminalx", "invented"], Unread::Both, &[]), 2, &["invented"]);
        assert_exit(&run(&["terminalx", "invented", "--json"], Unread::Stdout, &[]), 2, &["invented", "--json"]);
        // No app to ask.
        let nowhere = [("TERMINALX_NEXT_SOCKET", "/nonexistent/terminalx.sock"), ("TERMINALX_NEXT_TOKEN", "token")];
        assert_exit(&run(&["terminalx", "status"], Unread::Both, &nowhere), 1, &["status"]);
        assert_exit(&run(&["terminalx", "status", "--json"], Unread::Stdout, &nowhere), 1, &["status", "--json"]);
    }

    /// A stand-in app on the control socket. It answers one call, ok or
    /// failed, and passes on the command it was asked.
    struct App {
        dir: PathBuf,
        socket: String,
        asked: mpsc::Receiver<String>,
    }

    impl App {
        fn answering(ok: bool) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            // Short: a socket path is limited to about a hundred bytes.
            let dir = std::env::temp_dir().join(format!("tnx-pipe-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&dir).unwrap();
            let socket = dir.join("app.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let (tell, asked) = mpsc::channel();
            std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let _ = tell.send(request["command"].as_str().unwrap_or_default().to_string());
                let reply = if ok {
                    json!({"id": request["id"], "ok": true, "result": {"app": "TerminalX"}})
                } else {
                    json!({"id": request["id"], "ok": false, "error": {"code": "not_found", "message": "No such session.", "recovery": "List the sessions."}})
                };
                writeln!(&stream, "{reply}").unwrap();
            });
            Self { socket: socket.to_string_lossy().into_owned(), dir, asked }
        }

        fn env(&self) -> [(&str, &str); 2] {
            [("TERMINALX_NEXT_SOCKET", &self.socket), ("TERMINALX_NEXT_TOKEN", "token")]
        }
    }

    impl Drop for App {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn an_rpc_whose_reply_cannot_be_written_keeps_its_own_status() {
        for (ok, code) in [(true, 0), (false, 1)] {
            for (args, streams) in [(&["terminalx", "status", "--json"][..], Unread::Stdout), (&["terminalx", "status"], Unread::Both)] {
                let app = App::answering(ok);
                assert_exit(&run(args, streams, &app.env()), code, args);
                // The call reached the app: only its reply was lost.
                assert_eq!(app.asked.recv_timeout(Duration::from_secs(5)).as_deref(), Ok("status"), "{args:?}");
            }
        }
    }

    #[test]
    fn hook_and_statusline_survive_a_closed_pipe() {
        // Outside an app-launched tab the hook answers `{}`; nobody reads it.
        assert_exit(&run(&["hook", "PreToolUse"], Unread::Both, &[]), 0, &["hook"]);
        assert_exit(&run(&["statusline"], Unread::Both, &[]), 0, &["statusline"]);
    }
}

/// Write their whole output or panic, which the release build turns into an
/// abort and a crash report.
const PANICKING: [&str; 5] = ["print", "println", "eprint", "eprintln", "dbg"];

/// The CLI's output paths: `main` choosing the CLI, the thin client and its
/// formatters, the control client, and the hook and status-line commands.
const CLI_SOURCES: [&str; 6] = ["src/main.rs", "src/cli.rs", "src/control.rs", "src/hooks.rs", "src/computer/cli.rs", "src/browser/cli.rs"];

struct Printing {
    within: String,
    found: Vec<String>,
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg") && attr.parse_args::<syn::Ident>().is_ok_and(|ident| ident == "test"))
    })
}

impl Printing {
    fn within_fn(&mut self, name: &syn::Ident, visit: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.within, name.to_string());
        visit(self);
        self.within = outer;
    }
}

impl<'ast> Visit<'ast> for Printing {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !test_only(&item.attrs) {
            self.within_fn(&item.sig.ident, |me| syn::visit::visit_item_fn(me, item));
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !test_only(&item.attrs) {
            self.within_fn(&item.sig.ident, |me| syn::visit::visit_impl_item_fn(me, item));
        }
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Some(name) = mac.path.segments.last().map(|segment| segment.ident.to_string()) {
            if PANICKING.contains(&name.as_str()) {
                self.found.push(format!("{name}! in {}", self.within));
            }
        }
        // The arguments of `format!`, `write!`, `vec!` and the like.
        if let Ok(args) = mac.parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated) {
            for arg in &args {
                self.visit_expr(arg);
            }
        }
    }
}

fn printing_in(file: &str, source: &str) -> Vec<String> {
    let parsed = syn::parse_file(source).unwrap_or_else(|e| panic!("parse {file}: {e}"));
    let mut printing = Printing { within: "the module".into(), found: Vec::new() };
    printing.visit_file(&parsed);
    printing.found.into_iter().map(|found| format!("{file}: {found}")).collect()
}

#[test]
fn the_cli_never_prints_through_a_panicking_macro() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let found: Vec<String> = CLI_SOURCES
        .iter()
        .flat_map(|file| printing_in(file, &std::fs::read_to_string(manifest.join(file)).unwrap_or_else(|e| panic!("read {file}: {e}"))))
        .collect();
    assert!(
        found.is_empty(),
        "these panic when the reader closes the pipe, and the release build aborts on a panic; \
         write through `cli::emit` or ignore the io::Result instead (see tests/cli_closed_output.rs):\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn the_scan_finds_print_macros_outside_tests() {
    let source = r#"
        fn help() { println!("usage"); }
        impl Reply { fn say(&self) { let _ = format!("{}", std::eprintln!("x")); } }
        fn quiet(out: &mut dyn std::io::Write) { let _ = writeln!(out, "fine"); }
        #[cfg(test)]
        mod tests { #[test] fn prints() { println!("allowed in tests"); } }
    "#;
    assert_eq!(printing_in("example.rs", source), vec!["example.rs: println! in help", "example.rs: eprintln! in say"]);
}
