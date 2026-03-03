mod config;

use config::Config;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use std::convert::Infallible;

#[derive(Clone)]
struct State {
    config: Config,
}

async fn handle(
    req: Request<Incoming>,
    state: State,
) -> Result<Response<Incoming>, Infallible> {
    let _method = req.method();
    let path = req.uri().path();

    // * prefix filtering
    if !path.starts_with("/api/v1/") {
        return Ok(Response::new(req.into_body()));
    }

    let path = path.to_string();

    // * route based on path
    let upstream = &state.config.upstreams[0];
    let target_base = if path.starts_with("/v1/messages") {
        &upstream.anthropic_endpoint
    } else {
        &upstream.openai_endpoint
    };

    // * build new uri using relative path
    let new_uri = format!("{}{}", target_base, path);
    println!("Proxying to: {}", new_uri);

    // * echo back for now (placeholder)
    let mut res = Response::new(req.into_body());
    *res.status_mut() = StatusCode::OK;
    Ok(res)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // * load config
    let config = Config::load(".local/config.yml").map_err(|e| {
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>
    })?;
    println!("Loaded {} upstreams from config", config.upstreams.len());
    println!("Listen on: {}", config.listen);

    // * use first upstream for now
    let upstream = &config.upstreams[0];
    println!("Using upstream: {}", upstream.name);
    println!("OpenAI Endpoint: {}", upstream.openai_endpoint);
    println!("Anthropic Endpoint: {}", upstream.anthropic_endpoint);

    let listen_addr = if config.listen.starts_with(':') {
        format!("0.0.0.0{}", config.listen)
    } else {
        config.listen.clone()
    };
    let addr: SocketAddr = listen_addr.parse()?;
    let listener = TcpListener::bind(addr).await?;

    println!("Server listening on http://{}", addr);
    println!("Proxy endpoints:");
    println!("  - /api/v1/chat/completions -> OpenAI");
    println!("  - /api/v1/responses -> OpenAI Response API");
    println!("  - /api/v1/messages -> Anthropic");

    let state = State { config };

    loop {
        let (stream, _remote_addr) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            let io = hyper_util::rt::TokioIo::new(stream);
            let service = service_fn(move |req| handle(req, state.clone()));

            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                eprintln!("Error serving connection: {}", err);
            }
        });
    }
}
