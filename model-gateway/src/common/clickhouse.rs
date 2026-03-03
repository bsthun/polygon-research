use crate::common::config::ClickHouseConfig;
use crate::util::sequence_id::SequenceId;
use clickhouse::Client;
use std::sync::Arc;

/// ClickHouse client wrapper
pub struct ClickHouseClient {
    pub(crate) client: Client,
    pub(crate) sequence_id: Arc<SequenceId>,
    pub(crate) database: String,
    pub(crate) url: String,
    pub(crate) username: String,
    pub(crate) password: String,
}

impl ClickHouseClient {
    /// Creates a new ClickHouse client from config
    pub fn new(config: &ClickHouseConfig) -> Self {
        let client = Client::default()
            .with_url(&config.url)
            .with_database(&config.database)
            .with_user(&config.username)
            .with_password(&config.password);

        let sequence_id = Arc::new(SequenceId::with_node_id(config.node_id));

        Self {
            client,
            sequence_id,
            database: config.database.clone(),
            url: config.url.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
        }
    }
}

/// Initialises and returns a ClickHouse client
pub fn init(config: &ClickHouseConfig) -> ClickHouseClient {
    ClickHouseClient::new(config)
}
