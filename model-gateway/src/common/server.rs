use crate::handler::handler::{handle, State};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::net::TcpListener;

/// Server configuration
pub struct ServerConfig {
    pub listen: String,
}

impl ServerConfig {
    pub fn new(listen: String) -> Self {
        Self { listen }
    }

    /// Starts the HTTP server
    pub async fn run(self, state: State) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listen_addr = if self.listen.starts_with(':') {
            format!("0.0.0.0{}", self.listen)
        } else {
            self.listen.clone()
        };
        let addr: SocketAddr = SocketAddr::from_str(&listen_addr)?;
        let listener = TcpListener::bind(addr).await?;

        println!("Server listening on http://{}", addr);

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
}
