//! AI sidebar backend: provider requests with streaming, model lists, and API keys in the OS keychain.

use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const PROVIDERS: &[&str] = &["gemini", "groq", "openrouter", "ollama"];

/// Built-in free model: no account or API key. Served anonymously by Pollinations (rate-limited).
pub const FREE_URL: &str = "https://text.pollinations.ai/openai";
pub const FREE_MODEL: &str = "openai";

#[derive(Clone, Debug)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

pub struct Req {
    pub provider: String,
    pub model: String,
    pub system: String,
    pub messages: Vec<Msg>,
    pub ollama: String,
}

// ---------- keys (Windows Credential Manager) ----------

fn entry(p: &str) -> Result<keyring::Entry, String> {
    if !PROVIDERS.contains(&p) {
        return Err("unknown provider".into());
    }
    keyring::Entry::new("Wisp", &format!("ai-{p}")).map_err(|e| e.to_string())
}
pub fn get_key(p: &str) -> Option<String> {
    entry(p).ok()?.get_password().ok().filter(|k| !k.trim().is_empty())
}
pub fn set_key(p: &str, key: &str) -> Result<(), String> {
    entry(p)?
        .set_password(key.trim())
        .map_err(|e| format!("Couldn't save the key to the system keychain: {e}"))
}
pub fn del_key(p: &str) {
    if let Ok(e) = entry(p) {
        let _ = e.delete_credential();
    }
}

// ---------- parsing helpers (unit-tested) ----------

/// Delta text from an OpenAI-compatible streaming chunk (Groq, OpenRouter).
pub fn openai_delta(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    v["choices"][0]["delta"]["content"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Delta text from a Gemini `streamGenerateContent?alt=sse` chunk.
pub fn gemini_delta(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    let parts = v["candidates"][0]["content"]["parts"].as_array()?;
    let t: String = parts.iter().filter_map(|p| p["text"].as_str()).collect();
    (!t.is_empty()).then_some(t)
}

/// (delta, done) from one Ollama NDJSON line.
pub fn ollama_delta(line: &str) -> (Option<String>, bool) {
    match serde_json::from_str::<Value>(line) {
        Ok(v) => (
            v["message"]["content"].as_str().filter(|s| !s.is_empty()).map(String::from),
            v["done"].as_bool().unwrap_or(false),
        ),
        Err(_) => (None, false),
    }
}

fn error_in(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    let e = &v["error"];
    e["message"].as_str().or_else(|| e.as_str()).map(String::from)
}

fn http_err(code: u16, body: String) -> String {
    let detail = error_in(&body).unwrap_or_else(|| body.chars().take(200).collect());
    match code {
        401 | 403 => format!("The API key was rejected (HTTP {code}). Check it in the AI panel. {detail}"),
        404 => format!("Model or endpoint not found (HTTP 404). Pick another model. {detail}"),
        429 => "Rate limit reached. Free tiers are limited, so wait a moment or pick another model.".into(),
        _ => format!("Request failed (HTTP {code}). {detail}"),
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(90))
        .user_agent("Wisp")
        .build()
}

fn clean_base(u: &str) -> String {
    let u = u.trim().trim_end_matches('/');
    if u.starts_with("http://") || u.starts_with("https://") {
        u.to_string()
    } else {
        "http://127.0.0.1:11434".to_string()
    }
}

fn post(url: &str, headers: &[(&str, String)], body: &Value, local: bool) -> Result<ureq::Response, String> {
    let mut r = agent().post(url).set("Content-Type", "application/json");
    for (k, v) in headers {
        r = r.set(k, v);
    }
    match r.send_string(&body.to_string()) {
        Ok(x) => Ok(x),
        Err(ureq::Error::Status(c, resp)) => Err(http_err(c, resp.into_string().unwrap_or_default())),
        Err(e) => Err(if local {
            format!(
                "Can't reach Ollama. Is it running, and is the address right?{}",
                if cfg!(debug_assertions) { format!(" ({e})") } else { String::new() }
            )
        } else {
            format!("Network error: {e}")
        }),
    }
}

fn get(url: &str, headers: &[(&str, String)], local: bool) -> Result<Value, String> {
    let mut r = agent().get(url);
    for (k, v) in headers {
        r = r.set(k, v);
    }
    match r.call() {
        Ok(x) => {
            let body = x.into_string().map_err(|e| e.to_string())?;
            serde_json::from_str::<Value>(&body).map_err(|e| e.to_string())
        }
        Err(ureq::Error::Status(c, resp)) => Err(http_err(c, resp.into_string().unwrap_or_default())),
        Err(e) => Err(if local {
            format!(
                "Can't reach Ollama. Is it running, and is the address right?{}",
                if cfg!(debug_assertions) { format!(" ({e})") } else { String::new() }
            )
        } else {
            format!("Network error: {e}")
        }),
    }
}

// ---------- model lists ----------

pub fn list_models(provider: &str, ollama: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = match provider {
        "free" => vec![FREE_MODEL.to_string()],
        "gemini" => {
            let key = get_key("gemini").ok_or("No API key saved.")?;
            let v = get(
                "https://generativelanguage.googleapis.com/v1beta/models?pageSize=200",
                &[("x-goog-api-key", key)],
                false,
            )?;
            v["models"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|m| {
                            m["supportedGenerationMethods"]
                                .as_array()
                                .is_some_and(|g| g.iter().any(|x| x == "generateContent"))
                        })
                        .filter_map(|m| m["name"].as_str())
                        .map(|n| n.trim_start_matches("models/").to_string())
                        .filter(|n| n.starts_with("gemini") && !n.contains("embedding") && !n.contains("tts") && !n.contains("image"))
                        .collect()
                })
                .unwrap_or_default()
        }
        "groq" => {
            let key = get_key("groq").ok_or("No API key saved.")?;
            let v = get(
                "https://api.groq.com/openai/v1/models",
                &[("Authorization", format!("Bearer {key}"))],
                false,
            )?;
            v["data"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m["id"].as_str())
                        .filter(|n| !["whisper", "tts", "guard", "embed"].iter().any(|b| n.contains(b)))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default()
        }
        "openrouter" => {
            let v = get("https://openrouter.ai/api/v1/models", &[], false)?;
            v["data"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m["id"].as_str())
                        .filter(|n| n.ends_with(":free"))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default()
        }
        "ollama" => {
            let v = get(&format!("{}/api/tags", clean_base(ollama)), &[], true)?;
            v["models"]
                .as_array()
                .map(|a| a.iter().filter_map(|m| m["name"].as_str()).map(String::from).collect())
                .unwrap_or_default()
        }
        _ => return Err("unknown provider".into()),
    };
    out.sort();
    out.dedup();
    Ok(out)
}

// ---------- streaming chat ----------

fn openai_messages(req: &Req) -> Vec<Value> {
    let mut m = vec![];
    if !req.system.is_empty() {
        m.push(json!({"role":"system","content":req.system}));
    }
    for x in &req.messages {
        m.push(json!({"role":x.role,"content":x.content}));
    }
    m
}

/// Stream a chat completion, calling `on` with each text delta. Returns when finished or stopped.
pub fn stream(req: &Req, stop: &AtomicBool, on: &mut dyn FnMut(&str)) -> Result<(), String> {
    if req.model.trim().is_empty() {
        return Err("No model selected. Pick one from the model menu.".into());
    }
    let resp = match req.provider.as_str() {
        "gemini" => {
            let key = get_key("gemini").ok_or("No Gemini API key saved.")?;
            let contents: Vec<Value> = req
                .messages
                .iter()
                .map(|m| json!({"role": if m.role == "assistant" {"model"} else {"user"}, "parts":[{"text":m.content}]}))
                .collect();
            let mut body = json!({"contents":contents});
            if !req.system.is_empty() {
                body["systemInstruction"] = json!({"parts":[{"text":req.system}]});
            }
            post(
                &format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
                    req.model
                ),
                &[("x-goog-api-key", key)],
                &body,
                false,
            )?
        }
        "groq" | "openrouter" => {
            let key = get_key(&req.provider).ok_or("No API key saved for this provider.")?;
            let (url, extra): (&str, Vec<(&str, String)>) = if req.provider == "groq" {
                ("https://api.groq.com/openai/v1/chat/completions", vec![])
            } else {
                (
                    "https://openrouter.ai/api/v1/chat/completions",
                    vec![
                        ("HTTP-Referer", "https://github.com/Wisp-Browser/Wisp".to_string()),
                        ("X-Title", "Wisp".to_string()),
                    ],
                )
            };
            let mut h = vec![("Authorization", format!("Bearer {key}"))];
            h.extend(extra);
            post(
                url,
                &h,
                &json!({"model":req.model,"messages":openai_messages(req),"stream":true}),
                false,
            )?
        }
        "free" => post(
            FREE_URL,
            &[],
            &json!({"model":FREE_MODEL,"messages":openai_messages(req),"stream":true,"referrer":"wisp-browser"}),
            false,
        )?,
        "ollama" => post(
            &format!("{}/api/chat", clean_base(&req.ollama)),
            &[],
            &json!({"model":req.model,"messages":openai_messages(req),"stream":true}),
            true,
        )?,
        _ => return Err("unknown provider".into()),
    };

    let reader = BufReader::new(resp.into_reader());
    for line in reader.lines() {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let line = line.map_err(|e| format!("Connection lost: {e}"))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if req.provider == "ollama" {
            if let Some(e) = error_in(line) {
                return Err(e);
            }
            let (d, done) = ollama_delta(line);
            if let Some(t) = d {
                on(&t);
            }
            if done {
                break;
            }
            continue;
        }
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        if let Some(e) = error_in(data) {
            return Err(e);
        }
        let d = if req.provider == "gemini" {
            gemini_delta(data)
        } else {
            openai_delta(data)
        };
        if let Some(t) = d {
            on(&t);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai() {
        assert_eq!(openai_delta(r#"{"choices":[{"delta":{"content":"Hi"}}]}"#).as_deref(), Some("Hi"));
        assert_eq!(openai_delta(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#), None);
    }
    #[test]
    fn parses_gemini() {
        let j = r#"{"candidates":[{"content":{"parts":[{"text":"Hel"},{"text":"lo"}]}}]}"#;
        assert_eq!(gemini_delta(j).as_deref(), Some("Hello"));
        assert_eq!(gemini_delta(r#"{"candidates":[{"finishReason":"STOP"}]}"#), None);
    }
    #[test]
    fn parses_ollama() {
        assert_eq!(
            ollama_delta(r#"{"message":{"role":"assistant","content":"yo"},"done":false}"#),
            (Some("yo".into()), false)
        );
        assert_eq!(ollama_delta(r#"{"message":{"content":""},"done":true}"#), (None, true));
    }
    #[test]
    fn errors() {
        assert_eq!(error_in(r#"{"error":{"message":"bad key"}}"#).as_deref(), Some("bad key"));
        assert!(http_err(429, String::new()).contains("Rate limit"));
        assert!(http_err(401, "{}".into()).contains("rejected"));
    }
    #[test]
    fn base_url() {
        assert_eq!(clean_base("http://localhost:11434/"), "http://localhost:11434");
        assert_eq!(clean_base("javascript:alert(1)"), "http://127.0.0.1:11434");
    }
}
