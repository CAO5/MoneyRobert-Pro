pub mod agents;
pub mod autonomous;
pub mod config;
pub mod debate;
pub mod decision_tuning;
pub mod errors;
pub mod evolution;
pub mod llm_client;
pub mod market;
pub mod memory;
pub mod models;
pub mod notification;
pub mod promotion;
pub mod risk;
pub mod simulation;

pub use config::AgentConfig;
pub use debate::{
    Agent, AgentRegistry, AnalysisContext, DebateEngine, EventDrivenAnalyst, FundingRateAnalyst,
    KOLWhaleMonitor, KlinePatternAnalyst, LiquidityAnalyst, LongShortGameAnalyst,
    MacroPolicyAnalyst, OnChainDataAnalyst, PositionStructureAnalyst, QuantModelAnalyst,
    SentimentAnalyst, TechnicalIndicatorAnalyst,
};
pub use decision_tuning::DecisionTuningConfig;
pub use errors::{AgentError, AgentResult};
pub use evolution::EvolutionEngine;
pub use llm_client::LlmClient;
pub use memory::MemoryManager;
pub use models::*;
