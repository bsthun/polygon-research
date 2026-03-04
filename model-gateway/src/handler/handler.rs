use crate::common::clickhouse::ClickHouseClient;
use crate::common::config::Config;
use crate::database::clickhouse::query_log::{self, QueryLog};
use crate::util::parser::{extract_content, extract_model};
use crate::handler::validation::validate_api_key;
use sea_orm::DatabaseConnection;

use bytes::Bytes;
use http_body::Frame;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::body::Incoming;
use hyper::{Method, Request, Response, StatusCode};
use serde_json::Value;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::Mutex;

/// State shared across requests
#[derive(Clone)]
pub struct State {
    pub config: Config,
    #[allow(dead_code)]
    pub postgres: Option<DatabaseConnection>,
    pub clickhouse: Option<Arc<Mutex<ClickHouseClient>>>,
}

/// Creates a boxed HTTP body from a chunk of data.
pub fn box_body<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, Infallible> {
    Full::new(chunk.into()).boxed()
}

/// Creates an empty boxed HTTP body.
pub fn empty_body() -> BoxBody<Bytes, Infallible> {
    Empty::new().boxed()
}

/// Handles incoming HTTP requests and proxies them to the appropriate upstream.
pub async fn handle(
    req: Request<Incoming>,
    state: State,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // * prefix filtering
    if !path.starts_with("/api/v1/") {
        let mut res = Response::new(empty_body());
        *res.status_mut() = StatusCode::NOT_IMPLEMENTED;
        return Ok(res)
    }

    // * validate api key
    let authorization = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if let Err(status) = validate_api_key(&authorization, &state.config) {
        let mut res = Response::new(empty_body());
        *res.status_mut() = status;
        return Ok(res);
    }

    // * allowed GET paths
    let get_allowed_paths = ["/api/v1/models", "/v1/models"];
    let is_get_allowed = method == Method::GET && get_allowed_paths.iter().any(|p| path.contains(p));

    // * check method
    if method != Method::POST && !is_get_allowed {
        let mut res = Response::new(empty_body());
        *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
        return Ok(res);
    }

    // * handle get requests
    if is_get_allowed {
        let mut res = Response::new(empty_body());
        *res.status_mut() = StatusCode::OK;
        return Ok(res);
    }

    // * read request body
    let body_bytes = match req.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            let mut res = Response::new(empty_body());
            *res.status_mut() = StatusCode::BAD_REQUEST;
            return Ok(res);
        }
    };
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    // * extract model for logging
    let model = extract_model(&body_str).unwrap_or_default();
    let content = extract_content(&body_str).unwrap_or_default();

    // * strip prefix
    let path_v1 = path.strip_prefix("/api").unwrap_or(&path);

    // * determine upstream endpoint and build auth headers based on path
    let upstream = &state.config.upstreams[0];
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    let (target_base, path_suffix, forward_body) = if path_v1.starts_with("/v1/messages") {
        // * anthropic – strip unsupported fields
        const ANTHROPIC_DENYLIST: &[&str] = &["context_management"];
        let filtered = serde_json::from_slice::<Value>(&body_bytes)
            .map(|mut v| {
                if let Some(obj) = v.as_object_mut() {
                    for key in ANTHROPIC_DENYLIST {
                        obj.remove(*key);
                    }
                }
                serde_json::to_vec(&v).unwrap_or_else(|_| body_bytes.to_vec())
            })
            .unwrap_or_else(|_| body_bytes.to_vec());
        headers.insert("Authorization", format!("Bearer {}", upstream.key).parse().unwrap());
        headers.insert("x-api-key", upstream.key.parse().unwrap());
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
        (upstream.anthropic_endpoint.clone(), path_v1.to_string(), Bytes::from(filtered))
    } else if path_v1.starts_with("/v1/responses") {
        // * openai
        headers.insert("Authorization", format!("Bearer {}", upstream.key).parse().unwrap());
        (
            upstream.openai_endpoint.clone(),
            path_v1.strip_prefix("/v1").unwrap_or(path_v1).to_string(),
            body_bytes.clone(),
        )
    } else {
        // * openai
        headers.insert("Authorization", format!("Bearer {}", upstream.key).parse().unwrap());
        (
            upstream.openai_endpoint.clone(),
            path_v1.strip_prefix("/v1").unwrap_or(path_v1).to_string(),
            body_bytes.clone(),
        )
    };
    let new_uri = format!("{}{}", target_base, path_suffix);

    // * create http client
    let client = reqwest::Client::new();

    // * create proxy request using reqwest
    let proxy_req = client
        .post(&new_uri)
        .headers(headers)
        .body(forward_body.to_vec())
        .build()
        .unwrap();

    // * send request to upstream
    match client.execute(proxy_req).await {
        Ok(proxy_res) => {
            let status = StatusCode::from_u16(proxy_res.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

            // * build response
            let mut builder = Response::builder().status(status);

            // * copy headers
            for (key, value) in proxy_res.headers() {
                if let Ok(v) = value.to_str() {
                    builder = builder.header(key.as_str(), v);
                }
            }

            // * check if streaming response
            let content_type = proxy_res
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            if content_type.contains("text/event-stream") || content_type.contains("stream") {
                // * streaming response - stream to client while collecting for logging
                use futures_util::stream::StreamExt;

                // * track timing in microseconds for precision
                let start_time = std::time::Instant::now();
                let mut first_token_time: Option<u64> = None;
                let clickhouse = state.clickhouse.clone();

                // * create a channel to stream to client while collecting
                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::convert::Infallible>>(100);

                // * spawn task to collect stream and log to clickhouse
                let body_str_clone = body_str.to_string();
                let model_clone = model.clone();
                let content_clone = content.clone();
                let auth_clone = authorization.clone();

                tokio::spawn(async move {
                    let mut full_response = Vec::new();
                    let mut stream = proxy_res.bytes_stream();

                    while let Some(chunk) = stream.next().await {
                        if let Ok(bytes) = chunk {
                            // * check if this chunk contains content_block_start
                            let chunk_str = String::from_utf8_lossy(&bytes);
                            if first_token_time.is_none() && chunk_str.contains("content_block_start") {
                                first_token_time = Some(start_time.elapsed().as_millis() as u64);
                            }

                            // * send to client
                            let _ = tx.send(Ok(bytes.clone())).await;
                            full_response.extend_from_slice(&bytes);
                        }
                    }

                    let duration_completed = start_time.elapsed().as_millis() as u64;

                    // * log to clickhouse after stream completes
                    if let Some(clickhouse) = clickhouse {
                        let response_str = String::from_utf8_lossy(&full_response);
                        let (final_json, input_token, output_token, cache_token) =
                            parse_sse_events(&response_str);

                        let request_value: serde_json::Value = serde_json::from_str(&body_str_clone).unwrap_or(serde_json::Value::Object(Default::default()));
                        let response_value: serde_json::Value = serde_json::from_str(&final_json).unwrap_or(serde_json::Value::Object(Default::default()));

                        let query_log = QueryLog {
                            id: 0,
                            key_id: extract_key_id(&auth_clone),
                            model: model_clone,
                            content: content_clone,
                            request_payload: request_value,
                            response_payload: response_value,
                            duration_first_token: first_token_time.unwrap_or(0),
                            duration_completed,
                            input_token,
                            output_token,
                            cache_token,
                        };

                        let client = clickhouse.lock().await;
                        if let Err(e) = query_log::insert_log(&*client, &query_log).await {
                            log::error!("failed to insert clickhouse log: {}", e);
                        }
                    }
                });

                // * create stream from channel for response
                use http_body_util::BodyExt;
                let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
                    .map(|b| b.map(Frame::data));
                let body = BodyExt::boxed(StreamBody::new(stream));

                Ok(builder.body(body).unwrap())
            } else {
                // * non-streaming response
                let start_time = std::time::Instant::now();
                let response_body = proxy_res.bytes().await.unwrap_or_else(|_| Bytes::new());
                let duration_completed = start_time.elapsed().as_millis() as u64;

                // * log to clickhouse if configured
                if let Some(clickhouse) = &state.clickhouse {
                    let (input_token, output_token, cache_token) = extract_tokens(&response_body);
                    let key_id = extract_key_id(&authorization);

                    let request_value: serde_json::Value = serde_json::from_str(&body_str).unwrap_or(serde_json::Value::Object(Default::default()));
                    let response_value: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&response_body)).unwrap_or(serde_json::Value::Object(Default::default()));

                    let query_log = QueryLog {
                        id: 0,
                        key_id,
                        model,
                        content,
                        request_payload: request_value,
                        response_payload: response_value,
                        duration_first_token: 0,
                        duration_completed,
                        input_token,
                        output_token,
                        cache_token,
                    };

                    let clickhouse = clickhouse.clone();
                    let client = clickhouse.lock().await;
                    if let Err(e) = query_log::insert_log(&*client, &query_log).await {
                        log::error!("failed to insert clickhouse log: {}", e);
                        let mut res = Response::new(box_body(format!("clickhouse insert error: {}", e)));
                        *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                        return Ok(res);
                    }
                }

                Ok(builder.body(box_body(response_body)).unwrap())
            }
        }
        Err(e) => {
            let mut res = Response::new(box_body(format!("proxy error: {}", e)));
            *res.status_mut() = StatusCode::BAD_GATEWAY;
            Ok(res)
        }
    }
}

/// Extracts token usage from the response body
fn extract_tokens(response_body: &Bytes) -> (u64, u64, u64) {
    let body_str = String::from_utf8_lossy(response_body);
    let json: Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(_) => return (0, 0, 0),
    };

    let usage = json.get("usage");
    if usage.is_none() {
        return (0, 0, 0);
    }
    let usage = usage.unwrap();

    let input_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // * for anthropic, cache tokens are in cache_read_input_tokens
    let cache_tokens = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    (input_tokens, output_tokens, cache_tokens)
}

/// Extracts key ID from the authorization header
fn extract_key_id(authorization: &String) -> String {
    // * remove "Bearer " prefix if present
    authorization
        .trim_start_matches("Bearer ")
        .trim_start_matches("bearer ")
        .to_string()
}

/// Parses SSE events from streaming response and reconstructs JSON
fn parse_sse_events(response: &str) -> (String, u64, u64, u64) {
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut cache_tokens: u64 = 0;

    let mut message_id = String::new();
    let mut message_type = String::new();
    let mut message_role = String::new();
    let mut message_model = String::new();
    let mut message_stop_reason: Option<String> = None;
    let mut content_blocks: Vec<Value> = Vec::new();
    let mut current_block_text = String::new();
    let mut current_block_type = String::new();
    let mut current_block_thinking = String::new();
    let mut current_block_signature = String::new();

    for line in response.lines() {
        let line = line.trim();
        if !line.starts_with("data: ") {
            continue;
        }

        let data = line.strip_prefix("data: ").unwrap_or("");
        let event_json: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = event_json.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match event_type {
            "message_start" => {
                if let Some(msg) = event_json.get("message").and_then(|v| v.as_object()) {
                    message_id = msg.get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    message_type = msg.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    message_role = msg.get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    message_model = msg.get("model")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    // * extract initial usage
                    if let Some(usage) = msg.get("usage").and_then(|v| v.as_object()) {
                        input_tokens = usage.get("input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        cache_tokens = usage.get("cache_read_input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                    }
                }
            }
            "content_block_start" => {
                // * reset current block
                current_block_text = String::new();
                current_block_thinking = String::new();
                current_block_signature = String::new();

                if let Some(block) = event_json.get("content_block").and_then(|v| v.as_object()) {
                    current_block_type = block.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                }
            }
            "content_block_delta" => {
                if let Some(delta) = event_json.get("delta").and_then(|v| v.as_object()) {
                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                        current_block_text.push_str(text);
                    }
                    if let Some(thinking) = delta.get("thinking").and_then(|v| v.as_str()) {
                        current_block_thinking.push_str(thinking);
                    }
                    if let Some(sig) = delta.get("signature").and_then(|v| v.as_str()) {
                        current_block_signature.push_str(sig);
                    }
                }
            }
            "content_block_stop" => {
                // * build the content block
                let mut block_obj = serde_json::Map::new();
                block_obj.insert("type".to_string(), Value::String(current_block_type.clone()));

                if !current_block_thinking.is_empty() {
                    block_obj.insert("thinking".to_string(), Value::String(current_block_thinking.clone()));
                }
                if !current_block_signature.is_empty() {
                    block_obj.insert("signature".to_string(), Value::String(current_block_signature.clone()));
                }
                if !current_block_text.is_empty() {
                    block_obj.insert("text".to_string(), Value::String(current_block_text.clone()));
                }

                content_blocks.push(Value::Object(block_obj));

                // * reset
                current_block_text = String::new();
                current_block_thinking = String::new();
                current_block_signature = String::new();
            }
            "message_delta" => {
                // * extract final usage
                if let Some(usage) = event_json.get("usage").and_then(|v| v.as_object()) {
                    output_tokens = usage.get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if input_tokens == 0 {
                        input_tokens = usage.get("input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                    }
                    // * cache tokens can also be in message_delta
                    if cache_tokens == 0 {
                        cache_tokens = usage.get("cache_read_input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                    }
                }
                if let Some(delta) = event_json.get("delta").and_then(|v| v.as_object()) {
                    message_stop_reason = delta.get("stop_reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
            "message_stop" => {
                // * done
            }
            _ => {}
        }
    }

    // * build final JSON
    let mut response_obj = serde_json::Map::new();
    response_obj.insert("id".to_string(), Value::String(message_id));
    response_obj.insert("type".to_string(), Value::String(message_type));
    response_obj.insert("role".to_string(), Value::String(message_role));
    response_obj.insert("model".to_string(), Value::String(message_model));

    if !content_blocks.is_empty() {
        response_obj.insert("content".to_string(), Value::Array(content_blocks));
    }

    if let Some(reason) = message_stop_reason {
        response_obj.insert("stop_reason".to_string(), Value::String(reason));
    }

    // * build usage object
    let mut usage_obj = serde_json::Map::new();
    usage_obj.insert("input_tokens".to_string(), Value::Number(input_tokens.into()));
    usage_obj.insert("output_tokens".to_string(), Value::Number(output_tokens.into()));
    if cache_tokens > 0 {
        usage_obj.insert("cache_read_input_tokens".to_string(), Value::Number(cache_tokens.into()));
    }
    response_obj.insert("usage".to_string(), Value::Object(usage_obj));

    let final_json = serde_json::to_string(&Value::Object(response_obj)).unwrap_or_default();

    (final_json, input_tokens, output_tokens, cache_tokens)
}
