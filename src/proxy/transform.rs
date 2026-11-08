use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================
// OpenAI Request/Response Types
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: OpenAiContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

// ============================================================
// Anthropic Request/Response Types
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: AnthropicContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    Text(String),
    Parts(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

// ============================================================
// Unified Chat Turn & Model Resolution
// ============================================================

#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

/// Map incoming model names to Google Antigravity backend model and thinking level
/// Returns: (backend_model_name, thinking_level_option)
pub fn resolve_model_and_thinking(model: &str) -> (&'static str, Option<&'static str>) {
    match model.to_lowercase().as_str() {
        // Gemini 3.8 Flash with thinking levels (Antigravity upstream: gemini-3.8-flash-tiered)
        "gemini-3.8-flash-low" => ("gemini-3.8-flash-tiered", Some("low")),
        "gemini-3.8-flash-medium" => ("gemini-3.8-flash-tiered", Some("medium")),
        "gemini-3.8-flash-high" => ("gemini-3.8-flash-tiered", Some("high")),
        "gemini-3.8-flash" | "gemini-3.8-flash-tiered" => ("gemini-3.8-flash-tiered", Some("high")),

        // Gemini 3 Flash series
        "gemini-3-flash" | "gemini-3-flash-high" => ("gemini-3-flash", Some("high")),
        "gemini-3-flash-medium" => ("gemini-3-flash", Some("medium")),
        "gemini-3-flash-low" => ("gemini-3-flash", Some("low")),
        "gemini-3.5-flash-lite" => ("gemini-3.5-flash-lite", None),
        "gemini-3.1-flash-lite" => ("gemini-3.1-flash-lite", None),

        // Gemini 2.5 series
        "gemini-2.5-flash" => ("gemini-2.5-flash", None),
        "gemini-2.5-pro" => ("gemini-2.5-pro", None),
        "gemini-2.5-flash-lite" => ("gemini-2.5-flash-lite", None),
        "gemini-2.5-flash-thinking" => ("gemini-2.5-flash-thinking", Some("high")),
        "gemini-2.0-flash" | "gemini-2.0-flash-exp" => ("gemini-2.5-flash", None),

        // Direct Claude & GPT models available in Antigravity
        "claude-sonnet-4-6" => ("claude-sonnet-4-6", None),
        "claude-opus-4-6-thinking" => ("claude-opus-4-6-thinking", None),
        "gpt-oss-120b-medium" => ("gpt-oss-120b-medium", None),

        // Mappings for OpenAI models (routed to ultra-fast Gemini 3.8 Flash Tiered)
        "gpt-4o" | "gpt-4" | "gpt-4-turbo" => ("gemini-3.8-flash-tiered", Some("high")),
        "gpt-4o-mini" | "gpt-3.5-turbo" => ("gemini-3.8-flash-tiered", Some("low")),

        // Mappings for Anthropic models (routed to Gemini 3.8 Flash Tiered for speed and unlimited quota)
        "claude-3-5-sonnet" | "claude-3-5-sonnet-20241022" | "claude-3-7-sonnet" => {
            ("gemini-3.8-flash-tiered", Some("high"))
        }
        "claude-3-5-haiku" | "claude-3-5-haiku-20241022" => ("gemini-3.8-flash-tiered", Some("low")),
        "claude-3-opus" | "claude-3-opus-20240229" => ("gemini-3.8-flash-tiered", Some("high")),

        // Fallback default
        _ => ("gemini-3.8-flash-tiered", Some("high")),
    }
}

/// Compact whitespace and newlines losslessly:
/// - Replaces 3 or more consecutive newlines (\n\n\n+) with 2 (\n\n)
/// - Trims trailing whitespace on each line without touching leading indentation
/// - Preserves code indentation, markdown, and all semantics completely
pub fn lossless_compact_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut empty_line_count = 0;

    for line in text.split('\n') {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            empty_line_count += 1;
            if empty_line_count <= 2 {
                result.push('\n');
            }
        } else {
            empty_line_count = 0;
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(trimmed_end);
        }
    }

    result
}

pub fn openai_to_turns(req: &OpenAiRequest) -> (Option<String>, Vec<ChatTurn>) {
    let mut system_prompt = req.system.as_deref().map(lossless_compact_text);
    let mut turns = Vec::new();

    for msg in &req.messages {
        match msg.role.as_str() {
            "system" => {
                let raw = match &msg.content {
                    OpenAiContent::Text(s) => s.clone(),
                    OpenAiContent::Parts(parts) => parts
                        .iter()
                        .filter_map(|p| p.text.clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                let text = lossless_compact_text(&raw);
                system_prompt = Some(match system_prompt {
                    Some(s) => {
                        if s.contains(&text) {
                            s // Deduplicate identical system prompt
                        } else {
                            format!("{}\n\n{}", s, text)
                        }
                    }
                    None => text,
                });
            }
            "user" | "assistant" => {
                let raw = match &msg.content {
                    OpenAiContent::Text(s) => s.clone(),
                    OpenAiContent::Parts(parts) => parts
                        .iter()
                        .filter_map(|p| p.text.clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                let text = lossless_compact_text(&raw);
                turns.push(ChatTurn {
                    role: msg.role.clone(),
                    content: text,
                });
            }
            _ => {}
        }
    }

    (system_prompt, turns)
}

pub fn anthropic_to_turns(req: &AnthropicRequest) -> (Option<String>, Vec<ChatTurn>) {
    let system_prompt = req.system.as_deref().map(lossless_compact_text);
    let mut turns = Vec::new();

    for msg in &req.messages {
        let raw = match &msg.content {
            AnthropicContent::Text(s) => s.clone(),
            AnthropicContent::Parts(parts) => parts
                .iter()
                .filter_map(|p| p.text.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        };
        let text = lossless_compact_text(&raw);
        turns.push(ChatTurn {
            role: msg.role.clone(),
            content: text,
        });
    }

    (system_prompt, turns)
}

/// Build the Google Antigravity / Code Assist payload
pub fn build_google_payload(
    project_id: &str,
    model: &str,
    system_prompt: Option<&str>,
    turns: &[ChatTurn],
    temperature: Option<f64>,
    max_tokens: Option<i64>,
) -> (String, Value) {
    let (backend_model, base_thinking_level) = resolve_model_and_thinking(model);

    // Smart Thinking Level Optimization:
    // If user explicitly chose a thinking level variant (e.g. -low, -medium, -high), preserve it.
    // If using generic model/alias (gemini-3.8-flash, gpt-4o, claude-3-5-sonnet-20241022):
    // Analyze prompt complexity to optimize thinking tokens without degrading output quality:
    let thinking_level = match base_thinking_level {
        Some("high") if !model.to_lowercase().ends_with("-high") => {
            let total_prompt_len: usize = turns.iter().map(|t| t.content.len()).sum();
            let has_code = turns.iter().any(|t| {
                t.content.contains("```")
                    || t.content.contains("fn ")
                    || t.content.contains("def ")
                    || t.content.contains("class ")
                    || t.content.contains("import ")
                    || t.content.contains("pub ")
            });

            if total_prompt_len < 160 && !has_code && turns.len() <= 2 {
                // Short simple query: LOW thinking saves ~70% thinking tokens with identical output quality
                Some("low")
            } else if total_prompt_len < 600 && !has_code {
                Some("medium")
            } else {
                Some("high")
            }
        }
        other => other,
    };

    let mut contents = Vec::new();

    for turn in turns {
        let google_role = match turn.role.as_str() {
            "assistant" | "model" => "model",
            _ => "user",
        };

        contents.push(json!({
            "role": google_role,
            "parts": [{ "text": turn.content }]
        }));
    }

    // If turns are empty, add dummy prompt
    if contents.is_empty() {
        contents.push(json!({
            "role": "user",
            "parts": [{ "text": "Hello" }]
        }));
    }

    let mut generation_config = json!({
        "temperature": temperature.unwrap_or(0.7),
        "maxOutputTokens": max_tokens.unwrap_or(8192),
    });

    if let Some(level) = thinking_level {
        let level_upper = match level.to_lowercase().as_str() {
            "high" => "HIGH",
            "medium" => "MEDIUM",
            "low" => "LOW",
            _ => "HIGH",
        };
        generation_config["thinkingConfig"] = json!({
            "thinkingLevel": level_upper
        });
    }

    let mut inner_request = json!({
        "contents": contents,
        "generationConfig": generation_config,
    });

    if let Some(sys) = system_prompt {
        inner_request["systemInstruction"] = json!({
            "parts": [{ "text": sys }]
        });
    }

    let payload = json!({
        "project": project_id,
        "model": backend_model,
        "request": inner_request,
    });

    (backend_model.to_string(), payload)
}

// ============================================================
// Google Response Extraction & Conversion
// ============================================================

pub fn extract_text_from_google_response(resp: &Value) -> (String, i64, i64) {
    let mut text_parts = Vec::new();

    let candidates = resp
        .get("candidates")
        .or_else(|| resp.get("response").and_then(|r| r.get("candidates")))
        .and_then(|c| c.as_array());

    if let Some(candidates) = candidates {
        if let Some(first) = candidates.first() {
            if let Some(parts) = first
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array())
            {
                for p in parts {
                    let is_thought = p.get("thought").and_then(|t| t.as_bool()).unwrap_or(false);
                    if !is_thought {
                        if let Some(txt) = p.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(txt.to_string());
                        }
                    }
                }
                // If thought parts only, fallback to all text
                if text_parts.is_empty() {
                    for p in parts {
                        if let Some(txt) = p.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(txt.to_string());
                        }
                    }
                }
            }
        }
    }

    let usage = resp
        .get("usageMetadata")
        .or_else(|| resp.get("response").and_then(|r| r.get("usageMetadata")));

    let input_tokens = usage
        .and_then(|u| u.get("promptTokenCount"))
        .and_then(|t| t.as_i64())
        .unwrap_or(0);

    let output_tokens = usage
        .and_then(|u| u.get("candidatesTokenCount"))
        .and_then(|t| t.as_i64())
        .unwrap_or(0);

    (text_parts.join(""), input_tokens, output_tokens)
}

pub fn google_to_openai_response(google_resp: &Value, model: &str) -> Value {
    let (text, in_tok, out_tok) = extract_text_from_google_response(google_resp);
    json!({
        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": text
            },
            "finish_reason": "stop",
            "logprobs": null
        }],
        "usage": {
            "prompt_tokens": in_tok,
            "completion_tokens": out_tok,
            "total_tokens": in_tok + out_tok
        }
    })
}

pub fn google_to_anthropic_response(google_resp: &Value, model: &str) -> Value {
    let (text, in_tok, out_tok) = extract_text_from_google_response(google_resp);
    json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{
            "type": "text",
            "text": text
        }],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": in_tok,
            "output_tokens": out_tok
        }
    })
}

// ============================================================
// SSE Streaming Conversion
// ============================================================

pub fn extract_delta_from_google_chunk(chunk_val: &Value) -> Option<String> {
    let candidates = chunk_val
        .get("candidates")
        .or_else(|| chunk_val.get("response").and_then(|r| r.get("candidates")))
        .and_then(|c| c.as_array())?;

    let first = candidates.first()?;
    let parts = first
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())?;

    let non_thoughts: Vec<&str> = parts
        .iter()
        .filter(|p| !p.get("thought").and_then(|t| t.as_bool()).unwrap_or(false))
        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
        .collect();

    if !non_thoughts.is_empty() {
        Some(non_thoughts.join(""))
    } else {
        parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .next()
            .map(|s| s.to_string())
    }
}

pub fn google_chunk_to_openai_sse(delta_text: &str, model: &str, msg_id: &str) -> String {
    let chunk = json!({
        "id": msg_id,
        "object": "chat.completion.chunk",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {
                "content": delta_text
            },
            "finish_reason": null
        }]
    });
    format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap())
}

pub fn google_chunk_to_anthropic_sse(delta_text: &str) -> String {
    let event = json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {
            "type": "text_delta",
            "text": delta_text
        }
    });
    format!(
        "event: content_block_delta\ndata: {}\n\n",
        serde_json::to_string(&event).unwrap()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lossless_compact_preserves_leading_indentation() {
        let python_code = "def calculate_total(items):\n    total = 0   \n    for item in items:\n        total += item.price\n\n\n\n    return total   ";
        let compacted = lossless_compact_text(python_code);

        // Leading 4 and 8 spaces must be strictly preserved
        assert!(compacted.contains("    total = 0"));
        assert!(compacted.contains("        total += item.price"));
        assert!(compacted.contains("    return total"));
        // 3+ consecutive newlines must be collapsed to 2
        assert!(!compacted.contains("\n\n\n"));
        assert!(compacted.contains("\n\n"));
        // Trailing whitespace must be stripped
        assert!(!compacted.contains("items):   "));
        assert!(!compacted.contains("total = 0   "));
    }

    #[test]
    fn test_resolve_model_and_thinking() {
        assert_eq!(resolve_model_and_thinking("gemini-3.8-flash-low"), ("gemini-3.8-flash-tiered", Some("low")));
        assert_eq!(resolve_model_and_thinking("gemini-3.8-flash-high"), ("gemini-3.8-flash-tiered", Some("high")));
        assert_eq!(resolve_model_and_thinking("gemini-2.5-flash"), ("gemini-2.5-flash", None));
        assert_eq!(resolve_model_and_thinking("gpt-4o"), ("gemini-3.8-flash-tiered", Some("high")));
        assert_eq!(resolve_model_and_thinking("claude-3-5-sonnet-20241022"), ("gemini-3.8-flash-tiered", Some("high")));
    }

    #[test]
    fn test_smart_thinking_budget_simple_vs_code() {
        // Simple short prompt -> should optimize to LOW thinking
        let simple_turns = vec![ChatTurn {
            role: "user".to_string(),
            content: "What is the capital of France?".to_string(),
        }];
        let (_, payload_simple) = build_google_payload("proj-1", "gemini-3.8-flash", None, &simple_turns, None, None);
        assert_eq!(
            payload_simple["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "LOW"
        );

        // Code prompt -> should use HIGH thinking
        let code_turns = vec![ChatTurn {
            role: "user".to_string(),
            content: "Write a Rust function:\n```rust\nfn parse_data() {}\n```".to_string(),
        }];
        let (_, payload_code) = build_google_payload("proj-1", "gemini-3.8-flash", None, &code_turns, None, None);
        assert_eq!(
            payload_code["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "HIGH"
        );
    }

    #[test]
    fn test_extract_text_from_google_response() {
        let resp = json!({
            "response": {
                "candidates": [{
                    "content": {
                        "parts": [
                            {"thought": true, "text": "Thinking steps..."},
                            {"text": "Hello world!"}
                        ]
                    }
                }],
                "usageMetadata": {
                    "promptTokenCount": 15,
                    "candidatesTokenCount": 5
                }
            }
        });

        let (text, in_tok, out_tok) = extract_text_from_google_response(&resp);
        assert_eq!(text, "Hello world!");
        assert_eq!(in_tok, 15);
        assert_eq!(out_tok, 5);
    }
}
