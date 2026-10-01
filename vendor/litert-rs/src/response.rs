use serde_json::{json, Value};

/// Builds the user message JSON the LiteRT-LM conversation API expects:
/// `{"role":"user","content":[{"type":"text","text":"..."}]}`.
pub fn build_user_message(prompt: &str) -> String {
    json!({
        "role": "user",
        "content": [{"type": "text", "text": prompt}]
    })
    .to_string()
}

/// Extracts the assistant text from a LiteRT-LM JSON response chunk.
///
/// Shape: `{"role":"assistant","content":[{"type":"text","text":"hello"}]}`.
/// Thinking chunks (`{"channels":{"thought":"..."}}`) are rendered with the
/// same channel markers the Dart/JS paths use. Non-JSON input passes through
/// verbatim.
pub fn extract_text(json_str: &str) -> String {
    let value: Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return json_str.to_string(),
    };

    if let Some(thought) = value
        .get("channels")
        .and_then(|c| c.get("thought"))
        .and_then(|t| t.as_str())
    {
        if !thought.is_empty() {
            return format!("<|channel>thought\n{thought}<channel|>");
        }
    }

    let content = match value.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return json_str.to_string(),
    };

    let mut out = String::new();
    for item in content {
        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                out.push_str(text);
            }
        }
    }
    out
}
