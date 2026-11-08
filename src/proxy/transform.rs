use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Cache for Gemini 3+ thought signatures across multi-turn function calling
pub static THOUGHT_SIGNATURE_CACHE: LazyLock<DashMap<String, String>> =
    LazyLock::new(DashMap::new);

// ============================================================
// OpenAI Request/Response Types
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_call: Option<Value>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_call: Option<Value>,
}

// ============================================================
// Anthropic Request/Response Types
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
}

// ============================================================
// Unified Chat Turn & Model Resolution
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: String,
    pub parts: Vec<Value>,
}

impl ChatTurn {
    pub fn new_text(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            parts: vec![json!({ "text": text.into() })],
        }
    }

    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
    }
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
    if text.trim().is_empty() {
        return String::new();
    }

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

/// Extract clean text from an Option<Value> content field
pub fn extract_text_from_value(content: &Option<Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => {
            let mut parts = Vec::new();
            for item in arr {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    parts.push(text);
                } else if let Some(text) = item.as_str() {
                    parts.push(text);
                }
            }
            parts.join("\n")
        }
        Some(val) if !val.is_null() => val.to_string(),
        _ => String::new(),
    }
}

/// Sanitize JSON schema for Google Gemini function declarations
pub fn sanitize_gemini_schema(val: &Value) -> Value {
    match val {
        Value::Object(map) => {
            let mut cleaned = serde_json::Map::new();
            for (k, v) in map {
                // Strip fields that Gemini schema validation rejects
                if k == "$schema" || k == "additionalProperties" {
                    continue;
                }
                cleaned.insert(k.clone(), sanitize_gemini_schema(v));
            }
            Value::Object(cleaned)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sanitize_gemini_schema).collect()),
        other => other.clone(),
    }
}

/// Convert OpenAI or Anthropic tool declarations & choice to Google Antigravity format
pub fn convert_tools_to_google(
    tools: Option<&[Value]>,
    legacy_functions: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> (Option<Value>, Option<Value>) {
    let mut declarations = Vec::new();

    if let Some(tool_list) = tools {
        for t in tool_list {
            let func = t.get("function").unwrap_or(t);
            if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                let mut decl = json!({ "name": name });
                if let Some(desc) = func.get("description").and_then(|d| d.as_str()) {
                    decl["description"] = json!(desc);
                }
                // Handle both OpenAI 'parameters' and Anthropic 'input_schema'
                let params = func.get("parameters").or_else(|| func.get("input_schema"));
                if let Some(p) = params {
                    decl["parameters"] = sanitize_gemini_schema(p);
                }
                declarations.push(decl);
            }
        }
    }

    if let Some(func_list) = legacy_functions {
        for f in func_list {
            if let Some(name) = f.get("name").and_then(|n| n.as_str()) {
                let mut decl = json!({ "name": name });
                if let Some(desc) = f.get("description").and_then(|d| d.as_str()) {
                    decl["description"] = json!(desc);
                }
                if let Some(p) = f.get("parameters") {
                    decl["parameters"] = sanitize_gemini_schema(p);
                }
                declarations.push(decl);
            }
        }
    }

    let google_tools = if !declarations.is_empty() {
        Some(json!([{
            "functionDeclarations": declarations
        }]))
    } else {
        None
    };

    let google_tool_config = tool_choice.and_then(|tc| match tc {
        Value::String(s) => match s.to_lowercase().as_str() {
            "auto" => Some(json!({
                "functionCallingConfig": { "mode": "AUTO" }
            })),
            "none" => Some(json!({
                "functionCallingConfig": { "mode": "NONE" }
            })),
            "required" => Some(json!({
                "functionCallingConfig": { "mode": "ANY" }
            })),
            _ => None,
        },
        Value::Object(map) => {
            if let Some(func_type) = map.get("type").and_then(|t| t.as_str()) {
                match func_type {
                    "auto" => Some(json!({
                        "functionCallingConfig": { "mode": "AUTO" }
                    })),
                    "any" => Some(json!({
                        "functionCallingConfig": { "mode": "ANY" }
                    })),
                    "tool" => {
                        let name = map.get("name").and_then(|n| n.as_str());
                        if let Some(name) = name {
                            Some(json!({
                                "functionCallingConfig": {
                                    "mode": "ANY",
                                    "allowedFunctionNames": [name]
                                }
                            }))
                        } else {
                            Some(json!({ "functionCallingConfig": { "mode": "ANY" } }))
                        }
                    }
                    "function" => {
                        let name = map
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .or_else(|| map.get("name").and_then(|n| n.as_str()));
                        if let Some(name) = name {
                            Some(json!({
                                "functionCallingConfig": {
                                    "mode": "ANY",
                                    "allowedFunctionNames": [name]
                                }
                            }))
                        } else {
                            Some(json!({ "functionCallingConfig": { "mode": "ANY" } }))
                        }
                    }
                    _ => None,
                }
            } else if let Some(name) = map.get("name").and_then(|n| n.as_str()) {
                Some(json!({
                    "functionCallingConfig": {
                        "mode": "ANY",
                        "allowedFunctionNames": [name]
                    }
                }))
            } else {
                None
            }
        }
        _ => None,
    });

    (google_tools, google_tool_config)
}

/// Convert OpenAI messages and tool calls to Google Antigravity turns
pub fn openai_to_turns(req: &OpenAiRequest) -> (Option<String>, Vec<ChatTurn>) {
    let mut system_prompt = req.system.as_deref().map(lossless_compact_text);
    let mut raw_turns: Vec<ChatTurn> = Vec::new();
    let mut call_id_to_name: HashMap<String, String> = HashMap::new();

    for msg in &req.messages {
        match msg.role.as_str() {
            "system" => {
                let raw = extract_text_from_value(&msg.content);
                let text = lossless_compact_text(&raw);
                if !text.is_empty() {
                    system_prompt = Some(match system_prompt {
                        Some(s) => {
                            if s.contains(&text) {
                                s
                            } else {
                                format!("{}\n\n{}", s, text)
                            }
                        }
                        None => text,
                    });
                }
            }
            "user" => {
                let raw = extract_text_from_value(&msg.content);
                let text = lossless_compact_text(&raw);
                if !text.is_empty() {
                    raw_turns.push(ChatTurn::new_text("user", text));
                }
            }
            "assistant" => {
                let raw = extract_text_from_value(&msg.content);
                let text = lossless_compact_text(&raw);
                let mut parts = Vec::new();

                if !text.is_empty() {
                    parts.push(json!({ "text": text }));
                }

                if let Some(tool_calls) = &msg.tool_calls {
                    for call in tool_calls {
                        let call_id = call.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        if let Some(func) = call.get("function") {
                            let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            if !call_id.is_empty() && !name.is_empty() {
                                call_id_to_name.insert(call_id.to_string(), name.to_string());
                            }

                            let args_val = match func.get("arguments") {
                                Some(Value::String(s)) => {
                                    serde_json::from_str::<Value>(s).unwrap_or_else(|_| json!({}))
                                }
                                Some(val @ Value::Object(_)) => val.clone(),
                                _ => json!({}),
                            };

                            let mut part_obj = json!({
                                "functionCall": {
                                    "name": name,
                                    "args": args_val
                                }
                            });

                            let cached_sig = if !call_id.is_empty() {
                                THOUGHT_SIGNATURE_CACHE.get(call_id).map(|s| s.value().clone())
                            } else {
                                None
                            };

                            let thought_sig = call
                                .get("thoughtSignature")
                                .or_else(|| call.get("thought_signature"))
                                .or_else(|| func.get("thoughtSignature"))
                                .or_else(|| func.get("thought_signature"))
                                .and_then(|s| s.as_str())
                                .map(|s| s.to_string())
                                .or(cached_sig);

                            if let Some(sig) = thought_sig {
                                part_obj["thoughtSignature"] = json!(sig);
                            }

                            parts.push(part_obj);
                        }
                    }
                }

                if let Some(func) = &msg.function_call {
                    let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let args_val = match func.get("arguments") {
                        Some(Value::String(s)) => {
                            serde_json::from_str::<Value>(s).unwrap_or_else(|_| json!({}))
                        }
                        Some(val @ Value::Object(_)) => val.clone(),
                        _ => json!({}),
                    };
                    let mut part_obj = json!({
                        "functionCall": {
                            "name": name,
                            "args": args_val
                        }
                    });
                    if let Some(sig) = func
                        .get("thoughtSignature")
                        .or_else(|| func.get("thought_signature"))
                        .and_then(|s| s.as_str())
                    {
                        part_obj["thoughtSignature"] = json!(sig);
                    }
                    parts.push(part_obj);
                }

                if !parts.is_empty() {
                    raw_turns.push(ChatTurn {
                        role: "model".to_string(),
                        parts,
                    });
                }
            }
            "tool" | "function" => {
                let func_name = msg
                    .name
                    .as_deref()
                    .or_else(|| {
                        msg.tool_call_id
                            .as_deref()
                            .and_then(|id| call_id_to_name.get(id).map(|s| s.as_str()))
                    })
                    .unwrap_or("tool_result");

                let raw_content = extract_text_from_value(&msg.content);
                let response_obj = if let Ok(parsed) = serde_json::from_str::<Value>(&raw_content)
                {
                    if parsed.is_object() {
                        parsed
                    } else {
                        json!({ "result": parsed })
                    }
                } else {
                    json!({ "result": raw_content })
                };

                raw_turns.push(ChatTurn {
                    role: "user".to_string(),
                    parts: vec![json!({
                        "functionResponse": {
                            "name": func_name,
                            "response": response_obj
                        }
                    })],
                });
            }
            _ => {}
        }
    }

    // Merge consecutive turns with identical roles (Gemini requires alternating user/model turns)
    let turns = merge_consecutive_turns(raw_turns);
    (system_prompt, turns)
}

/// Convert Anthropic messages and tool calls to Google Antigravity turns
pub fn anthropic_to_turns(req: &AnthropicRequest) -> (Option<String>, Vec<ChatTurn>) {
    let system_prompt = match &req.system {
        Some(Value::String(s)) => {
            let compacted = lossless_compact_text(s);
            if compacted.is_empty() {
                None
            } else {
                Some(compacted)
            }
        }
        Some(Value::Array(arr)) => {
            let text = arr
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
            let compacted = lossless_compact_text(&text);
            if compacted.is_empty() {
                None
            } else {
                Some(compacted)
            }
        }
        _ => None,
    };

    let mut raw_turns: Vec<ChatTurn> = Vec::new();
    let mut call_id_to_name: HashMap<String, String> = HashMap::new();

    for msg in &req.messages {
        let is_assistant = msg.role == "assistant";
        let target_role = if is_assistant { "model" } else { "user" };
        let mut parts = Vec::new();

        match &msg.content {
            Value::String(s) => {
                let text = lossless_compact_text(s);
                if !text.is_empty() {
                    parts.push(json!({ "text": text }));
                }
            }
            Value::Array(blocks) => {
                for block in blocks {
                    let block_type = block
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("text");
                    match block_type {
                        "text" => {
                            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                                let text = lossless_compact_text(t);
                                if !text.is_empty() {
                                    parts.push(json!({ "text": text }));
                                }
                            }
                        }
                        "tool_use" => {
                            let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let input = block.get("input").cloned().unwrap_or(json!({}));
                            if !id.is_empty() && !name.is_empty() {
                                call_id_to_name.insert(id.to_string(), name.to_string());
                            }
                            let mut part_obj = json!({
                                "functionCall": {
                                    "name": name,
                                    "args": input
                                }
                            });
                            let cached_sig = if !id.is_empty() {
                                THOUGHT_SIGNATURE_CACHE.get(id).map(|s| s.value().clone())
                            } else {
                                None
                            };
                            let thought_sig = block
                                .get("thoughtSignature")
                                .or_else(|| block.get("thought_signature"))
                                .and_then(|s| s.as_str())
                                .map(|s| s.to_string())
                                .or(cached_sig);
                            if let Some(sig) = thought_sig {
                                part_obj["thoughtSignature"] = json!(sig);
                            }
                            parts.push(part_obj);
                        }
                        "tool_result" => {
                            let tool_use_id = block
                                .get("tool_use_id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("");
                            let func_name = call_id_to_name
                                .get(tool_use_id)
                                .map(|s| s.as_str())
                                .unwrap_or("tool_result");

                            let raw_content = match block.get("content") {
                                Some(Value::String(s)) => s.clone(),
                                Some(val) => val.to_string(),
                                None => String::new(),
                            };

                            let response_obj =
                                if let Ok(parsed) = serde_json::from_str::<Value>(&raw_content) {
                                    if parsed.is_object() {
                                        parsed
                                    } else {
                                        json!({ "result": parsed })
                                    }
                                } else {
                                    json!({ "result": raw_content })
                                };

                            parts.push(json!({
                                "functionResponse": {
                                    "name": func_name,
                                    "response": response_obj
                                }
                            }));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        if !parts.is_empty() {
            raw_turns.push(ChatTurn {
                role: target_role.to_string(),
                parts,
            });
        }
    }

    let turns = merge_consecutive_turns(raw_turns);
    (system_prompt, turns)
}

/// Merge consecutive turns with identical roles and ensure proper Gemini structure
fn merge_consecutive_turns(raw_turns: Vec<ChatTurn>) -> Vec<ChatTurn> {
    let mut merged: Vec<ChatTurn> = Vec::new();
    for turn in raw_turns {
        if turn.parts.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if last.role == turn.role {
                last.parts.extend(turn.parts);
                continue;
            }
        }
        merged.push(turn);
    }

    // Gemini requires the first turn to have role 'user'
    if let Some(first) = merged.first() {
        if first.role == "model" {
            merged.insert(0, ChatTurn::new_text("user", "Hello"));
        }
    }

    // If completely empty, insert a fallback prompt
    if merged.is_empty() {
        merged.push(ChatTurn::new_text("user", "Hello"));
    }

    merged
}

/// Build the Google Antigravity / Code Assist payload
pub fn build_google_payload(
    project_id: &str,
    model: &str,
    system_prompt: Option<&str>,
    turns: &[ChatTurn],
    temperature: Option<f64>,
    max_tokens: Option<i64>,
    google_tools: Option<Value>,
    google_tool_config: Option<Value>,
) -> (String, Value) {
    let (backend_model, base_thinking_level) = resolve_model_and_thinking(model);

    // Smart Thinking Level Optimization:
    // If user explicitly chose a thinking level variant (e.g. -low, -medium, -high), preserve it.
    // If using generic model/alias (gemini-3.8-flash, gpt-4o, claude-3-5-sonnet-20241022):
    // Analyze prompt complexity to optimize thinking tokens without degrading output quality:
    let thinking_level = match base_thinking_level {
        Some("high") if !model.to_lowercase().ends_with("-high") => {
            let total_prompt_len: usize = turns.iter().map(|t| t.text_content().len()).sum();
            let has_code = turns.iter().any(|t| {
                let content = t.text_content();
                content.contains("```")
                    || content.contains("fn ")
                    || content.contains("def ")
                    || content.contains("class ")
                    || content.contains("import ")
                    || content.contains("pub ")
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
            "parts": turn.parts
        }));
    }

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

    let has_tools = google_tools.is_some()
        || turns.iter().any(|t| {
            t.parts
                .iter()
                .any(|p| p.get("functionCall").is_some() || p.get("functionResponse").is_some())
        });

    if !has_tools {
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

    if let Some(tools) = google_tools {
        inner_request["tools"] = tools;
    }

    if let Some(tool_config) = google_tool_config {
        inner_request["toolConfig"] = tool_config;
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

#[derive(Debug, Clone, Default)]
pub struct ExtractedGoogleResponse {
    pub text: String,
    pub tool_calls: Vec<Value>,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub fn extract_google_response(resp: &Value) -> ExtractedGoogleResponse {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

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
                    if let Some(fc) = p.get("functionCall") {
                        if let Some(name) = fc.get("name").and_then(|n| n.as_str()) {
                            let args = fc.get("args").cloned().unwrap_or(json!({}));
                            let call_id = format!(
                                "call_{}",
                                &uuid::Uuid::new_v4().to_string().replace('-', "")[..24]
                            );
                            let args_str = serde_json::to_string(&args)
                                .unwrap_or_else(|_| "{}".to_string());

                            // Preserve Gemini 3+ thought signature for multi-turn tool continuity
                            let thought_sig = p
                                .get("thoughtSignature")
                                .or_else(|| p.get("thought_signature"))
                                .or_else(|| fc.get("thoughtSignature"))
                                .or_else(|| fc.get("thought_signature"))
                                .and_then(|s| s.as_str());

                            if let Some(sig) = thought_sig {
                                if THOUGHT_SIGNATURE_CACHE.len() > 10000 {
                                    THOUGHT_SIGNATURE_CACHE.clear();
                                }
                                THOUGHT_SIGNATURE_CACHE.insert(call_id.clone(), sig.to_string());
                            }

                            tool_calls.push(json!({
                                "id": call_id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": args_str
                                }
                            }));
                        }
                    }
                }
                // If thought parts only and no tool calls, fallback to all text
                if text_parts.is_empty() && tool_calls.is_empty() {
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

    ExtractedGoogleResponse {
        text: text_parts.join(""),
        tool_calls,
        input_tokens,
        output_tokens,
    }
}

pub fn extract_text_from_google_response(resp: &Value) -> (String, i64, i64) {
    let ext = extract_google_response(resp);
    (ext.text, ext.input_tokens, ext.output_tokens)
}

pub fn google_to_openai_response(google_resp: &Value, model: &str) -> Value {
    let extracted = extract_google_response(google_resp);
    let has_tools = !extracted.tool_calls.is_empty();

    let mut message = json!({
        "role": "assistant"
    });

    if has_tools {
        message["content"] = if extracted.text.is_empty() {
            Value::Null
        } else {
            Value::String(extracted.text)
        };
        message["tool_calls"] = json!(extracted.tool_calls);
        // Legacy function_call compatibility for frameworks like Hermes
        if let Some(first) = extracted.tool_calls.first() {
            if let Some(func) = first.get("function") {
                message["function_call"] = func.clone();
            }
        }
    } else {
        message["content"] = json!(extracted.text);
    }

    let finish_reason = if has_tools { "tool_calls" } else { "stop" };

    json!({
        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason,
            "logprobs": null
        }],
        "usage": {
            "prompt_tokens": extracted.input_tokens,
            "completion_tokens": extracted.output_tokens,
            "total_tokens": extracted.input_tokens + extracted.output_tokens
        }
    })
}

pub fn google_to_anthropic_response(google_resp: &Value, model: &str) -> Value {
    let extracted = extract_google_response(google_resp);
    let has_tools = !extracted.tool_calls.is_empty();

    let mut content_blocks = Vec::new();
    if !extracted.text.is_empty() {
        content_blocks.push(json!({
            "type": "text",
            "text": extracted.text
        }));
    }

    for tc in &extracted.tool_calls {
        let name = tc
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("");
        let args_str = tc
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(|a| a.as_str())
            .unwrap_or("{}");
        let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
        let call_id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
        content_blocks.push(json!({
            "type": "tool_use",
            "id": call_id,
            "name": name,
            "input": input
        }));
    }

    let stop_reason = if has_tools {
        "tool_use"
    } else {
        "end_turn"
    };

    json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content_blocks,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": extracted.input_tokens,
            "output_tokens": extracted.output_tokens
        }
    })
}

// ============================================================
// SSE Streaming Conversion
// ============================================================

#[derive(Debug, Clone, Default)]
pub struct GoogleChunkParts {
    pub text: Option<String>,
    pub tool_calls: Vec<Value>, // Elements are {"name": String, "args": Value}
}

pub fn extract_chunk_parts_from_google(chunk_val: &Value) -> GoogleChunkParts {
    let mut result = GoogleChunkParts::default();

    let candidates = chunk_val
        .get("candidates")
        .or_else(|| chunk_val.get("response").and_then(|r| r.get("candidates")))
        .and_then(|c| c.as_array());

    let candidates = match candidates {
        Some(c) => c,
        None => return result,
    };

    let first = match candidates.first() {
        Some(f) => f,
        None => return result,
    };

    let parts = match first
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
    {
        Some(p) => p,
        None => return result,
    };

    let mut text_parts = Vec::new();
    for p in parts {
        let is_thought = p.get("thought").and_then(|t| t.as_bool()).unwrap_or(false);
        if !is_thought {
            if let Some(txt) = p.get("text").and_then(|t| t.as_str()) {
                text_parts.push(txt);
            }
        }
        if let Some(fc) = p.get("functionCall") {
            if let Some(name) = fc.get("name").and_then(|n| n.as_str()) {
                let args = fc.get("args").cloned().unwrap_or(json!({}));
                let thought_sig = p
                    .get("thoughtSignature")
                    .or_else(|| p.get("thought_signature"))
                    .or_else(|| fc.get("thoughtSignature"))
                    .or_else(|| fc.get("thought_signature"))
                    .and_then(|s| s.as_str());

                let mut tc_val = json!({
                    "name": name,
                    "args": args
                });
                if let Some(sig) = thought_sig {
                    tc_val["thoughtSignature"] = json!(sig);
                }
                result.tool_calls.push(tc_val);
            }
        }
    }

    if !text_parts.is_empty() {
        result.text = Some(text_parts.join(""));
    }

    result
}

#[allow(dead_code)]
pub fn extract_delta_from_google_chunk(chunk_val: &Value) -> Option<String> {
    extract_chunk_parts_from_google(chunk_val).text
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

pub fn google_tool_chunk_to_openai_sse(
    tool_name: &str,
    args: &Value,
    tool_index: usize,
    call_id: &str,
    model: &str,
    msg_id: &str,
) -> String {
    let args_str = serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string());
    let chunk = json!({
        "id": msg_id,
        "object": "chat.completion.chunk",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {
                "role": "assistant",
                "tool_calls": [{
                    "index": tool_index,
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "arguments": args_str
                    }
                }]
            },
            "finish_reason": null
        }]
    });
    format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap())
}

pub fn openai_finish_sse(finish_reason: &str, model: &str, msg_id: &str) -> String {
    let chunk = json!({
        "id": msg_id,
        "object": "chat.completion.chunk",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": finish_reason
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

pub fn anthropic_tool_use_sse(
    block_index: usize,
    call_id: &str,
    tool_name: &str,
    args: &Value,
) -> Vec<String> {
    let start_event = json!({
        "type": "content_block_start",
        "index": block_index,
        "content_block": {
            "type": "tool_use",
            "id": call_id,
            "name": tool_name,
            "input": {}
        }
    });
    let delta_event = json!({
        "type": "content_block_delta",
        "index": block_index,
        "delta": {
            "type": "input_json_delta",
            "partial_json": serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string())
        }
    });
    let stop_event = json!({
        "type": "content_block_stop",
        "index": block_index
    });
    vec![
        format!(
            "event: content_block_start\ndata: {}\n\n",
            serde_json::to_string(&start_event).unwrap()
        ),
        format!(
            "event: content_block_delta\ndata: {}\n\n",
            serde_json::to_string(&delta_event).unwrap()
        ),
        format!(
            "event: content_block_stop\ndata: {}\n\n",
            serde_json::to_string(&stop_event).unwrap()
        ),
    ]
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
        assert_eq!(
            resolve_model_and_thinking("gemini-3.8-flash-low"),
            ("gemini-3.8-flash-tiered", Some("low"))
        );
        assert_eq!(
            resolve_model_and_thinking("gemini-3.8-flash-high"),
            ("gemini-3.8-flash-tiered", Some("high"))
        );
        assert_eq!(
            resolve_model_and_thinking("gemini-2.5-flash"),
            ("gemini-2.5-flash", None)
        );
        assert_eq!(
            resolve_model_and_thinking("gpt-4o"),
            ("gemini-3.8-flash-tiered", Some("high"))
        );
        assert_eq!(
            resolve_model_and_thinking("claude-3-5-sonnet-20241022"),
            ("gemini-3.8-flash-tiered", Some("high"))
        );
    }

    #[test]
    fn test_smart_thinking_budget_simple_vs_code() {
        let simple_turns = vec![ChatTurn::new_text("user", "What is the capital of France?")];
        let (_, payload_simple) = build_google_payload(
            "proj-1",
            "gemini-3.8-flash",
            None,
            &simple_turns,
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            payload_simple["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "LOW"
        );

        let code_turns = vec![ChatTurn::new_text(
            "user",
            "Write a Rust function:\n```rust\nfn parse_data() {}\n```",
        )];
        let (_, payload_code) = build_google_payload(
            "proj-1",
            "gemini-3.8-flash",
            None,
            &code_turns,
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            payload_code["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "HIGH"
        );
    }

    #[test]
    fn test_openai_tools_and_turns_conversion() {
        let req: OpenAiRequest = serde_json::from_value(json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": "What is the weather in Tokyo?"
                },
                {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_12345",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"Tokyo\"}"
                        }
                    }]
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_12345",
                    "content": "{\"temperature\": 24, \"condition\": \"sunny\"}"
                }
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get current weather in location",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": { "type": "string" }
                        },
                        "required": ["location"]
                    }
                }
            }],
            "tool_choice": "auto"
        }))
        .unwrap();

        let (tools, tool_config) = convert_tools_to_google(
            req.tools.as_deref(),
            req.functions.as_deref(),
            req.tool_choice.as_ref(),
        );
        assert!(tools.is_some());
        assert_eq!(
            tool_config.unwrap()["functionCallingConfig"]["mode"],
            "AUTO"
        );

        let (sys, turns) = openai_to_turns(&req);
        assert!(sys.is_none());
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, "user");
        assert_eq!(turns[1].role, "model");
        assert_eq!(
            turns[1].parts[0]["functionCall"]["name"],
            "get_weather"
        );
        assert_eq!(turns[2].role, "user");
        assert_eq!(
            turns[2].parts[0]["functionResponse"]["name"],
            "get_weather"
        );
    }

    #[test]
    fn test_extract_google_response_with_tool_call() {
        let resp = json!({
            "response": {
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [
                            {"thought": true, "text": "Let me call get_weather."},
                            {
                                "functionCall": {
                                    "name": "get_weather",
                                    "args": { "location": "Tokyo" }
                                }
                            }
                        ]
                    }
                }],
                "usageMetadata": {
                    "promptTokenCount": 20,
                    "candidatesTokenCount": 8
                }
            }
        });

        let extracted = extract_google_response(&resp);
        assert_eq!(extracted.tool_calls.len(), 1);
        assert_eq!(
            extracted.tool_calls[0]["function"]["name"],
            "get_weather"
        );
        assert!(extracted.tool_calls[0]["function"]["arguments"]
            .as_str()
            .unwrap()
            .contains("Tokyo"));

        let openai_val = google_to_openai_response(&resp, "gpt-4o");
        assert_eq!(openai_val["choices"][0]["finish_reason"], "tool_calls");
        assert!(openai_val["choices"][0]["message"]["tool_calls"].is_array());
        assert_eq!(
            openai_val["choices"][0]["message"]["function_call"]["name"],
            "get_weather"
        );

        let anthropic_val = google_to_anthropic_response(&resp, "claude-3-5-sonnet");
        assert_eq!(anthropic_val["stop_reason"], "tool_use");
        assert_eq!(
            anthropic_val["content"][0]["type"],
            "tool_use"
        );
        assert_eq!(
            anthropic_val["content"][0]["name"],
            "get_weather"
        );
    }

    #[test]
    fn test_anthropic_tools_conversion() {
        let req: AnthropicRequest = serde_json::from_value(json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 1024,
            "messages": [
                {
                    "role": "user",
                    "content": "Check weather"
                },
                {
                    "role": "assistant",
                    "content": [{
                        "type": "tool_use",
                        "id": "toolu_01",
                        "name": "check_weather",
                        "input": {"city": "Berlin"}
                    }]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "toolu_01",
                        "content": "18 degrees"
                    }]
                }
            ],
            "tools": [{
                "name": "check_weather",
                "description": "Check weather in city",
                "input_schema": {
                    "type": "object",
                    "properties": { "city": { "type": "string" } }
                }
            }]
        }))
        .unwrap();

        let (tools, _) = convert_tools_to_google(
            req.tools.as_deref(),
            None,
            req.tool_choice.as_ref(),
        );
        assert!(tools.is_some());

        let (_, turns) = anthropic_to_turns(&req);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, "user");
        assert_eq!(turns[1].role, "model");
        assert_eq!(turns[1].parts[0]["functionCall"]["name"], "check_weather");
        assert_eq!(turns[2].role, "user");
        assert_eq!(turns[2].parts[0]["functionResponse"]["name"], "check_weather");
    }

    #[test]
    fn test_parallel_tool_calls_merged_consecutive_user_turns() {
        let req: OpenAiRequest = serde_json::from_value(json!({
            "model": "gpt-4o",
            "messages": [
                { "role": "user", "content": "What is the weather in Paris and Tokyo?" },
                {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call_1",
                            "type": "function",
                            "function": { "name": "get_weather", "arguments": "{\"city\":\"Paris\"}" }
                        },
                        {
                            "id": "call_2",
                            "type": "function",
                            "function": { "name": "get_weather", "arguments": "{\"city\":\"Tokyo\"}" }
                        }
                    ]
                },
                { "role": "tool", "tool_call_id": "call_1", "content": "{\"temp\": 15}" },
                { "role": "tool", "tool_call_id": "call_2", "content": "{\"temp\": 24}" }
            ]
        })).unwrap();

        let (_, turns) = openai_to_turns(&req);
        // Turns must alternate user -> model -> user (consecutive tool responses merged into 1 user turn)
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, "user");
        assert_eq!(turns[1].role, "model");
        assert_eq!(turns[1].parts.len(), 2);
        assert_eq!(turns[2].role, "user");
        assert_eq!(turns[2].parts.len(), 2); // 2 functionResponses merged into single turn
        assert_eq!(turns[2].parts[0]["functionResponse"]["name"], "get_weather");
        assert_eq!(turns[2].parts[1]["functionResponse"]["name"], "get_weather");
    }

    #[test]
    fn test_extract_chunk_parts_from_google_streaming() {
        // Test text chunk
        let text_chunk = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{ "text": "Hello, world!" }]
                }
            }]
        });
        let parts = extract_chunk_parts_from_google(&text_chunk);
        assert_eq!(parts.text.as_deref(), Some("Hello, world!"));
        assert!(parts.tool_calls.is_empty());

        // Test tool call chunk
        let tool_chunk = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "search_database",
                            "args": { "query": "rust async" }
                        }
                    }]
                }
            }]
        });
        let parts = extract_chunk_parts_from_google(&tool_chunk);
        assert!(parts.text.is_none());
        assert_eq!(parts.tool_calls.len(), 1);
        assert_eq!(parts.tool_calls[0]["name"], "search_database");
        assert_eq!(parts.tool_calls[0]["args"]["query"], "rust async");
    }

    #[test]
    fn test_legacy_functions_and_schema_sanitization() {
        let req: OpenAiRequest = serde_json::from_value(json!({
            "model": "gpt-4o",
            "messages": [{ "role": "user", "content": "Execute calculation" }],
            "functions": [{
                "name": "calc",
                "description": "Calculate expression",
                "parameters": {
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "expr": { "type": "string" }
                    },
                    "required": ["expr"]
                }
            }],
            "function_call": "auto"
        })).unwrap();

        let (tools, tool_config) = convert_tools_to_google(
            req.tools.as_deref(),
            req.functions.as_deref(),
            req.function_call.as_ref(),
        );

        let tools = tools.unwrap();
        let decl = &tools[0]["functionDeclarations"][0];
        assert_eq!(decl["name"], "calc");
        // $schema and additionalProperties must be stripped for Gemini compatibility
        assert!(decl["parameters"].get("$schema").is_none());
        assert!(decl["parameters"].get("additionalProperties").is_none());
        assert_eq!(decl["parameters"]["properties"]["expr"]["type"], "string");

        assert_eq!(tool_config.unwrap()["functionCallingConfig"]["mode"], "AUTO");
    }
}
