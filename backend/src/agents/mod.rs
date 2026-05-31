pub mod agents;
pub mod debate;
pub mod simulation;
pub mod autonomous;
pub mod promotion;
pub mod models;
pub mod errors;
pub mod config;
pub mod market;
pub mod risk;
pub mod notification;
pub mod llm_client;

pub use errors::{AgentError, AgentResult};
pub use models::*;
pub use config::AgentConfig;
pub use llm_client::LlmClient;
pub use debate::{
    Agent,
    AnalysisContext,
    DebateEngine,
    AgentRegistry,
    KlinePatternAnalyst,
    TechnicalIndicatorAnalyst,
    OnChainDataAnalyst,
    QuantModelAnalyst,
    FundingRateAnalyst,
    PositionStructureAnalyst,
    LongShortGameAnalyst,
    LiquidityAnalyst,
    SentimentAnalyst,
    MacroPolicyAnalyst,
    KOLWhaleMonitor,
    EventDrivenAnalyst,
};
