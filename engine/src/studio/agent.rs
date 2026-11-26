//! One agent loop, two callers.
//!
//! Sols and Bee each grew their own copy of the same mechanics — call the
//! model, dispatch tool calls, count rounds and calls, force a last-round
//! report, accumulate tokens. Fixes landed twice and the copies drifted: a bee
//! only gained its round ceiling and forced report after a measured failure.
//!
//! The loop lives here. A caller implements [`Host`]: it supplies the tool
//! schemas, dispatches a call, and may persist or emit from the hooks. The
//! dispatcher is the seam — site tools for Sols, a read-only leaf set for a
//! bee — so the loop itself never learns what a patch or a site is.

use anyhow::Result;
use serde_json::{json, Value};

use super::{config::Settings, llm, StudioState};

pub struct Limits {
    pub rounds: usize,
    pub calls: usize,
}

pub struct Outcome {
    pub text: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub rounds: usize,
    pub calls: usize,
    /// Milliseconds per model round, so the caller keeps its timing log.
    pub round_ms: Vec<u128>,
}

pub struct Config<'a> {
    pub st: &'a StudioState,
    pub settings: &'a Settings,
    pub thread_id: &'a str,
    pub strip_cjk: bool,
    pub limits: Limits,
}

/// What a caller must provide. Everything else has a no-op default.
pub trait Host {
    fn tools(&self) -> Value;

    async fn dispatch(&mut self, call_id: &str, name: &str, args: Value) -> Result<Value>;

    async fn on_assistant(&mut self, _text: &str, _calls: &[Value]) -> Result<()> {
        Ok(())
    }

    async fn on_tool(&mut self, _call_id: &str, _name: &str, _value: &Value) -> Result<()> {
        Ok(())
    }

    /// Called before each round; a caller's policy may push a system message
    /// into the transcript here (nudges, a restated request, a forced report).
    async fn before_round(&mut self, _round: usize, _messages: &mut Vec<Value>) -> Result<()> {
        Ok(())
    }

    /// The model answered without calling a tool. Return true to give it
    /// another round (the policy usually queued a nudge in `before_round`).
    fn wants_more(&mut self) -> bool {
        false
    }
}

pub async fn run<H: Host>(
    cfg: Config<'_>,
    mut messages: Vec<Value>,
    host: &mut H,
) -> Result<Outcome> {
    let tools = host.tools();
    let mut out = Outcome {
        text: String::new(),
        tokens_in: 0,
        tokens_out: 0,
        rounds: 0,
        calls: 0,
        round_ms: Vec::new(),
    };

    for round in 0..cfg.limits.rounds {
        out.rounds = round + 1;
        host.before_round(round, &mut messages).await?;

        let t0 = std::time::Instant::now();
        let mut got = llm::stream_chat(
            cfg.st,
            cfg.settings,
            messages.clone(),
            tools.clone(),
            cfg.thread_id,
            cfg.strip_cjk,
        )
        .await;
        // A long tool argument (a whole page, a catalog) can lose the stream
        // mid-flight. Nothing has been executed at this point, so one retry is
        // safe; without it the turn died and the plan stalled.
        if let Err(e) = &got {
            let msg = format!("{e:#}");
            let transient = [
                "error decoding response body",
                "operation timed out",
                "connection reset",
                "connection closed",
                "unexpected eof",
                "temporarily unavailable",
            ]
            .iter()
            .any(|k| msg.to_lowercase().contains(k));
            if transient {
                tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                got = llm::stream_chat(
                    cfg.st,
                    cfg.settings,
                    messages.clone(),
                    tools.clone(),
                    cfg.thread_id,
                    cfg.strip_cjk,
                )
                .await;
            }
        }
        let got = got?;
        out.round_ms.push(t0.elapsed().as_millis());
        out.tokens_in += got.tokens_in;
        out.tokens_out += got.tokens_out;

        let has_calls = !got.tool_calls.is_empty();
        messages.push(json!({
            "role": "assistant",
            "content": got.text,
            "tool_calls": if has_calls { json!(got.tool_calls) } else { Value::Null },
        }));
        host.on_assistant(&got.text, &got.tool_calls).await?;

        if !has_calls {
            out.text = got.text;
            if host.wants_more() {
                continue;
            }
            return Ok(out);
        }

        for call in &got.tool_calls {
            let name = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let raw = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let args: Value = serde_json::from_str(raw).unwrap_or_else(|_| json!({}));
            let call_id = call
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("call")
                .to_string();
            out.calls += 1;

            let result = if out.calls > cfg.limits.calls {
                json!({
                    "ok": false,
                    "error": "the tool budget for this worker is spent; report what you have"
                })
            } else {
                match host.dispatch(&call_id, &name, args).await {
                    Ok(v) => v,
                    Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
                }
            };
            messages.push(json!({
                "role": "tool", "tool_call_id": call_id, "name": name,
                "content": clip(&result.to_string(), 24_000),
            }));
            host.on_tool(&call_id, &name, &result).await?;
        }
    }

    out.text = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|v| v.as_str()) == Some("assistant"))
        .and_then(|m| m.get("content").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    Ok(out)
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect()
    }
}
