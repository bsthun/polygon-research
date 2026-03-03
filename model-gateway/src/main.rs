mod common;
mod handler;
mod util;

use common::config::Config;
use handler::handler::{handle, State};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // * load config
    let config = Config::load(".local/config.yml").map_err(|e: Box<dyn std::error::Error>| {
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            as Box<dyn std::error::Error + Send + Sync>
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
    let addr: SocketAddr = SocketAddr::from_str(&listen_addr)?;
    let listener = TcpListener::bind(addr).await?;

    println!("Server listening on http://{}", addr);
    println!("Proxy endpoints:");
    println!("  - /api/v1/chat/completions");
    println!("  - /api/v1/responses");
    println!("  - /api/v1/messages");

    let state = State { config };

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| handle(req, state.clone()));

            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                eprintln!("Error serving connection from {}: {}", remote_addr, err);
            }
        });
    }
}
