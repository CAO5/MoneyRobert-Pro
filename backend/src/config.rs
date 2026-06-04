use serde::{Deserialize, Deserializer, Serialize};

fn default_pool_size() -> u32 {
    3
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RedisConfig {
    pub url: String,
}

fn default_algorithm() -> String {
    "HS256".to_string()
}

fn default_access_token_expire() -> i64 {
    30
}

fn default_refresh_token_expire() -> i64 {
    7
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SecurityConfig {
    pub secret_key: String,
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
    #[serde(default = "default_access_token_expire")]
    pub access_token_expire_minutes: i64,
    #[serde(default = "default_refresh_token_expire")]
    pub refresh_token_expire_days: i64,
}

fn default_origins() -> Vec<String> {
    vec![
        "http://localhost:3000".to_string(),
        "http://localhost:5173".to_string(),
        "http://localhost:8080".to_string(),
    ]
}

fn default_true() -> bool {
    true
}

mod origins_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(origins: &Vec<String>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_some(origins)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
        let val: serde_json::Value = serde_json::Value::deserialize(d)?;
        match val {
            serde_json::Value::Array(arr) => {
                arr.into_iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
                    .pipe(Ok)
            }
            serde_json::Value::String(s) => {
                if s.starts_with('[') {
                    serde_json::from_str::<Vec<String>>(&s).unwrap_or_else(|_| vec![s])
                } else {
                    s.split(',')
                        .map(|item| item.trim().to_string())
                        .filter(|item| !item.is_empty())
                        .collect::<Vec<_>>()
                }
                .pipe(Ok)
            }
            _ => Ok(vec![]),
        }
    }

    trait Pipe<T> {
        fn pipe<F: FnOnce(T) -> U, U>(self, f: F) -> U;
    }

    impl<T> Pipe<T> for T {
        fn pipe<F: FnOnce(T) -> U, U>(self, f: F) -> U {
            f(self)
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct CorsConfig {
    #[serde(default = "default_origins", with = "origins_serde")]
    pub origins: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_credentials: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_rpm")]
    pub requests_per_minute: u32,
    #[serde(default = "default_rph")]
    pub requests_per_hour: u32,
}

fn default_rpm() -> u32 {
    100
}

fn default_rph() -> u32 {
    1000
}

fn default_okx_public_url() -> String {
    "wss://ws.okx.com:8443/ws/v5/public".to_string()
}

fn default_okx_private_url() -> String {
    "wss://ws.okx.com:8443/ws/v5/private".to_string()
}

fn default_okx_business_url() -> String {
    "wss://ws.okx.com:8443/ws/v5/business".to_string()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct WebSocketConfig {
    #[serde(default = "default_okx_public_url")]
    pub okx_public_url: String,
    #[serde(default = "default_okx_private_url")]
    pub okx_private_url: String,
    #[serde(default = "default_okx_business_url")]
    pub okx_business_url: String,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8001
}

fn default_env() -> String {
    "development".to_string()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub debug: bool,
    #[serde(default = "default_env")]
    pub environment: String,
}

fn default_okx_api_key() -> String {
    String::new()
}

fn default_okx_is_demo() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OkxConfig {
    #[serde(default = "default_okx_api_key")]
    pub api_key: String,
    #[serde(default = "default_okx_api_key")]
    pub secret_key: String,
    #[serde(default = "default_okx_api_key")]
    pub passphrase: String,
    #[serde(default = "default_okx_is_demo")]
    pub is_demo: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub security: SecurityConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub websocket: WebSocketConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub okx: OkxConfig,
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        dotenvy::dotenv().ok();

        let config = config::Config::builder()
            .add_source(
                config::Environment::with_prefix("APP")
                    .prefix_separator("_")
                    .separator("__"),
            )
            .build()?;

        config.try_deserialize()
    }

    pub fn is_production(&self) -> bool {
        self.server.environment == "production"
    }

    pub fn is_development(&self) -> bool {
        self.server.environment == "development"
    }
}
