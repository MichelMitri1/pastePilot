//! Streaming OpenAI Chat Completions client.
//!
//! Speed choices: one shared HTTP/2 client so the TLS connection is reused,
//! a warm-up request at startup, keep-alive pings so the pooled connection
//! survives idle periods, and `stream: true` so tokens arrive as soon as the
//! model produces them.

use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

const API_BASE: &str = "https://api.openai.com/v1";
const MAX_OUTPUT_TOKENS: u32 = 1200;

pub struct OpenAi {
    client: reqwest::Client,
    /// Models that rejected `reasoning_effort`, so we stop sending it.
    no_reasoning_param: Mutex<HashSet<String>>,
}

impl OpenAi {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .pool_idle_timeout(Duration::from_secs(600))
            .http2_keep_alive_interval(Duration::from_secs(45))
            .http2_keep_alive_while_idle(true)
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .expect("http client");
        Self { client, no_reasoning_param: Mutex::new(HashSet::new()) }
    }

    /// Opens the TLS connection ahead of time (no key or data is sent).
    pub async fn warm_up(&self) {
        let _ = self.client.head(API_BASE).send().await;
    }

    /// Streams a reply and returns the full text once the stream ends.
    pub async fn generate(&self, api_key: &str, model: &str, messages: &[Value]) -> Result<String, String> {
        let skip_reasoning = self.no_reasoning_param.lock().map(|s| s.contains(model)).unwrap_or(false);
        let mut effort = if skip_reasoning { None } else { reasoning_effort(model) };

        loop {
            let mut body = json!({
                "model": model,
                "stream": true,
                "store": false,
                "max_completion_tokens": MAX_OUTPUT_TOKENS,
                "messages": messages
            });
            if let Some(e) = effort {
                body["reasoning_effort"] = json!(e);
            }

            let resp = self
                .client
                .post(format!("{API_BASE}/chat/completions"))
                .bearer_auth(api_key)
                .header("content-type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .map_err(|e| network_error(&e))?;

            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                let message = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                if status.as_u16() == 400 && effort.is_some() && message.contains("reasoning") {
                    if let Ok(mut set) = self.no_reasoning_param.lock() {
                        set.insert(model.to_owned());
                    }
                    effort = None;
                    continue;
                }
                return Err(http_error(status.as_u16(), &message));
            }
            return read_stream(resp).await;
        }
    }
}

/// Reasoning models are slow by default; ask for the least reasoning they allow.
fn reasoning_effort(model: &str) -> Option<&'static str> {
    let m = model.to_ascii_lowercase();
    if m.starts_with("gpt-5.") || m.starts_with("gpt-6") {
        Some("none")
    } else if m.starts_with("gpt-5") {
        Some("minimal")
    } else if m.len() > 1 && m.starts_with('o') && m.as_bytes()[1].is_ascii_digit() {
        Some("low")
    } else {
        None
    }
}

/// Parses server-sent events: `data: {json}` lines, terminated by `data: [DONE]`.
async fn read_stream(resp: reqwest::Response) -> Result<String, String> {
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut out = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| network_error(&e))?;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            match parse_sse_line(&line) {
                SseLine::Delta(text) => out.push_str(&text),
                SseLine::Done => return Ok(out),
                SseLine::Error(msg) => return Err(format!("OpenAI error: {msg}")),
                SseLine::Other => {}
            }
        }
    }
    Ok(out)
}

#[derive(Debug, PartialEq)]
enum SseLine {
    Delta(String),
    Done,
    Error(String),
    Other,
}

fn parse_sse_line(line: &[u8]) -> SseLine {
    let line = String::from_utf8_lossy(line);
    let Some(data) = line.trim().strip_prefix("data:") else { return SseLine::Other };
    let data = data.trim();
    if data == "[DONE]" {
        return SseLine::Done;
    }
    let Ok(v) = serde_json::from_str::<Value>(data) else { return SseLine::Other };
    if let Some(msg) = v["error"]["message"].as_str() {
        return SseLine::Error(msg.to_owned());
    }
    match v["choices"][0]["delta"]["content"].as_str() {
        Some(s) if !s.is_empty() => SseLine::Delta(s.to_owned()),
        _ => SseLine::Other,
    }
}

fn network_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "OpenAI request timed out.".into()
    } else if e.is_connect() {
        "Can't reach OpenAI. Check your connection.".into()
    } else {
        "Network error talking to OpenAI.".into()
    }
}

fn http_error(status: u16, message: &str) -> String {
    match status {
        401 => "Invalid OpenAI API key.".into(),
        403 => "OpenAI denied access to this model.".into(),
        404 => "Model not found. Check the model in Settings.".into(),
        429 => "OpenAI rate limit or quota reached.".into(),
        500..=599 => "OpenAI is having trouble. Try again.".into(),
        _ if !message.is_empty() => {
            let short: String = message.chars().take(120).collect();
            format!("OpenAI error: {short}")
        }
        _ => format!("OpenAI error ({status})."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sse_lines() {
        let delta = br#"data: {"choices":[{"delta":{"content":"Hi"}}]}"#;
        assert_eq!(parse_sse_line(delta), SseLine::Delta("Hi".into()));
        assert_eq!(parse_sse_line(b"data: [DONE]\n"), SseLine::Done);
        assert_eq!(parse_sse_line(b": keep-alive\n"), SseLine::Other);
        let role_only = br#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_sse_line(role_only), SseLine::Other);
        let err = br#"data: {"error":{"message":"boom"}}"#;
        assert_eq!(parse_sse_line(err), SseLine::Error("boom".into()));
    }

    #[test]
    fn picks_reasoning_effort() {
        assert_eq!(reasoning_effort("gpt-4.1-mini"), None);
        assert_eq!(reasoning_effort("gpt-5-mini"), Some("minimal"));
        assert_eq!(reasoning_effort("gpt-5.1"), Some("none"));
        assert_eq!(reasoning_effort("o4-mini"), Some("low"));
        assert_eq!(reasoning_effort("omni"), None);
    }
}
