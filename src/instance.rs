//! Single running instance + "open this link" hand-off (used when Wisp is the default browser).
//!
//! The first Wisp listens on a loopback port and writes `{port, token}` to the data folder; later
//! launches send their URL to it and exit. The token keeps other local programs from driving the browser.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};

use crate::store;

fn info_path() -> std::path::PathBuf {
    store::data_dir().join("instance.json")
}

/// A URL (or local HTML file) passed on the command line, as an openable URL.
pub fn url_from_args<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    for a in args.into_iter().skip(1) {
        if a.starts_with("--") {
            continue;
        }
        if a.starts_with("http://") || a.starts_with("https://") || a.starts_with("file://") {
            return Some(a);
        }
        let p = std::path::Path::new(&a);
        if p.is_file() {
            return url::Url::from_file_path(p).ok().map(|u| u.to_string());
        }
    }
    None
}

/// Try to hand `url` (may be empty = just focus the window) to an already running Wisp.
pub fn forward(url: &str) -> bool {
    let Ok(text) = fs::read_to_string(info_path()) else { return false };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    let (Some(port), Some(token)) = (v["port"].as_u64(), v["token"].as_str()) else {
        return false;
    };
    let Ok(mut s) = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port as u16)),
        Duration::from_millis(400),
    ) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(1500)));
    if writeln!(s, "{token}\n{}", url.replace(['\r', '\n'], "")).is_err() {
        return false;
    }
    let mut ack = String::new();
    BufReader::new(s).read_line(&mut ack).is_ok() && ack.trim() == "ok"
}

/// Start listening; `on_url` is called (on a background thread) for every hand-off.
pub fn listen(on_url: impl Fn(String) + Send + 'static) {
    let Ok(l) = TcpListener::bind(("127.0.0.1", 0)) else { return };
    let Ok(addr) = l.local_addr() else { return };
    let token = format!("{:x}{:x}", rand_u64(), rand_u64());
    let _ = fs::write(info_path(), serde_json::json!({"port": addr.port(), "token": token}).to_string());
    std::thread::spawn(move || {
        for conn in l.incoming().flatten() {
            let _ = conn.set_read_timeout(Some(Duration::from_millis(1500)));
            let mut r = BufReader::new(&conn);
            let (mut t, mut u) = (String::new(), String::new());
            if r.read_line(&mut t).is_ok() && r.read_line(&mut u).is_ok() && t.trim() == token {
                on_url(u.trim().to_string());
                let _ = (&conn).write_all(b"ok\n");
            }
        }
    });
}

fn rand_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            url_from_args(a(&["Wisp.exe", "https://a.com/x"])).as_deref(),
            Some("https://a.com/x")
        );
        assert_eq!(
            url_from_args(a(&["Wisp.exe", "--updated", "http://b.org"])).as_deref(),
            Some("http://b.org")
        );
        assert_eq!(url_from_args(a(&["Wisp.exe", "--incognito"])), None);
        assert_eq!(url_from_args(a(&["Wisp.exe", "javascript:alert(1)"])), None);
        assert_eq!(url_from_args(a(&["Wisp.exe"])), None);
    }
}
