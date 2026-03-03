use crate::common::config::Config;
use crate::util::parser::{extract_content, extract_model};
use crate::handler::validation::validate_api_key;

use bytes::Bytes;
use http_body::Frame;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::body::Incoming;
use hyper::{Method, Request, Response, StatusCode};
use std::convert::Infallible;

/// State shared across requests
#[derive(Clone)]
pub struct State {
    pub config: Config,
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
        return Ok(Response::new(empty_body()));
    }

    // * validate api key
    let authorization = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Err(status) = validate_api_key(authorization, &state.config) {
        let mut res = Response::new(empty_body());
        *res.status_mut() = status;
        return Ok(res);
    }

    // * only allow POST method
    if method != Method::POST {
        let mut res = Response::new(empty_body());
        *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
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
    let body_str = String::from_utf8_lossy(&body_bytes);

    // * extract model for logging
    let _model = extract_model(&body_str);
    let _content = extract_content(&body_str);

    // * strip prefix
    let path_v1 = path.strip_prefix("/api").unwrap_or(&path);

    // * determine upstream endpoint based on path
    let upstream = &state.config.upstreams[0];
    let (target_base, auth_header, path_suffix) = if path_v1.starts_with("/v1/messages") {
        // * anthropic
        (
            upstream.anthropic_endpoint.clone(),
            format!("Bearer {}", upstream.key),
            path_v1.to_string(),
        )
    } else if path_v1.starts_with("/v1/responses") {
        // * openai
        (
            upstream.openai_endpoint.clone(),
            format!("Bearer {}", upstream.key),
            path_v1.strip_prefix("/v1").unwrap_or(path_v1).to_string(),
        )
    } else {
        // * openai
        (
            upstream.openai_endpoint.clone(),
            format!("Bearer {}", upstream.key),
            path_v1.strip_prefix("/v1").unwrap_or(path_v1).to_string(),
        )
    };
    let new_uri = format!("{}{}", target_base, path_suffix);

    // * create http client
    let client = reqwest::Client::new();

    // * create proxy request using reqwest
    let proxy_req = client
        .post(&new_uri)
        .header("Content-Type", "application/json")
        .header("Authorization", &auth_header)
        .body(body_bytes.to_vec())
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
                // * streaming response
                use futures_util::stream::StreamExt;
                let stream = proxy_res.bytes_stream().map(|chunk| {
                    chunk
                        .map(Frame::data)
                        .map_err(|_e| unreachable!("stream error should not happen"))
                });
                use http_body_util::BodyExt;
                let body = BodyExt::boxed(StreamBody::new(stream));
                Ok(builder.body(body).unwrap())
            } else {
                // * non-streaming response
                let body_bytes = proxy_res.bytes().await.unwrap_or_else(|_| Bytes::new());
                Ok(builder.body(box_body(body_bytes)).unwrap())
            }
        }
        Err(e) => {
            let mut res = Response::new(box_body(format!("proxy error: {}", e)));
            *res.status_mut() = StatusCode::BAD_GATEWAY;
            Ok(res)
        }
    }
}
