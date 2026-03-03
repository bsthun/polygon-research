/// Extracts the model name from a request body.
///
/// # Arguments
/// * `body` - The JSON request body as a string
///
/// # Returns
/// * `Some(String)` if model is found
/// * `None` if model is not found or parsing fails
///
/// # Examples
/// ```
/// let body = r#"{"model": "gpt-4o", "messages": []}"#;
/// let model = extract_model(body);
/// assert_eq!(model, Some("gpt-4o".to_string()));
/// ```
pub fn extract_model(body: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(model) = value.get("model").and_then(|m| m.as_str()) {
            return Some(model.to_string());
        }
    }
    None
}

/// Extracts the content from the last message in the request body.
///
/// Supports multiple formats:
/// - OpenAI chat: `messages[-1].content` (string)
/// - OpenAI chat with content blocks: `messages[-1].content[-1].text`
/// - Anthropic: `messages[-1].content[-1].text`
/// - OpenAI Responses API: `input[-1].content` or `input[-1].content[-1].text`
///
/// # Arguments
/// * `body` - The JSON request body as a string
///
/// # Returns
/// * `Some(String)` if content is found
/// * `None` if content is not found or parsing fails
pub fn extract_content(body: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        // * openai format: messages[-1].content
        if let Some(messages) = value.get("messages").and_then(|m| m.as_array()) {
            if let Some(last) = messages.last() {
                // * content as string
                if let Some(content) = last.get("content").and_then(|c| c.as_str()) {
                    return Some(content.to_string());
                }
                // * content as array
                if let Some(content_arr) = last.get("content").and_then(|c| c.as_array()) {
                    if let Some(last_block) = content_arr.last() {
                        if let Some(text) = last_block.get("text").and_then(|t| t.as_str()) {
                            return Some(text.to_string());
                        }
                    }
                }
            }
        }

        // * anthropic format: messages[-1].content[-1].text
        if let Some(messages) = value.get("messages").and_then(|m| m.as_array()) {
            if let Some(last) = messages.last() {
                if let Some(content_arr) = last.get("content").and_then(|c| c.as_array()) {
                    if let Some(last_block) = content_arr.last() {
                        if let Some(text) = last_block.get("text").and_then(|t| t.as_str()) {
                            return Some(text.to_string());
                        }
                    }
                }
            }
        }

        // * openai responses API format: input[-1].content[-1].text
        if let Some(input) = value.get("input").and_then(|i| i.as_array()) {
            if let Some(last) = input.last() {
                // * content as string
                if let Some(content) = last.get("content").and_then(|c| c.as_str()) {
                    return Some(content.to_string());
                }
                // * content as array
                if let Some(content_arr) = last.get("content").and_then(|c| c.as_array()) {
                    if let Some(last_block) = content_arr.last() {
                        if let Some(text) = last_block.get("text").and_then(|t| t.as_str()) {
                            return Some(text.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_model_from_chat() {
        let body = r#"{"model": "gpt-4o", "messages": []}"#;
        let model = extract_model(body);
        assert_eq!(model, Some("gpt-4o".to_string()));
    }

    #[test]
    fn test_extract_model_not_found() {
        let body = r#"{"messages": []}"#;
        let model = extract_model(body);
        assert_eq!(model, None);
    }

    #[test]
    fn test_extract_content_from_string() {
        let body = r#"{"model": "gpt-4o", "messages": [{"role": "user", "content": "Hello world"}]}"#;
        let content = extract_content(body);
        assert_eq!(content, Some("Hello world".to_string()));
    }

    #[test]
    fn test_extract_content_from_blocks() {
        let body = r#"{"model": "gpt-4o", "messages": [{"role": "user", "content": [{"type": "text", "text": "Hello world"}]}]}"#;
        let content = extract_content(body);
        assert_eq!(content, Some("Hello world".to_string()));
    }

    #[test]
    fn test_extract_content_from_anthropic() {
        let body = r#"{"model": "claude-3-5-sonnet", "messages": [{"role": "user", "content": [{"type": "text", "text": "Hello"}]}]}"#;
        let content = extract_content(body);
        assert_eq!(content, Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_content_from_responses_api() {
        let body = r#"{"model": "gpt-4o", "input": [{"role": "user", "content": "Hello"}]}"#;
        let content = extract_content(body);
        assert_eq!(content, Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_content_invalid_json() {
        let body = "invalid json";
        let content = extract_content(body);
        assert_eq!(content, None);
    }
}
