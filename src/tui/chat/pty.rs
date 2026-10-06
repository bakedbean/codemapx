//! An agent's TUI in a pseudo-terminal: a reader thread feeds a vt100 screen and pings the event loop.

use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex, MutexGuard, mpsc::SyncSender},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// Minimum age before text is typed into the agent, so it isn't lost to start-up redraws.
const SETTLE: Duration = Duration::from_millis(1500);
/// Output silence that counts as idle.
const QUIET: Duration = Duration::from_millis(400);
const SCROLLBACK: usize = 1000;

pub struct Session {
    parser: Arc<Mutex<vt100::Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    exit: Arc<Mutex<Option<u32>>>,
    last_output: Arc<Mutex<Option<Instant>>>,
    started: Instant,
    size: (u16, u16),
}

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

impl Session {
    /// Runs `argv` in `cwd` with the parent's environment; `wake` gets `()` after output and at exit.
    /// Wakes are coalesced (`try_send` on a one-slot channel), so they can't pile up while the UI is paused in the editor.
    pub fn spawn(argv: &[String], cwd: &Path, rows: u16, cols: u16, wake: SyncSender<()>) -> Result<Session, String> {
        let bin = argv.first().ok_or("no agent command")?;
        let fail = |e: &dyn std::fmt::Display| format!("can't run {bin}: {e}");
        let pair = native_pty_system().openpty(pty_size(rows, cols)).map_err(|e| fail(&e))?;
        let mut cmd = CommandBuilder::new(bin);
        cmd.args(&argv[1..]);
        cmd.cwd(cwd);
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| fail(&e))?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(|e| fail(&e))?;
        let writer = pair.master.take_writer().map_err(|e| fail(&e))?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK)));
        let exit = Arc::new(Mutex::new(None));
        let last_output = Arc::new(Mutex::new(None));
        let (p, x, l) = (parser.clone(), exit.clone(), last_output.clone());
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                p.lock().unwrap().process(&buf[..n]);
                *l.lock().unwrap() = Some(Instant::now());
                let _ = wake.try_send(());
            }
            *x.lock().unwrap() = Some(child.wait().map(|s| s.exit_code()).unwrap_or(u32::MAX));
            let _ = wake.try_send(());
        });
        Ok(Session { parser, master: pair.master, writer, killer, exit, last_output, started: Instant::now(), size: (rows, cols) })
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            let _ = self.writer.write_all(bytes).and_then(|_| self.writer.flush());
        }
    }

    /// No-op unless the size changed, since drawing calls it every frame.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if (rows, cols) != self.size {
            self.size = (rows, cols);
            let _ = self.master.resize(pty_size(rows, cols));
            self.parser.lock().unwrap().set_size(rows, cols);
        }
    }

    pub fn parser(&self) -> MutexGuard<'_, vt100::Parser> {
        self.parser.lock().unwrap()
    }

    pub fn exit_code(&self) -> Option<u32> {
        *self.exit.lock().unwrap()
    }

    pub fn settled(&self, now: Instant) -> bool {
        settled(self.started, *self.last_output.lock().unwrap(), now)
    }

    /// Positive `d` moves back into the scrollback.
    pub fn scroll_by(&self, d: isize) {
        let mut p = self.parser();
        let cur = p.screen().scrollback() as isize;
        p.set_scrollback((cur + d).max(0) as usize);
    }

    pub fn scroll_to_live(&self) {
        self.parser().set_scrollback(0);
    }

    /// The wheel as the program asked to hear it (`col`/`row` 1-based in the panel), or None when it didn't ask.
    pub fn wheel_bytes(&self, up: bool, col: u16, row: u16) -> Option<Vec<u8>> {
        let p = self.parser();
        let screen = p.screen();
        if screen.mouse_protocol_mode() == vt100::MouseProtocolMode::None {
            return None;
        }
        let cb: u16 = if up { 64 } else { 65 };
        Some(match screen.mouse_protocol_encoding() {
            vt100::MouseProtocolEncoding::Sgr => format!("\x1b[<{cb};{col};{row}M").into_bytes(),
            _ => vec![0x1b, b'[', b'M', 32 + cb as u8, 32 + col.min(223) as u8, 32 + row.min(223) as u8],
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.killer.kill();
    }
}

/// Old enough, has drawn something, and quiet since.
pub fn settled(started: Instant, last_output: Option<Instant>, now: Instant) -> bool {
    now.duration_since(started) >= SETTLE && last_output.is_some_and(|t| now.duration_since(t) >= QUIET)
}
