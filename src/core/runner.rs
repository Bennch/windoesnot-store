use std::io::{BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use std::sync::mpsc::Sender;

use crate::core::models::TerminalEmulator;

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl CommandSpec {
    pub fn new(program: &str, args: Vec<String>) -> Self {
        Self {
            program: program.to_string(),
            args,
            env: vec![(String::from("LC_ALL"), String::from("C"))],
        }
    }

    pub fn display_line(&self) -> String {
        let mut parts = Vec::new();
        for (k, v) in &self.env {
            parts.push(format!("{k}={}", shell_quote(v)));
        }
        parts.push(shell_quote(&self.program));
        for arg in &self.args {
            parts.push(shell_quote(arg));
        }
        parts.join(" ")
    }

    fn shell_command(&self) -> String {
        let mut parts = vec!["env".to_string()];
        for (k, v) in &self.env {
            parts.push(format!("{k}={}", shell_quote(v)));
        }
        parts.push(shell_quote(&self.program));
        for arg in &self.args {
            parts.push(shell_quote(arg));
        }
        parts.join(" ")
    }
}

#[derive(Debug, Clone)]
pub enum LogEvent {
    Line(String),
    Finished(i32),
}

/// Handle to a running command spawned by `CommandRunner`. Calling `cancel()`
/// terminates the underlying child process (and, for external-terminal runs,
/// the whole process group, so spawned shells / pacman are reaped too).
#[derive(Clone)]
pub struct RunningHandle {
    inner: Arc<Mutex<HandleInner>>,
}

enum HandleInner {
    /// A single child process. `cancel()` sends SIGTERM to the PID, which
    /// works regardless of whether the wait thread has already taken the
    /// `Child` out of the handle.
    Process(i32),
    /// A process group (used for external-terminal runs spawned via
    /// `setsid`). `cancel()` sends SIGTERM to the group.
    ProcessGroup(i32),
}

impl RunningHandle {
    fn from_pid(pid: i32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HandleInner::Process(pid))),
        }
    }

    fn from_pgid(pgid: i32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HandleInner::ProcessGroup(pgid))),
        }
    }

    pub fn cancel(&self) {
        if let Ok(inner) = self.inner.lock() {
            match *inner {
                HandleInner::Process(pid) => {
                    #[cfg(unix)]
                    unsafe {
                        libc::kill(pid, libc::SIGTERM);
                    }
                    #[cfg(not(unix))]
                    let _ = pid;
                }
                #[cfg(unix)]
                HandleInner::ProcessGroup(pgid) => {
                    // Negative pid means "send to process group" in kill(2).
                    unsafe {
                        libc::killpg(pgid, libc::SIGTERM);
                    }
                }
                #[cfg(not(unix))]
                HandleInner::ProcessGroup(_) => {}
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandRunner {
    pub log_limit: usize,
}

impl Default for CommandRunner {
    fn default() -> Self {
        Self { log_limit: 1000 }
    }
}

impl CommandRunner {
    pub fn run_capture(&self, spec: &CommandSpec) -> Result<String> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        let output = cmd.output()?;
        if !output.status.success() {
            return Err(anyhow!("command failed with status {}", output.status));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn run_streaming(
        &self,
        spec: CommandSpec,
        sender: Sender<LogEvent>,
        input_rx: Option<Receiver<String>>,
    ) -> Result<RunningHandle> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|err| {
            let _ = sender.send(LogEvent::Line(format!("Failed to spawn: {err}")));
            let _ = sender.send(LogEvent::Finished(1));
            anyhow!("failed to spawn {}: {err}", spec.program)
        })?;

        // Capture the PID before the child is moved into the wait thread, so
        // `RunningHandle::cancel` can SIGTERM it even after the wait thread
        // has taken ownership.
        let pid = child.id() as i32;
        let handle = RunningHandle::from_pid(pid);

        thread::spawn(move || {
            if let Some(mut stdin) = child.stdin.take() {
                if let Some(rx) = input_rx {
                    thread::spawn(move || {
                        for line in rx {
                            let mut input = line;
                            if !input.ends_with('\n') {
                                input.push('\n');
                            }
                            let _ = stdin.write_all(input.as_bytes());
                            let _ = stdin.flush();
                        }
                    });
                }
            }

            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            if let Some(out) = stdout {
                let tx = sender.clone();
                thread::spawn(move || stream_reader(out, tx));
            }

            if let Some(err) = stderr {
                let tx = sender.clone();
                thread::spawn(move || stream_reader(err, tx));
            }

            let status = match child.wait() {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1,
            };
            let _ = sender.send(LogEvent::Finished(status));
        });

        Ok(handle)
    }
}

/// Reads from a pipe in chunks, yielding complete lines immediately.
/// Also yields partial lines that match interactive prompt patterns
/// (e.g. "[Y/n]") so the UI can show a dialog before the process
/// blocks waiting for stdin.
fn stream_reader(reader: impl Read + Send + 'static, tx: Sender<LogEvent>) {
    let mut reader = BufReader::new(reader);
    let mut partial = Vec::new();
    let mut partial_yielded = false;
    let mut buf = [0u8; 4096];

    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => {
                if !partial.is_empty() {
                    let line = String::from_utf8_lossy(&partial)
                        .trim_end()
                        .to_string();
                    if !line.is_empty() {
                        let _ = tx.send(LogEvent::Line(line));
                    }
                }
                break;
            }
            Ok(n) => n,
            Err(err) => {
                let _ = tx.send(LogEvent::Line(format!("read error: {err}")));
                break;
            }
        };

        let mut start = 0;
        for i in 0..n {
            if buf[i] == b'\n' {
                partial.extend_from_slice(&buf[start..i]);
                let line = String::from_utf8_lossy(&partial)
                    .trim_end()
                    .to_string();
                if !line.is_empty() {
                    let _ = tx.send(LogEvent::Line(line));
                }
                partial.clear();
                partial_yielded = false;
                start = i + 1;
            }
        }

        if start < n {
            partial.extend_from_slice(&buf[start..n]);
            if !partial_yielded {
                let text = String::from_utf8_lossy(&partial);
                let lower = text.to_lowercase();
                if lower.contains("[y/n]")
                    || lower.contains("[y/a/n]")
                    || lower.contains("[y/N]")
                    || lower.contains("? (")
                {
                    let line = text.trim().to_string();
                    if !line.is_empty() {
                        let _ = tx.send(LogEvent::Line(line));
                        partial.clear();
                        partial_yielded = false;
                    }
                }
            }
        }
    }
}

impl CommandRunner {
    pub fn run_external_terminal(
        &self,
        spec: CommandSpec,
        preferred_terminal: TerminalEmulator,
        sender: Sender<LogEvent>,
    ) -> Result<RunningHandle> {
        let Some(terminal) = resolve_terminal(preferred_terminal) else {
            return Err(anyhow!(
                "No supported terminal found. Install kitty, konsole, or alacritty."
            ));
        };

        let display_line = spec.display_line();
        let shell_command = spec.shell_command();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let exit_file = std::env::temp_dir().join(format!("aurora-exit-{stamp}.code"));
        let exit_file_text = exit_file.to_string_lossy().to_string();

        let script = format!(
            "printf '%s\\n' {trace}\n{command}\ncode=$?\nprintf '%s\\n' \"$code\" > {exit_file}\nexit \"$code\"\n",
            trace = shell_quote(&format!("[aurora] Running: {display_line}")),
            command = shell_command,
            exit_file = shell_quote(&exit_file_text),
        );

        let mut cmd = Command::new(terminal.binary());
        cmd.args(terminal.launch_args(&script));

        // Put the terminal in its own session / process group so that a
        // cancel signal tears down the shell + pacman children cleanly
        // without affecting the user's other terminal windows.
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = sender.send(LogEvent::Line(format!(
                    "Failed to launch terminal {}: {err}",
                    terminal.label()
                )));
                let _ = sender.send(LogEvent::Finished(1));
                return Err(anyhow!("failed to launch terminal: {err}"));
            }
        };

        // The terminal is its own session leader (setsid above), so its PID
        // is also the process-group ID we can signal to tear down the whole
        // shell + pacman subtree.
        let pgid = child.id() as i32;
        let handle = RunningHandle::from_pgid(pgid);

        thread::spawn(move || {
            let status = match child.wait() {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1,
            };

            let final_code = std::fs::read_to_string(&exit_file)
                .ok()
                .and_then(|value| value.trim().parse::<i32>().ok())
                .unwrap_or(status);
            let _ = std::fs::remove_file(&exit_file);
            let _ = sender.send(LogEvent::Line(format!(
                "External terminal finished with exit code {final_code}"
            )));
            let _ = sender.send(LogEvent::Finished(final_code));
        });

        Ok(handle)
    }
}

fn shell_quote(input: &str) -> String {
    if input.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}

fn command_exists(name: &str) -> bool {
    if name.is_empty() || name.contains('/') {
        return false;
    }

    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&path_var).any(|dir| {
        let candidate = dir.join(name);
        let Ok(meta) = std::fs::metadata(&candidate) else {
            return false;
        };
        if !meta.is_file() {
            return false;
        }

        #[cfg(unix)]
        {
            meta.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

fn resolve_terminal(preferred: TerminalEmulator) -> Option<TerminalEmulator> {
    match preferred {
        TerminalEmulator::Auto => [TerminalEmulator::Kitty, TerminalEmulator::Konsole, TerminalEmulator::Alacritty]
            .into_iter()
            .find(|terminal| command_exists(terminal.binary())),
        _ => {
            if command_exists(preferred.binary()) {
                Some(preferred)
            } else {
                None
            }
        }
    }
}

impl TerminalEmulator {
    fn binary(self) -> &'static str {
        match self {
            TerminalEmulator::Auto => "",
            TerminalEmulator::Kitty => "kitty",
            TerminalEmulator::Konsole => "konsole",
            TerminalEmulator::Alacritty => "alacritty",
        }
    }

    fn launch_args(self, script: &str) -> Vec<String> {
        match self {
            TerminalEmulator::Kitty => vec![
                "sh".to_string(),
                "-lc".to_string(),
                script.to_string(),
            ],
            TerminalEmulator::Konsole => vec![
                "-e".to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                script.to_string(),
            ],
            TerminalEmulator::Alacritty => vec![
                "-e".to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                script.to_string(),
            ],
            TerminalEmulator::Auto => vec![
                "sh".to_string(),
                "-lc".to_string(),
                script.to_string(),
            ],
        }
    }
}
