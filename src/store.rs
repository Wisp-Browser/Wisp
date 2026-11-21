//! Tiny JSON persistence in the user data dir (settings + spaces/session).

use crate::layout::Split;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{mpsc, OnceLock},
    time::Duration,
};

static PRIVATE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Private windows keep everything in a throw-away folder that is deleted on exit.
pub fn set_private(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = PRIVATE_DIR.set(dir);
}
pub fn is_private() -> bool {
    PRIVATE_DIR.get().is_some()
}

/// Delete a private window's throw-away folder. WebView2 may still hold files for a moment, so on
/// Windows a hidden helper waits a few seconds and removes it after this process has exited.
pub fn cleanup_private() {
    let Some(dir) = PRIVATE_DIR.get() else { return };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .args(["/C", &format!("ping -n 4 127.0.0.1 >nul & rmdir /s /q \"{}\"", dir.display())])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn();
    }
    #[cfg(not(windows))]
    let _ = fs::remove_dir_all(dir);
}

/// The normal (non-private) data directory.
pub fn real_data_dir() -> PathBuf {
    // WISP_DATA_DIR runs Wisp on a separate data folder (testing, portable use).
    if let Some(d) = std::env::var_os("WISP_DATA_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Wisp")
}

pub fn data_dir() -> PathBuf {
    let d = PRIVATE_DIR.get().cloned().unwrap_or_else(real_data_dir);
    let _ = fs::create_dir_all(&d);
    d
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub engine: String,
    pub light: bool,
    pub accent: String,
    pub glass: f64,
    pub perf: bool,
    pub ai: Value,
    pub suggest: bool,
    pub blocker: bool,
    pub https_only: bool,
    /// Memory saver: unload tabs hidden for this many seconds (0 = off).
    pub hibernate_s: u32,
    pub shortcuts: Value,
    pub zoom: std::collections::HashMap<String, f64>,
    pub block_off: Vec<String>,
    pub auto_update: bool,
    pub channel: String,
    /// "classic" | "compact" | "vertical"
    pub layout: String,
    /// Glass effects (falls back to Flat when the OS can't blur the window).
    pub glass_on: bool,
    /// Vertical layout: tab sidebar collapsed to icons.
    pub side_collapsed: bool,
    /// The first-run setup still has to be shown.
    pub setup: bool,
    /// Interface font: "inter" | "rounded" | "system" | "serif" | "mono".
    pub font: String,
    /// Bookmarks bar under the toolbar (shown when there are bookmarks).
    pub bmbar: bool,
    /// Extensions pinned to the toolbar (their IDs, in order).
    pub ext_pins: Vec<String>,
    /// Offer to save passwords when signing in.
    pub pw_save: bool,
    /// Sites where Wisp never offers to save passwords (origins).
    pub pw_never: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: "ddg".into(),
            light: false,
            accent: "#5e5ce6".into(),
            glass: 0.5,
            perf: false,
            ai: Value::Null,
            suggest: true,
            blocker: true,
            https_only: false,
            hibernate_s: 30,
            shortcuts: Value::Null,
            zoom: Default::default(),
            block_off: vec![],
            auto_update: true,
            channel: "stable".into(),
            // Settings files from before layouts existed keep the old sidebar arrangement.
            layout: "vertical".into(),
            glass_on: true,
            side_collapsed: false,
            setup: false,
            font: "inter".into(),
            bmbar: true,
            ext_pins: vec![],
            pw_save: true,
            pw_never: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TabP {
    pub id: usize,
    pub url: String,
    pub title: String,
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SpaceP {
    pub id: usize,
    pub name: String,
    pub color: String,
    pub tabs: Vec<TabP>,
    pub active: usize,
    pub split: Option<Split>,
    pub nt: Value,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct State {
    pub spaces: Vec<SpaceP>,
    pub cur: usize,
    pub next_id: usize,
}

fn load<T: for<'de> Deserialize<'de> + Default>(name: &str) -> T {
    fs::read_to_string(data_dir().join(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
enum Job {
    Write(String, String),
    Flush(mpsc::Sender<()>),
}

/// All disk writes go through one background thread so the UI thread never waits on the disk
/// (antivirus scans and slow drives made synchronous writes visibly stall the whole window).
fn writer() -> &'static mpsc::Sender<Job> {
    static W: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    W.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            while let Ok(first) = rx.recv() {
                // Coalesce: keep only the newest content per file, then write once.
                let mut latest: Vec<(String, String)> = vec![];
                let mut flushes = vec![];
                let mut handle = |j: Job| match j {
                    Job::Write(n, c) => match latest.iter_mut().find(|(k, _)| *k == n) {
                        Some(e) => e.1 = c,
                        None => latest.push((n, c)),
                    },
                    Job::Flush(f) => flushes.push(f),
                };
                handle(first);
                while let Ok(j) = rx.try_recv() {
                    handle(j);
                }
                for (name, content) in latest {
                    let tmp = data_dir().join(format!("{name}.tmp"));
                    if fs::write(&tmp, content).is_ok() {
                        let _ = fs::rename(tmp, data_dir().join(&name));
                    }
                }
                for f in flushes {
                    let _ = f.send(());
                }
            }
        });
        tx
    })
}

fn save_str(name: &str, s: &str) {
    let _ = writer().send(Job::Write(name.to_string(), s.to_string()));
}

/// Block (briefly) until queued writes are on disk. Call before exiting.
pub fn flush() {
    let (tx, rx) = mpsc::channel();
    if writer().send(Job::Flush(tx)).is_ok() {
        let _ = rx.recv_timeout(Duration::from_millis(800));
    }
}

pub fn load_json<T: for<'de> Deserialize<'de> + Default>(name: &str) -> T {
    load(name)
}
pub fn save_json<T: Serialize>(name: &str, v: &T) {
    if let Ok(j) = serde_json::to_string(v) {
        save_str(name, &j)
    }
}

pub fn load_settings() -> Settings {
    load("settings.json")
}
pub fn save_settings(s: &Settings) {
    if let Ok(j) = serde_json::to_string_pretty(s) {
        save_str("settings.json", &j)
    }
}
pub fn load_state() -> State {
    load("state.json")
}
pub fn save_state(s: &State) -> Option<String> {
    let j = serde_json::to_string(s).ok()?;
    save_str("state.json", &j);
    Some(j)
}
