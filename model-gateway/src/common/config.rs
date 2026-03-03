use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub listen: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    pub upstreams: Vec<Upstream>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Upstream {
    pub name: String,
    #[serde(rename = "openaiEndpoint")]
    pub openai_endpoint: String,
    #[serde(rename = "anthropicEndpoint")]
    pub anthropic_endpoint: String,
    pub key: String,
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
