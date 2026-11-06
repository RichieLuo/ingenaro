use super::AiEvent;
use super::runtime::Job;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::BufReader;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError},
};
use std::time::Duration;

const MAX_TURNS: usize = 24;
const MAX_RESPONSE: usize = 16 * 1024 * 1024;
const MAX_ARGUMENTS: usize = 512 * 1024;

enum Chunk {
    Data(Value),
    End,
    Error(String),
}
#[derive(Default)]
struct PendingTool {
    id: String,
    name: String,
    arguments: String,
}

pub(super) fn run(job: &Job) -> Result<(), String> {
    // Local compatible servers may deliberately require no API key.
    let key = job.request.key.as_deref().unwrap_or("");
    if key.len() > 16384 || key.contains(['\r', '\n']) {
        return Err("Invalid API key".into());
    }
    let base = job
        .request
        .config
        .api_base
        .as_deref()
        .ok_or("API base URL is required")?;
    let base = base.trim_end_matches('/');
    let url = if base.ends_with("/chat/completions") {
        base.to_owned()
    } else {
        format!("{base}/chat/completions")
    };
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(
            job.request.config.timeout_seconds.min(120),
        ))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot initialize API client")?;
    job.bind(
        job.request
            .resume
            .as_ref()
            .map(|r| r.session_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
    );
    job.emit(AiEvent::State {
        state: "running".into(),
    });
    let mut messages = vec![json!({"role":"user","content":user_content(job)})];
    let tools = api_tools();
    for _ in 0..MAX_TURNS {
        job.check()?;
        let body = json!({"model":job.request.config.model,"messages":messages,"stream":true,"tools":tools,"tool_choice":"auto"});
        if serde_json::to_vec(&body).map_or(true, |v| v.len() > super::process::MAX_INPUT_FRAME) {
            return Err("API conversation exceeded the size limit".into());
        }
        let chunks = request_stream(
            client.clone(),
            url.clone(),
            key.to_owned(),
            body,
            job.request.cancelled.clone(),
        );
        let mut calls: BTreeMap<usize, PendingTool> = BTreeMap::new();
        let mut text = String::new();
        let mut finished = false;
        loop {
            job.check()?;
            let chunk = match chunks.recv_timeout(Duration::from_millis(50)) {
                Ok(c) => c,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err("API stream ended unexpectedly".into()),
            };
            match chunk {
                Chunk::Error(message) => return Err(message),
                Chunk::End => break,
                Chunk::Data(value) => {
                    if value.get("error").is_some() {
                        return Err(
                            "API provider reported an error; check model, key and quota".into()
                        );
                    }
                    let Some(choice) = value["choices"].as_array().and_then(|v| v.first()) else {
                        continue;
                    };
                    if let Some(reason) = choice["finish_reason"].as_str() {
                        if !matches!(reason, "stop" | "tool_calls" | "function_call") {
                            return Err("API stopped before completing the response (token limit or content policy)".into());
                        }
                        finished = true;
                    }
                    let delta = &choice["delta"];
                    if let Some(part) = delta["content"].as_str() {
                        if text.len() + part.len() > MAX_RESPONSE {
                            return Err("API response exceeded the size limit".into());
                        }
                        text.push_str(part);
                        job.emit(AiEvent::TextDelta { text: part.into() });
                    }
                    if let Some(deltas) = delta["tool_calls"].as_array() {
                        for delta in deltas {
                            let index = delta["index"]
                                .as_u64()
                                .ok_or("API tool delta had no index")?
                                as usize;
                            if index >= 32 {
                                return Err("API requested too many parallel tools".into());
                            }
                            let tool = calls.entry(index).or_default();
                            append_bounded(&mut tool.id, delta["id"].as_str(), 256)?;
                            append_bounded(
                                &mut tool.name,
                                delta["function"]["name"].as_str(),
                                256,
                            )?;
                            append_bounded(
                                &mut tool.arguments,
                                delta["function"]["arguments"].as_str(),
                                MAX_ARGUMENTS,
                            )?;
                        }
                    }
                }
            }
        }
        if !finished {
            return Err("API stream ended before a completion marker".into());
        }
        if calls.is_empty() {
            return Ok(());
        }
        let mut tool_calls = Vec::new();
        let mut parsed = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        for (_, tool) in calls {
            if tool.id.is_empty() || tool.name.is_empty() || !seen_ids.insert(tool.id.clone()) {
                return Err("API returned an invalid tool call identity".into());
            }
            let arguments: Value = serde_json::from_str(&tool.arguments)
                .map_err(|_| "API returned invalid tool arguments")?;
            if !arguments.is_object() {
                return Err("API tool arguments must be an object".into());
            }
            tool_calls.push(json!({"id":tool.id,"type":"function","function":{"name":tool.name,"arguments":tool.arguments}}));
            parsed.push((tool.id, tool.name, arguments));
        }
        messages.push(json!({"role":"assistant","content":text,"tool_calls":tool_calls}));
        for (id, name, arguments) in parsed {
            let (result, _) = job.call_tool(id.clone(), name, arguments)?;
            messages.push(json!({"role":"tool","tool_call_id":id,"content":result.to_string()}));
        }
    }
    Err("API tool loop reached its safety limit".into())
}
fn user_content(job: &Job) -> Value {
    if job.request.images.is_empty() {
        return json!(job.prompt());
    }
    let mut content: Vec<Value> = job
        .request
        .images
        .iter()
        .map(|image| json!({"type":"image_url","image_url":{"url":image.data_url()}}))
        .collect();
    content.push(json!({"type":"text","text":job.prompt()}));
    Value::Array(content)
}
fn api_tools() -> Value {
    Value::Array(crate::automation::tools().as_array().into_iter().flatten().map(|t| json!({"type":"function","function":{"name":t["name"],"description":t["description"],"parameters":t["inputSchema"]}})).collect())
}
fn append_bounded(dest: &mut String, part: Option<&str>, limit: usize) -> Result<(), String> {
    if let Some(part) = part {
        if dest.len() + part.len() > limit {
            return Err("API tool delta exceeded the size limit".into());
        }
        dest.push_str(part);
    }
    Ok(())
}
fn request_stream(
    client: reqwest::blocking::Client,
    url: String,
    key: String,
    body: Value,
    cancelled: Arc<AtomicBool>,
) -> Receiver<Chunk> {
    let (tx, rx) = mpsc::sync_channel(16);
    std::thread::spawn(move || {
        let result = (|| {
            let request = client.post(url);
            let request = if key.is_empty() {
                request
            } else {
                request.bearer_auth(key)
            };
            let response = request
                .header("Accept", "text/event-stream")
                .json(&body)
                .send()
                .map_err(|_| "API connection failed or timed out".to_owned())?;
            if !response.status().is_success() {
                return Err(format!(
                    "API request failed (HTTP {}); check key, model and quota",
                    response.status().as_u16()
                ));
            }
            let mut reader = BufReader::new(response);
            let mut total = 0;
            let mut data = String::new();
            loop {
                if cancelled.load(Ordering::Acquire) {
                    return Ok(());
                }
                let line = super::process::read_frame(&mut reader)?;
                let Some(line) = line else {
                    if !data.is_empty() {
                        dispatch_data(&data, &tx)?;
                    }
                    let _ = tx.send(Chunk::End);
                    return Ok(());
                };
                total += line.len();
                if total > MAX_RESPONSE {
                    return Err("API stream exceeded the size limit".into());
                }
                let line = std::str::from_utf8(&line)
                    .map_err(|_| "API stream was not UTF-8")?
                    .trim_end_matches(['\r', '\n']);
                if line.is_empty() {
                    if !data.is_empty() {
                        if dispatch_data(&data, &tx)? {
                            return Ok(());
                        }
                        data.clear();
                    }
                } else if let Some(part) = line.strip_prefix("data:") {
                    let part = part.strip_prefix(' ').unwrap_or(part);
                    if data.len() + part.len() + 1 > super::process::MAX_FRAME {
                        return Err("API SSE event exceeded the size limit".into());
                    }
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(part);
                }
            }
        })();
        if let Err(error) = result {
            let _ = tx.send(Chunk::Error(error));
        }
    });
    rx
}
fn dispatch_data(data: &str, tx: &mpsc::SyncSender<Chunk>) -> Result<bool, String> {
    if data.trim() == "[DONE]" {
        let _ = tx.send(Chunk::End);
        return Ok(true);
    }
    let value = serde_json::from_str(data).map_err(|_| "API stream contained invalid JSON")?;
    tx.send(Chunk::Data(value))
        .map_err(|_| "API consumer closed")?;
    Ok(false)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sse_does_not_treat_embedded_error_body_as_error_text() {
        let (tx, rx) = mpsc::sync_channel(2);
        dispatch_data("{\"error\":{\"message\":\"secret\"}}", &tx).unwrap();
        assert!(matches!(rx.recv().unwrap(), Chunk::Data(_)));
        assert!(
            dispatch_data("not-json secret", &tx)
                .unwrap_err()
                .contains("invalid JSON")
        );
    }
    #[test]
    fn fragmented_tool_arguments_are_bounded() {
        let mut text = "{\"x\":".to_owned();
        append_bounded(&mut text, Some("1}"), 20).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["x"], 1);
        assert!(append_bounded(&mut text, Some("too much"), 5).is_err());
    }
    #[test]
    fn mcp_tools_convert_to_openai_schema() {
        for tool in api_tools().as_array().unwrap() {
            assert_eq!(tool["type"], "function");
            assert!(tool["function"]["parameters"].is_object());
        }
    }
}
