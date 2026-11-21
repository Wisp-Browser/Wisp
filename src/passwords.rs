//! Saved passwords: an encrypted vault (Windows DPAPI, tied to the Windows account), import from
//! Chrome / Edge / Brave and from CSV files (every browser and password manager can export one),
//! and CSV export.
//!
//! Passkeys can't be imported: browsers and Google Password Manager don't let passkeys leave them.
//! Websites' passkey sign-in in Wisp goes through Windows (Windows Hello, or a phone).

use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE: &str = "passwords.dat";

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Login {
    pub id: u64,
    /// scheme://host[:port]
    pub origin: String,
    pub username: String,
    pub password: String,
    pub note: String,
    pub created: u64,
    pub used: u64,
}

#[derive(Default)]
pub struct Vault {
    pub logins: Vec<Login>,
    next: u64,
}

pub enum Upsert {
    Added,
    Updated,
    Same,
}

/// The origin (scheme://host[:port]) of a web address, for http(s) pages.
pub fn origin_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url.trim()).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let host = u.host_str()?;
    if host.is_empty() || host == "wisp.localhost" {
        return None;
    }
    Some(match u.port() {
        Some(p) => format!("{}://{host}:{p}", u.scheme()),
        None => format!("{}://{host}", u.scheme()),
    })
}

/// The site name shown for an origin: its host without "www.".
pub fn site_of(origin: &str) -> String {
    let h = origin.split("://").nth(1).unwrap_or(origin);
    h.trim_start_matches("www.").to_string()
}

impl Vault {
    pub fn load() -> Vault {
        // Private windows use the saved passwords too (they just don't save new ones).
        let path = crate::store::real_data_dir().join(FILE);
        let logins: Vec<Login> = std::fs::read(&path)
            .ok()
            .and_then(|b| unprotect(&b).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let next = logins.iter().map(|l| l.id).max().unwrap_or(0) + 1;
        Vault { logins, next }
    }

    pub fn save(&self) {
        let Ok(json) = serde_json::to_vec(&self.logins) else { return };
        let Ok(enc) = protect(&json) else { return };
        let path = crate::store::real_data_dir().join(FILE);
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, enc).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    /// Logins saved for exactly this origin (an https password is never offered to an http page).
    pub fn for_origin(&self, origin: &str) -> Vec<&Login> {
        self.logins.iter().filter(|l| l.origin == origin).collect()
    }

    pub fn upsert(&mut self, origin: &str, username: &str, password: &str) -> Upsert {
        let now = crate::library::now();
        if let Some(l) = self.logins.iter_mut().find(|l| l.origin == origin && l.username == username) {
            if l.password == password {
                l.used = now;
                return Upsert::Same;
            }
            l.password = password.to_string();
            l.used = now;
            return Upsert::Updated;
        }
        self.logins.push(Login {
            id: self.next,
            origin: origin.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            note: String::new(),
            created: now,
            used: now,
        });
        self.next += 1;
        Upsert::Added
    }

    pub fn get(&self, id: u64) -> Option<&Login> {
        self.logins.iter().find(|l| l.id == id)
    }

    pub fn delete(&mut self, id: u64) -> bool {
        let n = self.logins.len();
        self.logins.retain(|l| l.id != id);
        n != self.logins.len()
    }

    /// Add imported logins. Returns (added, updated, unchanged).
    pub fn merge(&mut self, items: Vec<(String, String, String, String)>) -> (usize, usize, usize) {
        let (mut a, mut u, mut s) = (0, 0, 0);
        for (url, user, pass, note) in items {
            let Some(origin) = origin_of(&url) else { continue };
            if pass.is_empty() {
                continue;
            }
            match self.upsert(&origin, &user, &pass) {
                Upsert::Added => {
                    a += 1;
                    if !note.is_empty() {
                        if let Some(l) = self.logins.last_mut() {
                            l.note = note;
                        }
                    }
                }
                Upsert::Updated => u += 1,
                Upsert::Same => s += 1,
            }
        }
        (a, u, s)
    }

    /// Chrome's CSV format (name,url,username,password,note), which every browser can import.
    pub fn to_csv(&self) -> String {
        let q = |s: &str| {
            if s.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_string()
            }
        };
        let mut out = String::from("name,url,username,password,note\n");
        for l in &self.logins {
            out.push_str(&format!(
                "{},{},{},{},{}\n",
                q(&site_of(&l.origin)),
                q(&l.origin),
                q(&l.username),
                q(&l.password),
                q(&l.note)
            ));
        }
        out
    }
}

/// Parse CSV text (RFC 4180: quotes, doubled quotes, newlines inside quotes).
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                if row.iter().any(|f| !f.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Logins from a password CSV exported by Chrome, Edge, Brave, Opera, Firefox, Safari, Bitwarden,
/// 1Password, LastPass, Dashlane, Proton Pass, KeePass and others: columns are found by name.
pub fn parse_csv(text: &str) -> Result<Vec<(String, String, String, String)>, String> {
    let rows = csv_rows(text);
    let Some(head) = rows.first() else {
        return Err("That file is empty.".into());
    };
    let head: Vec<String> = head.iter().map(|h| h.trim().to_lowercase()).collect();
    let col = |names: &[&str]| head.iter().position(|h| names.contains(&h.as_str()));
    let url = col(&[
        "url",
        "login_uri",
        "website",
        "web site",
        "origin",
        "hostname",
        "login url",
        "address",
    ]);
    let user = col(&["username", "login_username", "user name", "login", "email", "user", "login name"]);
    let pass = col(&["password", "login_password", "pass"]);
    let note = col(&["note", "notes", "extra", "comments"]);
    let (Some(url), Some(pass)) = (url, pass) else {
        return Err("That file doesn't look like a passwords export (it needs url and password columns).".into());
    };
    let get = |r: &Vec<String>, i: Option<usize>| i.and_then(|i| r.get(i)).cloned().unwrap_or_default();
    Ok(rows
        .iter()
        .skip(1)
        .map(|r| {
            let mut u = get(r, Some(url));
            // Some exports leave out the scheme.
            if !u.is_empty() && !u.contains("://") {
                u = format!("https://{u}");
            }
            (u, get(r, user), get(r, Some(pass)), get(r, note))
        })
        .filter(|(u, _, p, _)| !u.is_empty() && !p.is_empty())
        .collect())
}

/// Chromium browsers whose saved passwords can be read on this PC: (id, name, user data folder).
pub fn chromium_sources() -> Vec<(&'static str, &'static str, std::path::PathBuf)> {
    let Some(l) = dirs::data_local_dir() else { return vec![] };
    [
        ("chrome", "Google Chrome", "Google/Chrome/User Data"),
        ("edge", "Microsoft Edge", "Microsoft/Edge/User Data"),
        ("brave", "Brave", "BraveSoftware/Brave-Browser/User Data"),
        ("vivaldi", "Vivaldi", "Vivaldi/User Data"),
    ]
    .into_iter()
    .map(|(id, name, rel)| (id, name, l.join(rel)))
    .filter(|(_, _, p)| p.join("Local State").is_file())
    .collect()
}

/// Open a browser's own password page, where it can export a CSV (for passwords it keeps locked).
pub fn open_export_page(id: &str) -> Result<(), String> {
    let (exe, page): (&[&str], &str) = match id {
        "chrome" => (&["Google/Chrome/Application/chrome.exe"], "chrome://password-manager/settings"),
        "edge" => (&["Microsoft/Edge/Application/msedge.exe"], "edge://wallet/passwords"),
        "brave" => (
            &["BraveSoftware/Brave-Browser/Application/brave.exe"],
            "brave://password-manager/settings",
        ),
        "vivaldi" => (&["Vivaldi/Application/vivaldi.exe"], "vivaldi://password-manager/settings"),
        _ => return Err("Unknown browser".into()),
    };
    let roots = [
        dirs::data_local_dir(),
        std::env::var_os("ProgramFiles").map(Into::into),
        std::env::var_os("ProgramFiles(x86)").map(Into::into),
    ];
    let path = roots
        .iter()
        .flatten()
        .flat_map(|r| exe.iter().map(move |e| r.join(e)))
        .find(|p| p.is_file())
        .ok_or("Couldn't find that browser's program")?;
    std::process::Command::new(path)
        .arg(page)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub struct BrowserImport {
    pub logins: Vec<(String, String, String, String)>,
    /// Passwords the browser keeps locked to itself (Chrome's app-bound encryption).
    pub locked: usize,
}

/// Saved passwords from a Chromium browser's profiles (`Login Data`, decrypted with the key in
/// `Local State`, which Windows protects for this user). Passwords under Chrome's newer app-bound
/// encryption ("v20") can only be read by that browser and are counted in `locked`.
pub fn read_chromium(user_data: &Path) -> Result<BrowserImport, String> {
    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(user_data.join("Local State")).map_err(|_| "Couldn't read that browser's settings.")?,
    )
    .map_err(|_| "Couldn't read that browser's settings.")?;
    let key = state["os_crypt"]["encrypted_key"]
        .as_str()
        .and_then(b64decode)
        .and_then(|k| k.strip_prefix(b"DPAPI").map(|k| k.to_vec()))
        .and_then(|k| unprotect(&k).ok());
    let mut profiles: Vec<std::path::PathBuf> = std::fs::read_dir(user_data)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("Login Data").is_file())
        .collect();
    profiles.sort();
    if profiles.is_empty() {
        return Err("That browser has no saved passwords on this PC.".into());
    }
    let mut out = BrowserImport { logins: vec![], locked: 0 };
    for p in profiles {
        let tmp = crate::library::copy_db(&p.join("Login Data"), "passwords")?;
        let rows = (|| -> Result<Vec<(String, String, Vec<u8>)>, rusqlite::Error> {
            let c = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            let mut st = c.prepare("SELECT origin_url, username_value, password_value FROM logins WHERE blacklisted_by_user = 0")?;
            let r = st.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    r.get::<_, Option<Vec<u8>>>(2)?.unwrap_or_default(),
                ))
            })?;
            r.collect()
        })();
        let _ = std::fs::remove_file(&tmp);
        for (url, user, blob) in rows.map_err(|e| format!("Couldn't read the saved passwords: {e}"))? {
            match decrypt_chromium(&blob, key.as_deref()) {
                Some(pass) if !pass.is_empty() => out.logins.push((url, user, pass, String::new())),
                Some(_) => {}
                None => out.locked += 1,
            }
        }
    }
    Ok(out)
}

/// One `password_value`: "v10"/"v11" + 12-byte nonce + AES-256-GCM, or plain DPAPI (old versions).
pub fn decrypt_chromium(blob: &[u8], key: Option<&[u8]>) -> Option<String> {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    if blob.is_empty() {
        return Some(String::new());
    }
    if blob.starts_with(b"v10") || blob.starts_with(b"v11") {
        let key = key.filter(|k| k.len() == 32)?;
        if blob.len() < 3 + 12 + 16 {
            return None;
        }
        let cipher = Aes256Gcm::new_from_slice(key).ok()?;
        let plain = cipher.decrypt(Nonce::from_slice(&blob[3..15]), &blob[15..]).ok()?;
        return String::from_utf8(plain).ok();
    }
    if blob.starts_with(b"v20") {
        return None;
    }
    unprotect(blob).ok().and_then(|b| String::from_utf8(b).ok())
}

fn b64decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Encrypt for this Windows user (DPAPI).
#[cfg(windows)]
pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    dpapi(data, true)
}
/// Decrypt data protected for this Windows user (DPAPI).
#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    dpapi(data, false)
}

#[cfg(windows)]
fn dpapi(data: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at `data` for the duration of the call; the output buffer is
    // allocated by Windows, copied, then freed with LocalFree.
    unsafe {
        let r = if encrypt {
            CryptProtectData(
                &input,
                windows::core::w!("Wisp"),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        } else {
            CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        r.map_err(|e| e.to_string())?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(out.pbData as _));
        Ok(v)
    }
}

/// Non-Windows builds exist only so CI can compile and test the shared logic.
#[cfg(not(windows))]
pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}
#[cfg(not(windows))]
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}

/// Ask Windows to confirm it's the user (Windows Hello face, fingerprint or PIN) before showing,
/// copying or exporting passwords. `Ok(true)` also when no Windows Hello is set up on this PC.
#[cfg(windows)]
pub fn verify_user(hwnd: isize, message: &str) -> Result<bool, String> {
    use windows::{
        core::HSTRING,
        Foundation::IAsyncOperation,
        Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability},
        Win32::{
            Foundation::HWND,
            System::{
                Com::{CoInitializeEx, COINIT_MULTITHREADED},
                WinRT::IUserConsentVerifierInterop,
            },
        },
    };
    // SAFETY: initialises COM for this (background) thread; WinRT calls follow.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let avail = UserConsentVerifier::CheckAvailabilityAsync()
        .and_then(|op| op.get())
        .map_err(|e| e.to_string())?;
    if avail != UserConsentVerifierAvailability::Available {
        return Ok(true);
    }
    let interop = windows::core::factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(|e| e.to_string())?;
    // SAFETY: our own window handle; the returned async operation is awaited on this thread.
    let op: IAsyncOperation<UserConsentVerificationResult> =
        unsafe { interop.RequestVerificationForWindowAsync(HWND(hwnd as _), &HSTRING::from(message)) }.map_err(|e| e.to_string())?;
    Ok(op.get().map_err(|e| e.to_string())? == UserConsentVerificationResult::Verified)
}
#[cfg(not(windows))]
pub fn verify_user(_hwnd: isize, _message: &str) -> Result<bool, String> {
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origins() {
        assert_eq!(
            origin_of("https://www.example.com/login?x=1").as_deref(),
            Some("https://www.example.com")
        );
        assert_eq!(origin_of("http://localhost:3000/a").as_deref(), Some("http://localhost:3000"));
        assert_eq!(origin_of("http://wisp.localhost/newtab"), None);
        assert_eq!(origin_of("chrome-extension://abc/x"), None);
        assert_eq!(site_of("https://www.example.com"), "example.com");
    }
    #[test]
    fn csv_formats() {
        // Chrome / Edge
        let c = parse_csv("name,url,username,password,note\nex,https://ex.com/,a@b.c,\"p,\"\"w\",\n").unwrap();
        assert_eq!(c, vec![("https://ex.com/".into(), "a@b.c".into(), "p,\"w".into(), "".into())]);
        // Firefox
        let f = parse_csv("\"url\",\"username\",\"password\",\"httpRealm\"\r\n\"https://x.org\",\"me\",\"pw\",\"\"\r\n").unwrap();
        assert_eq!(f[0].1, "me");
        // Bitwarden
        let b = parse_csv("folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n,,login,X,hi,,0,x.net,u,p,\n").unwrap();
        assert_eq!(b[0], ("https://x.net".into(), "u".into(), "p".into(), "hi".into()));
        assert!(parse_csv("a,b\n1,2\n").is_err());
    }
    #[test]
    fn vault_merge_and_csv() {
        let mut v = Vault::default();
        let (a, u, s) = v.merge(vec![
            ("https://a.com/x".into(), "u".into(), "1".into(), "".into()),
            ("https://a.com/y".into(), "u".into(), "2".into(), "".into()),
            ("https://a.com".into(), "u".into(), "2".into(), "".into()),
        ]);
        assert_eq!((a, u, s), (1, 1, 1));
        assert_eq!(v.for_origin("https://a.com").len(), 1);
        assert!(v.for_origin("http://a.com").is_empty());
        let back = parse_csv(&v.to_csv()).unwrap();
        assert_eq!(back[0].2, "2");
    }
    /// A fake Chrome profile, encrypted the way Chrome does it on Windows (DPAPI-wrapped AES key in
    /// Local State, AES-256-GCM "v10" passwords in Login Data), read back by the importer.
    #[cfg(windows)]
    #[test]
    fn chromium_profile_roundtrip() {
        use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
        let dir = std::env::temp_dir().join(format!("wisp-pwtest-{}", std::process::id()));
        let prof = dir.join("Default");
        std::fs::create_dir_all(&prof).unwrap();
        let key = [42u8; 32];
        let mut wrapped = b"DPAPI".to_vec();
        wrapped.extend(protect(&key).unwrap());
        let b64 = {
            const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut o = String::new();
            for c in wrapped.chunks(3) {
                let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                for i in 0..4 {
                    o.push(if i <= c.len() {
                        T[(n >> (18 - 6 * i)) as usize & 63] as char
                    } else {
                        '='
                    });
                }
            }
            o
        };
        std::fs::write(dir.join("Local State"), format!("{{\"os_crypt\":{{\"encrypted_key\":\"{b64}\"}}}}")).unwrap();
        let enc = |p: &str| {
            let mut b = b"v10".to_vec();
            b.extend([3u8; 12]);
            b.extend(
                Aes256Gcm::new_from_slice(&key)
                    .unwrap()
                    .encrypt(Nonce::from_slice(&[3u8; 12]), p.as_bytes())
                    .unwrap(),
            );
            b
        };
        let c = rusqlite::Connection::open(prof.join("Login Data")).unwrap();
        c.execute_batch("CREATE TABLE logins (origin_url TEXT, username_value TEXT, password_value BLOB, blacklisted_by_user INTEGER)")
            .unwrap();
        c.execute("INSERT INTO logins VALUES ('https://a.com/login','me',?1,0)", [enc("s3cret")])
            .unwrap();
        c.execute(
            "INSERT INTO logins VALUES ('https://b.com/','you',?1,0)",
            [b"v20lockedlockedlockedlockedlocked".to_vec()],
        )
        .unwrap();
        drop(c);
        let r = read_chromium(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            r.logins,
            vec![("https://a.com/login".into(), "me".into(), "s3cret".into(), "".into())]
        );
        assert_eq!(r.locked, 1);
    }

    #[test]
    fn chromium_v10() {
        use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
        let key = [7u8; 32];
        let nonce = [1u8; 12];
        let ct = Aes256Gcm::new_from_slice(&key)
            .unwrap()
            .encrypt(Nonce::from_slice(&nonce), b"hunter2".as_ref())
            .unwrap();
        let mut blob = b"v10".to_vec();
        blob.extend(nonce);
        blob.extend(ct);
        assert_eq!(decrypt_chromium(&blob, Some(&key)).as_deref(), Some("hunter2"));
        assert_eq!(decrypt_chromium(b"v20xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", Some(&key)), None);
        assert_eq!(b64decode("RFBBUEk=").unwrap(), b"DPAPI");
    }
}
