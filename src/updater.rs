//! Auto-update from GitHub Releases.
//!
//! The release workflow publishes `latest.json` (version, download URL, SHA-256) next to the
//! Windows build. Wisp downloads the new `Wisp.exe` in the background, verifies its SHA-256, and
//! swaps it in on the next restart. The previous version is kept as `Wisp.exe.prev`; if the new
//! version keeps dying right after launch, the old one is put back automatically.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use crate::store;

/// A new session must stay up this long before an update is considered healthy.
pub const STABLE_AFTER_SECS: u64 = 20;
/// This many short-lived launches of a freshly installed version trigger a rollback.
const MAX_TRIAL_STARTS: u32 = 3;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pending {
    pub version: String,
    pub path: PathBuf,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Trial {
    pub from: String,
    pub to: String,
    pub starts: u32,
    pub stable: bool,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct State {
    pub pending: Option<Pending>,
    pub trial: Option<Trial>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Startup {
    /// Carry on starting normally.
    Continue,
    /// A staged update was applied and the new version has been launched: exit now.
    Relaunched,
    /// The new version kept failing, the previous one was restored and launched: exit now.
    RolledBack,
}

/// Order two versions the semver way: "1.2.3" > "1.2.3-beta.2" > "1.2.3-beta.1" > "1.2.2".
/// A leading "v" and build metadata ("+...") are ignored.
pub fn cmp_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn split(s: &str) -> (Vec<u64>, Vec<String>) {
        let s = s.trim().trim_start_matches(['v', 'V']);
        let s = s.split('+').next().unwrap_or("");
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, p),
            None => (s, ""),
        };
        let mut nums: Vec<u64> = core.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect();
        nums.resize(3.max(nums.len()), 0);
        let pre = if pre.is_empty() {
            vec![]
        } else {
            pre.split('.').map(String::from).collect()
        };
        (nums, pre)
    }
    let ((an, ap), (bn, bp)) = (split(a), split(b));
    match an.cmp(&bn) {
        Ordering::Equal => {}
        o => return o,
    }
    // No pre-release part means the final release, which comes after its pre-releases.
    match (ap.is_empty(), bp.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in ap.iter().zip(bp.iter()) {
        let o = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(i), Ok(j)) => i.cmp(&j),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            _ => x.to_lowercase().cmp(&y.to_lowercase()),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ap.len().cmp(&bp.len())
}

/// `true` when `tag` (e.g. "v1.2.3" or "0.3.0-beta.2") is a newer version than `cur`.
pub fn newer(tag: &str, cur: &str) -> bool {
    cmp_versions(tag, cur) == std::cmp::Ordering::Greater
}

pub fn parse_feed(json: &str) -> Result<Release, String> {
    let r: Release = serde_json::from_str(json).map_err(|e| format!("Bad update feed: {e}"))?;
    let hex_ok = r.sha256.len() == 64 && r.sha256.chars().all(|c| c.is_ascii_hexdigit());
    if !hex_ok || !r.url.starts_with("https://") || r.version.is_empty() {
        return Err("The update feed is missing a version, an https URL or a SHA-256.".into());
    }
    Ok(r)
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(secs))
        .user_agent("Wisp-updater")
        .build()
}

fn get_text(url: &str) -> Result<String, String> {
    agent(15)
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => "No releases published yet.".to_string(),
            ureq::Error::Status(c, _) => format!("GitHub returned HTTP {c}."),
            _ => "Couldn't reach GitHub. Check your connection.".to_string(),
        })?
        .into_string()
        .map_err(|e| e.to_string())
}

/// Look up the newest release on the chosen channel (the stable feed ignores pre-releases).
pub fn fetch_latest(owner: &str, repo: &str, beta: bool) -> Result<Release, String> {
    if !beta {
        return parse_feed(&get_text(&format!(
            "https://github.com/{owner}/{repo}/releases/latest/download/latest.json"
        ))?);
    }
    let list: serde_json::Value = serde_json::from_str(&get_text(&format!(
        "https://api.github.com/repos/{owner}/{repo}/releases?per_page=10"
    ))?)
    .map_err(|e| e.to_string())?;
    // The highest version among the recent releases (betas and stable ones), not just the newest
    // one created.
    let rel = list
        .as_array()
        .and_then(|a| {
            a.iter()
                .filter(|r| r["draft"] != true)
                .max_by(|x, y| cmp_versions(x["tag_name"].as_str().unwrap_or(""), y["tag_name"].as_str().unwrap_or("")))
        })
        .ok_or_else(|| "No releases published yet.".to_string())?;
    let url = rel["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == "latest.json"))
        .and_then(|x| x["browser_download_url"].as_str())
        .ok_or_else(|| "That release has no update feed.".to_string())?;
    parse_feed(&get_text(url)?)
}

/// Download `rel` into `dir`, verify its SHA-256, and return the verified file.
pub fn download(rel: &Release, dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let part = dir.join(format!("Wisp-{}.exe.part", rel.version));
    let done = dir.join(format!("Wisp-{}.exe", rel.version));
    let resp = agent(300)
        .get(&rel.url)
        .call()
        .map_err(|_| "Couldn't download the update.".to_string())?;
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|_| "The download was interrupted.".to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    drop(file);
    let got = hex(&hasher.finalize());
    if !got.eq_ignore_ascii_case(&rel.sha256) {
        let _ = fs::remove_file(&part);
        return Err("The downloaded update failed its SHA-256 check and was discarded.".into());
    }
    let _ = fs::remove_file(&done);
    fs::rename(&part, &done).map_err(|e| e.to_string())?;
    Ok(done)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&h.finalize()));
        }
        h.update(&buf[..n]);
    }
}

// ---------- state ----------

pub fn load_state() -> State {
    store::load_json("update.json")
}
/// Update bookkeeping must hit the disk right away (a crash is exactly when it matters).
pub fn save_state(s: &State) {
    store::save_json("update.json", s);
    store::flush();
}

// ---------- swapping files ----------

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Put `new` in place of `current`, keeping the old file as `<current>.prev`.
/// (Windows lets a running .exe be renamed, so this works while Wisp is running.)
pub fn swap_in(current: &Path, new: &Path) -> std::io::Result<()> {
    let prev = with_suffix(current, ".prev");
    let _ = fs::remove_file(&prev);
    fs::rename(current, &prev)?;
    if let Err(e) = fs::copy(new, current) {
        let _ = fs::remove_file(current);
        let _ = fs::rename(&prev, current); // put the old one back
        return Err(e);
    }
    let _ = fs::remove_file(new);
    Ok(())
}

/// Restore `<current>.prev`, keeping the failed file as `<current>.bad`.
pub fn restore_previous(current: &Path) -> std::io::Result<()> {
    let prev = with_suffix(current, ".prev");
    if !prev.is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no previous version"));
    }
    let bad = with_suffix(current, ".bad");
    let _ = fs::remove_file(&bad);
    fs::rename(current, &bad)?;
    fs::rename(&prev, current)
}

fn relaunch(exe: &Path, extra: &[&str]) {
    let mut c = std::process::Command::new(exe);
    c.args(extra);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_0008); // DETACHED_PROCESS
    }
    let _ = c.spawn();
}

/// The staged file must still match the checksum it was verified against when downloaded.
fn verified(p: &Pending) -> bool {
    p.path.is_file() && sha256_file(&p.path).map(|h| h.eq_ignore_ascii_case(&p.sha256)).unwrap_or(false)
}

/// Decide what to do at launch: roll back a failing update, apply a staged one, or carry on.
/// `exe` is the running executable, `cur` its version.
pub fn startup(exe: &Path, cur: &str) -> Startup {
    let mut st = load_state();
    if let Some(t) = st.trial.clone() {
        if t.to == cur && !t.stable {
            let starts = t.starts + 1;
            if starts >= MAX_TRIAL_STARTS {
                st.trial = None;
                st.pending = None;
                save_state(&st);
                store::flush();
                if restore_previous(exe).is_ok() {
                    relaunch(exe, &["--rolled-back"]);
                    return Startup::RolledBack;
                }
            } else {
                st.trial = Some(Trial { starts, ..t });
                save_state(&st);
            }
        } else if t.to != cur || t.stable {
            st.trial = None;
        }
    }
    if let Some(p) = st.pending.clone() {
        if newer(&p.version, cur) && verified(&p) {
            if swap_in(exe, &p.path).is_ok() {
                st.pending = None;
                st.trial = Some(Trial {
                    from: cur.to_string(),
                    to: p.version,
                    starts: 0,
                    stable: false,
                });
                save_state(&st);
                store::flush();
                relaunch(exe, &["--updated"]);
                return Startup::Relaunched;
            }
        } else {
            st.pending = None;
        }
    }
    save_state(&st);
    Startup::Continue
}

/// Called once a session has been up for `STABLE_AFTER_SECS`: the running version is healthy.
pub fn mark_stable(exe: &Path) {
    let mut st = load_state();
    if let Some(t) = st.trial.as_mut() {
        t.stable = true;
    }
    save_state(&st);
    let _ = fs::remove_file(with_suffix(exe, ".bad"));
    let _ = fs::remove_file(with_suffix(exe, ".old"));
}

/// Apply the staged update right now (the user clicked "Restart") and launch the new version.
pub fn apply_now(exe: &Path, cur: &str) -> Result<(), String> {
    match startup_apply_only(exe, cur) {
        true => Ok(()),
        false => Err("There is no update ready to install.".to_string()),
    }
}

fn startup_apply_only(exe: &Path, cur: &str) -> bool {
    let mut st = load_state();
    let Some(p) = st.pending.clone() else { return false };
    if !(newer(&p.version, cur) && verified(&p)) || swap_in(exe, &p.path).is_err() {
        return false;
    }
    st.pending = None;
    st.trial = Some(Trial {
        from: cur.to_string(),
        to: p.version,
        starts: 0,
        stable: false,
    });
    save_state(&st);
    store::flush();
    relaunch(exe, &["--updated"]);
    true
}

pub fn stage(version: &str, path: &Path, sha256: &str) {
    let mut st = load_state();
    st.pending = Some(Pending {
        version: version.to_string(),
        path: path.to_path_buf(),
        sha256: sha256.to_string(),
    });
    save_state(&st);
    store::flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wisp-upd-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn versions() {
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(newer("1.0.0", "0.9.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("v0.0.9", "0.1.0"));
        assert!(newer("0.1.10", "0.1.9"));
        // betas
        assert!(newer("v0.3.0-beta.2", "0.3.0-beta.1"));
        assert!(newer("0.3.0-beta.10", "0.3.0-beta.9"));
        assert!(!newer("0.3.0-beta.1", "0.3.0-beta.2"));
        assert!(!newer("0.3.0-beta.1", "0.3.0-beta.1"));
        assert!(newer("0.3.0", "0.3.0-beta.5"));
        assert!(!newer("0.3.0-beta.5", "0.3.0"));
        assert!(newer("0.3.1-beta.1", "0.3.0"));
        assert!(newer("0.4.0-beta.1", "0.3.0-beta.9"));
        assert!(newer("0.3.0-rc.1", "0.3.0-beta.3"));
        assert!(newer("0.2.10-beta.3", "0.2.10-beta.2"));
        assert!(!newer("v0.2.9", "0.2.10-beta.1"));
        assert!(!newer("1.0.0+build.7", "1.0.0"));
    }

    #[test]
    fn feed_validation() {
        let good = r#"{"version":"0.1.1","url":"https://github.com/o/r/releases/download/v0.1.1/Wisp.exe","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","notes":"x"}"#;
        assert_eq!(parse_feed(good).unwrap().version, "0.1.1");
        assert!(parse_feed(&good.replace("https://", "http://")).is_err());
        assert!(parse_feed(&good.replace("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "123")).is_err());
        assert!(parse_feed("{}").is_err());
    }

    #[test]
    fn sha256_known_value() {
        let d = tmp("sha");
        let f = d.join("a.bin");
        fs::write(&f, b"abc").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn swap_and_restore() {
        let d = tmp("swap");
        let (cur, new) = (d.join("Wisp.exe"), d.join("new.exe"));
        fs::write(&cur, b"v1").unwrap();
        fs::write(&new, b"v2").unwrap();
        swap_in(&cur, &new).unwrap();
        assert_eq!(fs::read(&cur).unwrap(), b"v2");
        assert_eq!(fs::read(with_suffix(&cur, ".prev")).unwrap(), b"v1");
        assert!(!new.exists());
        restore_previous(&cur).unwrap();
        assert_eq!(fs::read(&cur).unwrap(), b"v1");
        assert_eq!(fs::read(with_suffix(&cur, ".bad")).unwrap(), b"v2");
        assert!(restore_previous(&cur).is_err());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn failed_copy_keeps_old_version() {
        let d = tmp("failcopy");
        let cur = d.join("Wisp.exe");
        fs::write(&cur, b"v1").unwrap();
        assert!(swap_in(&cur, &d.join("missing.exe")).is_err());
        assert_eq!(fs::read(&cur).unwrap(), b"v1");
        let _ = fs::remove_dir_all(d);
    }

    // The tests below share one state file, so they run one at a time.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn isolated() -> std::sync::MutexGuard<'static, ()> {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| store::set_private(std::env::temp_dir().join(format!("wisp-upd-state-{}", std::process::id()))));
        let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        save_state(&State::default());
        store::flush();
        g
    }

    #[test]
    fn staged_update_is_verified_and_applied() {
        let _g = isolated();
        let d = tmp("apply");
        let (exe, new) = (d.join("Wisp.exe"), d.join("Wisp-0.2.0.exe"));
        fs::write(&exe, b"old").unwrap();
        fs::write(&new, b"new-build").unwrap();
        let sha = sha256_file(&new).unwrap();

        // wrong checksum: refused, nothing changes
        stage("0.2.0", &new, &"0".repeat(64));
        assert_eq!(startup(&exe, "0.1.0"), Startup::Continue);
        assert_eq!(fs::read(&exe).unwrap(), b"old");

        stage("0.2.0", &new, &sha);
        assert_eq!(startup(&exe, "0.1.0"), Startup::Relaunched);
        assert_eq!(fs::read(&exe).unwrap(), b"new-build");
        assert_eq!(fs::read(with_suffix(&exe, ".prev")).unwrap(), b"old");
        let st = load_state();
        assert!(st.pending.is_none());
        assert_eq!(st.trial.unwrap().to, "0.2.0");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn failing_update_rolls_back_after_three_short_launches() {
        let _g = isolated();
        let d = tmp("rollback");
        let exe = d.join("Wisp.exe");
        fs::write(&exe, b"broken-new").unwrap();
        fs::write(with_suffix(&exe, ".prev"), b"good-old").unwrap();
        save_state(&State {
            pending: None,
            trial: Some(Trial {
                from: "0.1.0".into(),
                to: "0.2.0".into(),
                starts: 0,
                stable: false,
            }),
        });
        assert_eq!(startup(&exe, "0.2.0"), Startup::Continue); // 1st launch of the new version
        assert_eq!(startup(&exe, "0.2.0"), Startup::Continue); // crashed/closed early again
        assert_eq!(startup(&exe, "0.2.0"), Startup::RolledBack); // third strike
        assert_eq!(fs::read(&exe).unwrap(), b"good-old");
        assert_eq!(fs::read(with_suffix(&exe, ".bad")).unwrap(), b"broken-new");
        assert!(load_state().trial.is_none());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn healthy_update_is_not_rolled_back() {
        let _g = isolated();
        let d = tmp("healthy");
        let exe = d.join("Wisp.exe");
        fs::write(&exe, b"new").unwrap();
        save_state(&State {
            pending: None,
            trial: Some(Trial {
                from: "0.1.0".into(),
                to: "0.2.0".into(),
                starts: 0,
                stable: false,
            }),
        });
        assert_eq!(startup(&exe, "0.2.0"), Startup::Continue);
        mark_stable(&exe);
        for _ in 0..5 {
            assert_eq!(startup(&exe, "0.2.0"), Startup::Continue);
        }
        assert_eq!(fs::read(&exe).unwrap(), b"new");
        let _ = fs::remove_dir_all(d);
    }
}
