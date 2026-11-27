//! OpenAI-compatible streaming client, with the tool-call reassembly that
//! streaming makes necessary.
//!
//! Deliberately narrow: one endpoint shape, one transport. Providers that speak
//! `/v1/chat/completions` work without a plugin, and everything the model can
//! do lives in [`super::tools`], not in the transport.

use anyhow::{bail, Context, Result};

/// One complete line from a raw byte buffer, decoded only at a line boundary.
fn take_line(buf: &mut Vec<u8>) -> Option<String> {
    let nl = buf.iter().position(|b| *b == b'\n')?;
    let line = String::from_utf8_lossy(&buf[..nl])
        .trim_end_matches('\r')
        .to_string();
    buf.drain(..=nl);
    Some(line)
}
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

use super::config::Settings;
use super::{emit, StudioState};

pub struct Outcome {
    pub text: String,
    /// OpenAI-shaped tool_call objects, ready to append to the transcript.
    pub tool_calls: Vec<Value>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub finish: String,
}

pub async fn stream_chat(
    st: &StudioState,
    settings: &Settings,
    messages: Vec<Value>,
    tools: Value,
    thread_id: &str,
    strip_cjk: bool,
) -> Result<Outcome> {
    let body = json!({
        "model": settings.model,
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
        "stream": true,
        "temperature": 0.2,
    });

    let mut req = st
        .client
        .post(settings.endpoint())
        .timeout(Duration::from_secs(480))
        .header("content-type", "application/json")
        .json(&body);
    if let Some(key) = settings.key() {
        req = req.bearer_auth(key);
    }
    for (k, v) in &settings.extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }

    // One retry for a transient transport error. Safe: nothing is executed
    // until the model answers, so retrying the call cannot double a patch.
    // Keep one clone for the retry, send another (or the original).
    let retry_req = req.try_clone();
    let mut resp = match req.try_clone() {
        Some(r) => r.send().await.context("POST chat/completions")?,
        None => req.send().await.context("POST chat/completions")?,
    };
    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
        || resp.status().is_server_error()
    {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        if let Some(again) = retry_req {
            resp = again.send().await.context("POST chat/completions (retry)")?;
        }
    }
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!(
            "model endpoint answered {status}: {}",
            text.chars().take(600).collect::<String>()
        );
    }

    let mut text = String::new();
    let mut calls: BTreeMap<usize, Value> = BTreeMap::new();
    let mut usage = Value::Null;
    let mut finish = String::new();
    // Raw bytes, not a lossy String: a Thai character is three bytes and the
    // transport can split it across two chunks. Decoding per chunk turned the
    // halves into U+FFFD, which the patch guard then rejected — the model had
    // to regenerate a 30 KB page because one word broke.
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    let mut announced: std::collections::HashSet<usize> = Default::default();
    let mut last_kb: std::collections::HashMap<usize, usize> = Default::default();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("read model stream")?;
        buf.extend_from_slice(&bytes);
        // Only decode a complete line. A newline is one byte and never
        // appears inside a multi-byte character, so a line boundary is
        // always a character boundary.
        while let Some(line) = take_line(&mut buf) {
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                buf.clear();
                break;
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            if let Some(err) = v.get("error") {
                bail!("model error: {}", err);
            }
            if v.get("usage").map(|u| !u.is_null()).unwrap_or(false) {
                usage = v["usage"].clone();
            }
            let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
                continue;
            };
            if let Some(f) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                if !f.is_empty() && f != "null" {
                    finish = f.to_string();
                }
            }
            let Some(delta) = choice.get("delta") else {
                continue;
            };

            if let Some(content) = delta.get("content") {
                let piece = match content {
                    Value::String(s) => s.clone(),
                    Value::Array(parts) => parts
                        .iter()
                        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                        .collect::<String>(),
                    _ => String::new(),
                };
                // The operator did not write Chinese; an answer that contains it
                // is a bug even when it renders. Filter before the panel and the
                // transcript see it, so streaming and history agree.
                let piece = if strip_cjk {
                    super::lang::strip_cjk(&piece)
                } else {
                    piece
                };
                if !piece.is_empty() {
                    text.push_str(&piece);
                    emit(
                        &st.events,
                        thread_id,
                        json!({"type":"text_delta","text":piece}),
                    );
                }
            }

            if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tcs {
                    let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let entry = calls.entry(idx).or_insert_with(
                        || json!({"id":"","type":"function","function":{"name":"","arguments":""}}),
                    );
                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                        if !id.is_empty() {
                            entry["id"] = json!(id);
                        }
                    }
                    if let Some(f) = tc.get("function") {
                        if let Some(name) = f.get("name").and_then(|v| v.as_str()) {
                            if !name.is_empty() {
                                entry["function"]["name"] = json!(name);
                                // A whole-page patch is tens of kilobytes of
                                // JSON: the panel used to look frozen for a
                                // minute before the tool card appeared. Say
                                // what is being written, and how much.
                                if announced.insert(idx) {
                                    emit(
                                        &st.events,
                                        thread_id,
                                        json!({"type":"tool_preparing","name":name,"index":idx}),
                                    );
                                }
                            }
                        }
                        if let Some(args) = f.get("arguments").and_then(|v| v.as_str()) {
                            let mut acc = entry["function"]["arguments"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            acc.push_str(args);
                            let kb = acc.len() / 1024;
                            if last_kb.get(&idx).copied().unwrap_or(0) != kb {
                                last_kb.insert(idx, kb);
                                let name = entry["function"]["name"].as_str().unwrap_or("");
                                emit(
                                    &st.events,
                                    thread_id,
                                    json!({"type":"tool_progress","name":name,"index":idx,"chars":acc.len()}),
                                );
                            }
                            entry["function"]["arguments"] = json!(acc);
                        }
                    }
                }
            }
        }
    }

    let mut tool_calls: Vec<Value> = calls.into_values().collect();
    for (i, c) in tool_calls.iter_mut().enumerate() {
        if c.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            c["id"] = json!(format!("call_{i}"));
        }
    }

    let (tokens_in, tokens_out) = if usage.is_null() {
        // A provider that does not report usage still has to be auditable;
        // a 4-chars-per-token estimate is written down as an estimate.
        let all = messages_len(&messages);
        (all / 4, text.len() as u64 / 4)
    } else {
        (
            usage
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            usage
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        )
    };

    Ok(Outcome {
        text,
        tool_calls,
        tokens_in,
        tokens_out,
        finish,
    })
}

fn messages_len(messages: &[Value]) -> u64 {
    messages.iter().map(|m| m.to_string().len() as u64).sum()
}

#[cfg(test)]
mod tests {
    use super::take_line;

    #[test]
    fn a_character_split_across_chunks_survives() {
        // "ก" is E0 B8 81; the transport can end a chunk between any two bytes.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"data: {\"text\":\"\xe0");
        assert_eq!(take_line(&mut buf), None);
        buf.extend_from_slice(b"\xb8\x81\"}\n");
        let line = take_line(&mut buf).unwrap();
        assert!(line.contains('ก'), "line was {line:?}");
        assert!(!line.contains('\u{FFFD}'));
    }
}
