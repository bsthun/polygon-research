use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub listen: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    pub upstreams: Vec<Upstream>,
    pub postgres: Option<PostgresConfig>,
    pub clickhouse: Option<ClickHouseConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Upstream {
    pub name: String,
    #[serde(rename = "openaiEndpoint")]
    pub openai_endpoint: String,
    #[serde(rename = "anthropicEndpoint")]
    pub anthropic_endpoint: String,
    pub key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresConfig {
    pub dsn: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClickHouseConfig {
    pub url: String,
    pub database: String,
    pub username: String,
    pub password: String,
    #[serde(default = "default_node_id")]
    pub node_id: u8,
}

fn default_node_id() -> u8 {
    0
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

/// Loads config from environment or default path
pub fn init() -> Config {
    let path = std::env::var("GATEWAY_CONFIG_PATH")
        .unwrap_or_else(|_| ".local/config.yml".to_string());
    Config::load(&path).expect("failed to load config")
}
