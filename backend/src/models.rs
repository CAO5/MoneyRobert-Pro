use sqlx::FromRow;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MarketData {
    pub id: Uuid,
    pub symbol: String,
    pub interval: String,
    pub open_time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Trade {
    pub id: Uuid,
    pub user_id: i64,
    pub symbol: String,
    pub side: String,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub size: f64,
    pub leverage: i32,
    pub status: String,
    pub pnl: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Position {
    pub id: Uuid,
    pub user_id: i64,
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub leverage: i32,
    pub liquidation_price: f64,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiAnalysis {
    pub id: Uuid,
    pub user_id: i64,
    pub symbol: String,
    pub analysis_type: String,
    pub content: serde_json::Value,
    pub confidence: f64,
    pub risk_level: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Report {
    pub id: Uuid,
    pub user_id: i64,
    pub title: String,
    pub content: String,
    pub format: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub user_id: i64,
    pub name: String,
    pub key: String,
    pub secret: String,
    pub passphrase: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: i64,
    pub title: String,
    pub content: String,
    pub notification_type: String,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SentimentData {
    pub id: Uuid,
    pub symbol: String,
    pub sentiment_score: f64,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct News {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub source: String,
    pub url: String,
    pub published_at: DateTime<Utc>,
    pub sentiment: Option<f64>,
    pub created_at: DateTime<Utc>,
}
