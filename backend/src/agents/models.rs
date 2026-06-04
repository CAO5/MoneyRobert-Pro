use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionMode {
    Paper,
    Demo,
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentDepartment {
    Technical,
    Capital,
    News,
    FundManager,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentSentiment {
    Bullish,
    Bearish,
    Neutral,
    Cautious,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DecisionAction {
    Long,
    Short,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DebateStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub name: String,
    pub department: AgentDepartment,
    pub role: String,
    pub reference_institution: String,
    pub credibility_score: f64,
    pub calibration_factor: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAnalysis {
    pub agent_name: String,
    pub department: AgentDepartment,
    pub sentiment: AgentSentiment,
    pub confidence: f64,
    pub content: String,
    pub analysis_data: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundManagerDecision {
    pub session_id: Uuid,
    pub action: DecisionAction,
    pub symbol: String,
    pub confidence: f64,
    pub position_size_percent: f64,
    pub leverage: i32,
    pub stop_loss_percent: Option<f64>,
    pub take_profit_percent: Option<f64>,
    pub reasoning: String,
    pub agent_contributions: Vec<AgentContribution>,
    pub risk_assessment: RiskAssessment,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContribution {
    pub agent_name: String,
    pub department: AgentDepartment,
    pub sentiment: AgentSentiment,
    pub confidence: f64,
    pub contribution_weight: f64,
    pub credibility_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub overall_risk_level: String,
    pub max_position_risk: f64,
    pub margin_requirement: f64,
    pub risk_reward_ratio: f64,
    pub volatility_rating: String,
    pub alerts: Vec<String>,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiSimulationConfig {
    pub id: Uuid,
    pub user_id: i64,
    pub symbol: String,
    pub mode: String,
    pub level: i32,
    pub status: String,
    
    pub initial_balance: f64,
    pub current_balance: f64,
    pub max_position_size_percent: f64,
    pub max_leverage: i32,
    pub max_daily_trades: i32,
    pub max_daily_loss_percent: f64,
    pub max_weekly_loss_percent: f64,
    pub max_single_trade_loss_percent: f64,
    pub ai_confidence_threshold: f64,
    pub analysis_interval_minutes: i32,
    pub allowed_symbols: Vec<String>,
    pub autonomous_mode_enabled: bool,
    pub requires_manual_confirm: bool,
    
    pub total_trades: i32,
    pub winning_trades: i32,
    pub losing_trades: i32,
    pub win_rate: f64,
    pub avg_pnl_percent: f64,
    pub profit_loss_ratio: f64,
    pub max_drawdown_percent: f64,
    pub sharpe_ratio: f64,
    
    pub weekly_pnl: f64,
    pub weekly_loss_percent: f64,
    pub daily_pnl: f64,
    pub daily_loss_percent: f64,
    pub consecutive_stop_losses: i32,
    
    pub running_days: i32,
    pub last_trade_at: Option<DateTime<Utc>>,
    pub promotion_eligible: bool,
    pub risk_confirmation_signed: bool,
    pub risk_confirmation_signed_at: Option<DateTime<Utc>>,
    pub max_acceptable_loss_amount: Option<f64>,
    
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiSimulationTrade {
    pub id: Uuid,
    pub config_id: Uuid,
    pub symbol: String,
    pub mode: String,
    
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub quantity: f64,
    pub leverage: i32,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    
    pub ai_confidence: Option<f64>,
    pub ai_reasoning: Option<serde_json::Value>,
    pub agent_session_id: Option<Uuid>,
    
    pub pnl: Option<f64>,
    pub pnl_percent: Option<f64>,
    pub fee_percent: f64,
    pub net_pnl_percent: Option<f64>,
    
    pub status: String,
    pub close_reason: Option<String>,
    pub holding_duration_minutes: Option<i32>,
    
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub symbol: String,
    pub current_price: f64,
    pub open_24h: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub close_24h: f64,
    pub volume_24h: f64,
    pub price_change_percent_24h: f64,
    pub funding_rate: Option<f64>,
    pub open_interest: Option<f64>,
    pub long_short_ratio: Option<f64>,
    pub rsi_14: Option<f64>,
    pub macd_signal: Option<f64>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateSession {
    pub id: Uuid,
    pub config_id: Option<Uuid>,
    pub user_id: Option<i64>,
    pub symbol: String,
    pub status: DebateStatus,
    pub messages: Vec<DebateMessage>,
    pub final_decision: Option<FundManagerDecision>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateMessage {
    pub id: Uuid,
    pub session_id: Uuid,
    pub agent_name: String,
    pub agent_department: AgentDepartment,
    pub role: String,
    pub content: String,
    pub analysis_data: serde_json::Value,
    pub confidence: f64,
    pub sentiment: Option<AgentSentiment>,
    pub message_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingStats {
    pub total_trades: i32,
    pub winning_trades: i32,
    pub losing_trades: i32,
    pub win_rate: f64,
    pub avg_pnl_percent: f64,
    pub profit_loss_ratio: f64,
    pub max_drawdown_percent: f64,
    pub running_days: i32,
    pub daily_loss_percent: f64,
    pub consecutive_days_without_risk_trigger: i32,
    pub weekly_loss_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionEligibility {
    pub eligible: bool,
    pub current_level: i32,
    pub next_level: Option<i32>,
    pub stats: RollingStats,
    pub requirements_met: bool,
    pub missing_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemotionTrigger {
    pub from_level: i32,
    pub to_level: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousConfig {
    pub enabled: bool,
    pub max_position_size_percent: f64,
    pub max_leverage: i32,
    pub max_daily_trades: i32,
    pub max_hourly_trades: i32,
    pub min_trade_interval_seconds: i64,
    pub max_daily_loss_percent: f64,
    pub max_weekly_loss_percent: f64,
    pub max_single_trade_loss_percent: f64,
    pub high_confidence_threshold: f64,
    pub allowed_symbols: Vec<String>,
    pub emergency_stop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousStatus {
    pub running: bool,
    pub paused: bool,
    pub last_trade_at: Option<DateTime<Utc>>,
    pub daily_trade_count: i32,
    pub hourly_trade_count: i32,
    pub daily_pnl: f64,
    pub weekly_pnl: f64,
    pub consecutive_stop_losses: i32,
    pub last_decision_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Critical,
    Emergency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::Paper
    }
}

impl Default for AutonomousConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_position_size_percent: 10.0,
            max_leverage: 5,
            max_daily_trades: 20,
            max_hourly_trades: 5,
            min_trade_interval_seconds: 300,
            max_daily_loss_percent: 3.0,
            max_weekly_loss_percent: 5.0,
            max_single_trade_loss_percent: 1.0,
            high_confidence_threshold: 0.8,
            allowed_symbols: vec!["DOGE-USDT-SWAP".to_string()],
            emergency_stop: false,
        }
    }
}

impl Default for AutonomousStatus {
    fn default() -> Self {
        Self {
            running: false,
            paused: false,
            last_trade_at: None,
            daily_trade_count: 0,
            hourly_trade_count: 0,
            daily_pnl: 0.0,
            weekly_pnl: 0.0,
            consecutive_stop_losses: 0,
            last_decision_summary: None,
        }
    }
}
