use thiserror::Error;
use serde::{Deserialize, Serialize};

pub type AgentResult<T> = Result<T, AgentError>;

#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum AgentError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Market data error: {0}")]
    MarketDataError(String),
    
    #[error("Debate error: {0}")]
    DebateError(String),
    
    #[error("Decision error: {0}")]
    DecisionError(String),
    
    #[error("Risk check failed: {0}")]
    RiskCheckFailed(String),
    
    #[error("Simulation error: {0}")]
    SimulationError(String),
    
    #[error("Promotion check error: {0}")]
    PromotionError(String),
    
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    #[error("Insufficient data: {0}")]
    InsufficientData(String),
    
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    #[error("External API error: {0}")]
    ExternalApiError(String),
    
    #[error("Execution error: {0}")]
    ExecutionError(String),
    
    #[error("Emergency stop triggered: {0}")]
    EmergencyStop(String),
    
    #[error("Level restriction: {0}")]
    LevelRestriction(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Analysis error: {0}")]
    AnalysisError(String),
}

impl From<sqlx::Error> for AgentError {
    fn from(err: sqlx::Error) -> Self {
        Self::DatabaseError(err.to_string())
    }
}

impl From<serde_json::Error> for AgentError {
    fn from(err: serde_json::Error) -> Self {
        Self::ConfigurationError(err.to_string())
    }
}

impl From<anyhow::Error> for AgentError {
    fn from(err: anyhow::Error) -> Self {
        Self::ConfigurationError(err.to_string())
    }
}
