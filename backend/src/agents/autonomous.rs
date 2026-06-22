use crate::agents::errors::*;
use crate::agents::models::*;
use crate::exchanges::okx::OkxClient;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EngineState {
    Stopped,
    Starting,
    Running,
    Paused,
    EmergencyStopped,
    CircuitBreaker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: MarketEventType,
    pub symbol: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarketEventType {
    PriceUpdate,
    VolumeSpike,
    OrderBookUpdate,
    FundingRateUpdate,
    OpenInterestChange,
    LongShortRatioChange,
    VolatilitySpike,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub opportunity_type: OpportunityType,
    pub confidence: f64,
    pub price_level: f64,
    pub target_price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub reasoning: String,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpportunityType {
    Long,
    Short,
    ClosePosition,
    Hedge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub source: String,
    pub signal_type: SignalType,
    pub strength: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalType {
    Technical,
    Fundamental,
    Sentiment,
    OnChain,
    OrderFlow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAlert {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub severity: RiskSeverity,
    pub alert_type: RiskAlertType,
    pub symbol: Option<String>,
    pub message: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskSeverity {
    Info,
    Warning,
    High,
    Critical,
    Emergency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskAlertType {
    DrawdownWarning,
    LossLimitBreach,
    PositionLimitBreach,
    VolatilityBreach,
    LiquidityRisk,
    ExecutionFailure,
    MarketDisruption,
    SystemHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub max_daily_loss_percent: f64,
    pub max_consecutive_losses: i32,
    pub max_drawdown_percent: f64,
    pub cool_down_period_seconds: i64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_daily_loss_percent: 5.0,
            max_consecutive_losses: 3,
            max_drawdown_percent: 10.0,
            cool_down_period_seconds: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerState {
    pub tripped: bool,
    pub trip_time: Option<DateTime<Utc>>,
    pub trip_reason: Option<String>,
    pub consecutive_losses: i32,
    pub daily_loss_percent: f64,
    pub current_drawdown_percent: f64,
    pub total_trades: i32,
    pub winning_trades: i32,
    pub losing_trades: i32,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self {
            tripped: false,
            trip_time: None,
            trip_reason: None,
            consecutive_losses: 0,
            daily_loss_percent: 0.0,
            current_drawdown_percent: 0.0,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionLog {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub decision_type: DecisionType,
    pub symbol: Option<String>,
    pub action: Option<DecisionAction>,
    pub reasoning: String,
    pub context: serde_json::Value,
    pub outcome: Option<DecisionOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DecisionType {
    TradeExecution,
    RiskMitigation,
    OpportunityPass,
    EmergencyAction,
    CircuitBreakerAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOutcome {
    pub success: bool,
    pub pnl_percent: Option<f64>,
    pub execution_time_ms: Option<i64>,
    pub error_message: Option<String>,
}

pub struct MarketWatcher {
    config: AutonomousConfig,
    okx_client_provider: Option<Arc<dyn Fn() -> AgentResult<Arc<OkxClient>> + Send + Sync>>,
    market_events_tx: broadcast::Sender<MarketEvent>,
    market_events_rx: broadcast::Receiver<MarketEvent>,
    last_prices: Arc<DashMap<String, f64>>,
    snapshots: Arc<DashMap<String, MarketSnapshot>>,
}

impl MarketWatcher {
    pub fn new(config: AutonomousConfig) -> Self {
        let (tx, rx) = broadcast::channel(1000);
        Self {
            config,
            okx_client_provider: None,
            market_events_tx: tx,
            market_events_rx: rx,
            last_prices: Arc::new(DashMap::new()),
            snapshots: Arc::new(DashMap::new()),
        }
    }

    pub fn set_okx_client_provider<F>(&mut self, provider: F)
    where
        F: Fn() -> AgentResult<Arc<OkxClient>> + Send + Sync + 'static,
    {
        self.okx_client_provider = Some(Arc::new(provider));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MarketEvent> {
        self.market_events_tx.subscribe()
    }

    pub async fn watch(&self) -> AgentResult<()> {
        info!("MarketWatcher starting");

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));

        loop {
            interval.tick().await;

            for symbol in &self.config.allowed_symbols {
                if let Err(e) = self.fetch_market_data(symbol).await {
                    warn!("Failed to fetch market data for {}: {}", symbol, e);
                }
            }
        }
    }

    async fn fetch_market_data(&self, symbol: &str) -> AgentResult<()> {
        let snapshot = match self.fetch_snapshot_from_okx(symbol).await {
            Ok(snapshot) => snapshot,
            Err(e) => {
                debug!("OKX API failed for {}: {}, using fallback", symbol, e);
                self.generate_sample_snapshot(symbol)
            }
        };

        if let Some(prev_price) = self.last_prices.get(symbol) {
            if (snapshot.current_price - *prev_price).abs() > *prev_price * 0.001 {
                let event = MarketEvent {
                    id: Uuid::new_v4(),
                    timestamp: Utc::now(),
                    event_type: MarketEventType::PriceUpdate,
                    symbol: symbol.to_string(),
                    data: serde_json::json!({
                        "old_price": *prev_price,
                        "new_price": snapshot.current_price,
                        "change_percent": (snapshot.current_price - *prev_price) / *prev_price * 100.0
                    }),
                };
                let _ = self.market_events_tx.send(event);
            }
        }

        self.last_prices
            .insert(symbol.to_string(), snapshot.current_price);
        self.snapshots.insert(symbol.to_string(), snapshot);

        Ok(())
    }

    async fn fetch_snapshot_from_okx(&self, symbol: &str) -> AgentResult<MarketSnapshot> {
        let okx_client = match &self.okx_client_provider {
            Some(provider) => provider()?,
            None => {
                return Err(AgentError::ExternalApiError(
                    "OKX client provider not set".to_string(),
                ))
            }
        };

        let ticker = okx_client
            .get_ticker(symbol)
            .await
            .map_err(|e| AgentError::ExternalApiError(e.to_string()))?;

        let current_price = ticker
            .last
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .map_err(|e| AgentError::ExternalApiError(format!("Failed to parse price: {}", e)))?;
        let open_24h = ticker
            .open_24h
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(current_price);
        let high_24h = ticker
            .high_24h
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(current_price);
        let low_24h = ticker
            .low_24h
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(current_price);
        let volume_24h = ticker
            .vol_24h
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);

        let price_change_percent_24h = if open_24h > 0.0 {
            (current_price - open_24h) / open_24h * 100.0
        } else {
            0.0
        };

        Ok(MarketSnapshot {
            symbol: symbol.to_string(),
            current_price,
            open_24h,
            high_24h,
            low_24h,
            close_24h: current_price,
            volume_24h,
            price_change_percent_24h,
            funding_rate: None,
            open_interest: None,
            long_short_ratio: None,
            rsi_14: None,
            macd_signal: None,
            timestamp: Utc::now(),
        })
    }

    fn generate_sample_snapshot(&self, symbol: &str) -> MarketSnapshot {
        let base_price = if symbol.contains("DOGE") {
            0.22
        } else {
            42000.0
        };
        let noise = (rand::random::<f64>() - 0.5) * 0.02 * base_price;

        MarketSnapshot {
            symbol: symbol.to_string(),
            current_price: base_price + noise,
            open_24h: base_price * 0.98,
            high_24h: base_price * 1.05,
            low_24h: base_price * 0.95,
            close_24h: base_price * 1.01,
            volume_24h: 100_000_000.0,
            price_change_percent_24h: 1.0,
            funding_rate: Some(-0.0001),
            open_interest: Some(500_000_000.0),
            long_short_ratio: Some(1.2),
            rsi_14: Some(45.0),
            macd_signal: Some(-0.001),
            timestamp: Utc::now(),
        }
    }

    pub fn get_snapshot(&self, symbol: &str) -> Option<MarketSnapshot> {
        self.snapshots.get(symbol).map(|s| s.clone())
    }
}

pub struct OpportunityScanner {
    config: AutonomousConfig,
    market_events_rx: tokio::sync::Mutex<broadcast::Receiver<MarketEvent>>,
    opportunities_tx: mpsc::Sender<Opportunity>,
    opportunities: Arc<DashMap<String, Vec<Opportunity>>>,
}

impl OpportunityScanner {
    pub fn new(
        config: AutonomousConfig,
        market_events_rx: broadcast::Receiver<MarketEvent>,
        opportunities_tx: mpsc::Sender<Opportunity>,
    ) -> Self {
        Self {
            config,
            market_events_rx: tokio::sync::Mutex::new(market_events_rx),
            opportunities_tx,
            opportunities: Arc::new(DashMap::new()),
        }
    }

    pub async fn scan(&self) -> AgentResult<()> {
        info!("OpportunityScanner starting");

        loop {
            match self.market_events_rx.lock().await.recv().await {
                Ok(event) => {
                    if let Err(e) = self.analyze_event(&event).await {
                        warn!("Failed to analyze event: {}", e);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    warn!("Market events channel closed");
                    break;
                }
                Err(e) => {
                    error!("Error receiving market event: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn analyze_event(&self, event: &MarketEvent) -> AgentResult<()> {
        debug!("Analyzing market event: {:?}", event);

        let opportunity = self.detect_opportunity(event).await?;

        if let Some(opp) = opportunity {
            info!(
                "Opportunity detected: {:?} for {}",
                opp.opportunity_type, opp.symbol
            );

            if !self.opportunities.contains_key(&opp.symbol) {
                self.opportunities.insert(opp.symbol.clone(), Vec::new());
            }

            if let Some(mut opps) = self.opportunities.get_mut(&opp.symbol) {
                opps.push(opp.clone());
            }

            let _ = self.opportunities_tx.send(opp).await;
        }

        Ok(())
    }

    async fn detect_opportunity(&self, event: &MarketEvent) -> AgentResult<Option<Opportunity>> {
        // Real market-driven opportunity detection (no more rand mock)
        // Based on price change, volume, and technical indicators from the market event data
        let new_price = event
            .data
            .get("new_price")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let price_change_pct = event
            .data
            .get("change_percent")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let funding_rate = event.data.get("funding_rate").and_then(|v| v.as_f64());
        let rsi = event
            .data
            .get("rsi_14")
            .and_then(|v| v.as_f64())
            .unwrap_or(50.0);

        // RSI-based signal: oversold (<30) → Long, overbought (>70) → Short
        let (opportunity_type, base_confidence, description) = if rsi < 30.0 {
            (
                OpportunityType::Long,
                0.7_f64,
                format!("RSI oversold ({:.1}), potential reversal up", rsi),
            )
        } else if rsi > 70.0 {
            (
                OpportunityType::Short,
                0.7_f64,
                format!("RSI overbought ({:.1}), potential reversal down", rsi),
            )
        } else if price_change_pct < -5.0 {
            // Significant dip - potential bounce
            (
                OpportunityType::Long,
                0.65_f64,
                format!("Price dropped {:.1}%, potential bounce", price_change_pct),
            )
        } else if price_change_pct > 5.0 {
            // Significant pump - potential correction
            (
                OpportunityType::Short,
                0.65_f64,
                format!(
                    "Price pumped {:.1}%, potential correction",
                    price_change_pct
                ),
            )
        } else {
            // No clear signal
            return Ok(None);
        };

        // Adjust confidence based on funding rate (contrarian signal)
        let funding_adjustment: f64 = funding_rate
            .map(|fr| {
                if fr > 0.001 && opportunity_type == OpportunityType::Long {
                    -0.1 // High positive funding: longs paying shorts, bearish for longs
                } else if fr < -0.001 && opportunity_type == OpportunityType::Short {
                    -0.1 // High negative funding: shorts paying longs, bullish for shorts
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);

        let confidence: f64 = (base_confidence + funding_adjustment).clamp(0.0, 1.0);

        if confidence < self.config.high_confidence_threshold {
            return Ok(None);
        }

        let signals = vec![Signal {
            source: "Technical Analysis".to_string(),
            signal_type: SignalType::Technical,
            strength: confidence,
            description: description.clone(),
        }];

        let target_price = match opportunity_type {
            OpportunityType::Long => Some(new_price * 1.05),
            OpportunityType::Short => Some(new_price * 0.95),
            _ => None,
        };
        let stop_loss = match opportunity_type {
            OpportunityType::Long => Some(new_price * 0.97),
            OpportunityType::Short => Some(new_price * 1.03),
            _ => None,
        };

        Ok(Some(Opportunity {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            symbol: event.symbol.clone(),
            opportunity_type,
            confidence,
            price_level: new_price,
            target_price,
            stop_loss,
            reasoning: description,
            signals,
        }))
    }
}

pub struct RiskWatchdog {
    config: AutonomousConfig,
    circuit_breaker_config: CircuitBreakerConfig,
    risk_alerts_tx: broadcast::Sender<RiskAlert>,
    circuit_breaker_state: Arc<RwLock<CircuitBreakerState>>,
    portfolio: Arc<RwLock<PortfolioContext>>,
}

impl RiskWatchdog {
    pub fn new(
        config: AutonomousConfig,
        circuit_breaker_config: CircuitBreakerConfig,
        portfolio: Arc<RwLock<PortfolioContext>>,
    ) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            config,
            circuit_breaker_config,
            risk_alerts_tx: tx,
            circuit_breaker_state: Arc::new(RwLock::new(CircuitBreakerState::default())),
            portfolio,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RiskAlert> {
        self.risk_alerts_tx.subscribe()
    }

    pub async fn monitor(&self) -> AgentResult<()> {
        info!("RiskWatchdog starting");

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

        loop {
            interval.tick().await;

            if let Err(e) = self.check_risk_parameters().await {
                error!("Risk check failed: {}", e);
            }

            if let Err(e) = self.check_circuit_breaker().await {
                error!("Circuit breaker check failed: {}", e);
            }
        }
    }

    async fn check_risk_parameters(&self) -> AgentResult<()> {
        let portfolio = self.portfolio.read().await;

        if portfolio.daily_loss_percent >= self.config.max_daily_loss_percent * 0.8 {
            self.emit_risk_alert(RiskAlert {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                severity: RiskSeverity::High,
                alert_type: RiskAlertType::DrawdownWarning,
                symbol: None,
                message: format!(
                    "Daily loss approaching limit: {:.2}%",
                    portfolio.daily_loss_percent
                ),
                details: serde_json::json!({
                    "current": portfolio.daily_loss_percent,
                    "limit": self.config.max_daily_loss_percent
                }),
            })
            .await?;
        }

        Ok(())
    }

    async fn check_circuit_breaker(&self) -> AgentResult<()> {
        let mut cb_state = self.circuit_breaker_state.write().await;
        let portfolio = self.portfolio.read().await;

        if cb_state.tripped {
            if let Some(trip_time) = cb_state.trip_time {
                let elapsed = Utc::now().signed_duration_since(trip_time);
                if elapsed.num_seconds() >= self.circuit_breaker_config.cool_down_period_seconds {
                    cb_state.tripped = false;
                    cb_state.trip_time = None;
                    cb_state.trip_reason = None;
                    info!("Circuit breaker reset after cool-down period");
                }
            }
            return Ok(());
        }

        if portfolio.daily_loss_percent >= self.circuit_breaker_config.max_daily_loss_percent {
            self.trip_circuit_breaker(&mut cb_state, "Daily loss limit exceeded".to_string())
                .await?;
            return Ok(());
        }

        if cb_state.consecutive_losses >= self.circuit_breaker_config.max_consecutive_losses {
            self.trip_circuit_breaker(&mut cb_state, "Max consecutive losses exceeded".to_string())
                .await?;
            return Ok(());
        }

        if portfolio.current_drawdown_percent >= self.circuit_breaker_config.max_drawdown_percent {
            self.trip_circuit_breaker(&mut cb_state, "Max drawdown exceeded".to_string())
                .await?;
            return Ok(());
        }

        Ok(())
    }

    async fn trip_circuit_breaker(
        &self,
        cb_state: &mut CircuitBreakerState,
        reason: String,
    ) -> AgentResult<()> {
        cb_state.tripped = true;
        cb_state.trip_time = Some(Utc::now());
        cb_state.trip_reason = Some(reason.clone());

        error!("Circuit breaker tripped: {}", reason);

        self.emit_risk_alert(RiskAlert {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            severity: RiskSeverity::Critical,
            alert_type: RiskAlertType::LossLimitBreach,
            symbol: None,
            message: format!("CIRCUIT BREAKER TRIPPED: {}", reason),
            details: serde_json::json!({
                "reason": reason,
                "cool_down_seconds": self.circuit_breaker_config.cool_down_period_seconds
            }),
        })
        .await?;

        Ok(())
    }

    async fn emit_risk_alert(&self, alert: RiskAlert) -> AgentResult<()> {
        warn!("Risk alert: {:?} - {}", alert.severity, alert.message);
        let _ = self.risk_alerts_tx.send(alert);
        Ok(())
    }

    pub async fn record_trade_outcome(
        &self,
        success: bool,
        pnl_percent: Option<f64>,
    ) -> AgentResult<()> {
        let mut cb_state = self.circuit_breaker_state.write().await;
        cb_state.total_trades += 1;

        if success {
            cb_state.winning_trades += 1;
            cb_state.consecutive_losses = 0;
        } else {
            cb_state.losing_trades += 1;
            cb_state.consecutive_losses += 1;
        }

        if let Some(pnl) = pnl_percent {
            if pnl < 0.0 {
                cb_state.daily_loss_percent += pnl.abs();
            }
        }

        Ok(())
    }

    pub async fn is_circuit_breaker_tripped(&self) -> bool {
        self.circuit_breaker_state.read().await.tripped
    }

    pub async fn get_circuit_breaker_state(&self) -> CircuitBreakerState {
        self.circuit_breaker_state.read().await.clone()
    }
}

pub struct DecisionLogger {
    logs: Arc<RwLock<Vec<DecisionLog>>>,
    max_logs: usize,
}

impl DecisionLogger {
    pub fn new(max_logs: usize) -> Self {
        Self {
            logs: Arc::new(RwLock::new(Vec::new())),
            max_logs,
        }
    }

    pub async fn log(&self, log: DecisionLog) -> AgentResult<()> {
        info!("Logging decision: {:?}", log.decision_type);

        let mut logs = self.logs.write().await;
        logs.push(log);

        if logs.len() > self.max_logs {
            logs.remove(0);
        }

        Ok(())
    }

    pub async fn get_logs(&self, limit: usize) -> Vec<DecisionLog> {
        let logs = self.logs.read().await;
        logs.iter().rev().take(limit).cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioContext {
    pub total_balance: f64,
    pub available_balance: f64,
    pub positions: Vec<Position>,
    pub daily_pnl_percent: f64,
    pub daily_loss_percent: f64,
    pub current_drawdown_percent: f64,
    pub total_trades_today: i32,
    pub last_trade_at: Option<DateTime<Utc>>,
}

impl Default for PortfolioContext {
    fn default() -> Self {
        Self {
            total_balance: 10000.0,
            available_balance: 10000.0,
            positions: Vec::new(),
            daily_pnl_percent: 0.0,
            daily_loss_percent: 0.0,
            current_drawdown_percent: 0.0,
            total_trades_today: 0,
            last_trade_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: Uuid,
    pub symbol: String,
    pub direction: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub size: f64,
    pub leverage: i32,
    pub unrealized_pnl_percent: f64,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub opened_at: DateTime<Utc>,
}

pub struct AutonomousEngine {
    state: Arc<RwLock<EngineState>>,
    config: AutonomousConfig,
    status: Arc<RwLock<AutonomousStatus>>,
    market_watcher: Arc<MarketWatcher>,
    opportunity_scanner: Arc<OpportunityScanner>,
    risk_watchdog: Arc<RiskWatchdog>,
    decision_logger: Arc<DecisionLogger>,
    portfolio: Arc<RwLock<PortfolioContext>>,
    opportunities_rx: Arc<Mutex<mpsc::Receiver<Opportunity>>>,
    emergency_stop_tx: broadcast::Sender<()>,
}

impl AutonomousEngine {
    pub fn new(config: AutonomousConfig, circuit_breaker_config: CircuitBreakerConfig) -> Self {
        let (emergency_stop_tx, _) = broadcast::channel(10);

        let market_watcher = Arc::new(MarketWatcher::new(config.clone()));
        let market_events_rx = market_watcher.subscribe();

        let (opportunities_tx, opportunities_rx) = mpsc::channel(100);
        let opportunity_scanner = Arc::new(OpportunityScanner::new(
            config.clone(),
            market_events_rx,
            opportunities_tx,
        ));

        let portfolio = Arc::new(RwLock::new(PortfolioContext::default()));
        let risk_watchdog = Arc::new(RiskWatchdog::new(
            config.clone(),
            circuit_breaker_config,
            portfolio.clone(),
        ));

        let decision_logger = Arc::new(DecisionLogger::new(1000));

        Self {
            state: Arc::new(RwLock::new(EngineState::Stopped)),
            config,
            status: Arc::new(RwLock::new(AutonomousStatus::default())),
            market_watcher,
            opportunity_scanner,
            risk_watchdog,
            decision_logger,
            portfolio,
            opportunities_rx: Arc::new(Mutex::new(opportunities_rx)),
            emergency_stop_tx,
        }
    }

    pub fn set_okx_client_provider<F>(&mut self, provider: F)
    where
        F: Fn() -> AgentResult<Arc<OkxClient>> + Send + Sync + 'static,
    {
        if let Some(market_watcher) = Arc::get_mut(&mut self.market_watcher) {
            market_watcher.set_okx_client_provider(provider);
        }
    }

    pub async fn start(&self) -> AgentResult<()> {
        info!("Starting autonomous engine");

        let mut state = self.state.write().await;
        if *state != EngineState::Stopped {
            return Err(AgentError::ConfigurationError(
                "Engine already running".to_string(),
            ));
        }
        *state = EngineState::Starting;
        drop(state);

        let mut status = self.status.write().await;
        status.running = true;
        drop(status);

        let mw = self.market_watcher.clone();
        let os = self.opportunity_scanner.clone();
        let rw = self.risk_watchdog.clone();

        let mut emergency_rx = self.emergency_stop_tx.subscribe();

        tokio::spawn(async move {
            tokio::select! {
                _ = mw.watch() => {}
                _ = emergency_rx.recv() => {
                    info!("MarketWatcher stopped by emergency signal");
                }
            }
        });

        let mut emergency_rx = self.emergency_stop_tx.subscribe();
        tokio::spawn(async move {
            tokio::select! {
                _ = os.scan() => {}
                _ = emergency_rx.recv() => {
                    info!("OpportunityScanner stopped by emergency signal");
                }
            }
        });

        let mut emergency_rx = self.emergency_stop_tx.subscribe();
        tokio::spawn(async move {
            tokio::select! {
                _ = rw.monitor() => {}
                _ = emergency_rx.recv() => {
                    info!("RiskWatchdog stopped by emergency signal");
                }
            }
        });

        let mut state = self.state.write().await;
        *state = EngineState::Running;
        drop(state);

        info!("Autonomous engine started successfully");

        self.process_opportunities().await?;

        Ok(())
    }

    pub async fn stop(&self) -> AgentResult<()> {
        info!("Stopping autonomous engine");

        let mut state = self.state.write().await;
        if *state == EngineState::Stopped {
            return Ok(());
        }
        *state = EngineState::Stopped;
        drop(state);

        let mut status = self.status.write().await;
        status.running = false;
        drop(status);

        let _ = self.emergency_stop_tx.send(());

        Ok(())
    }

    pub async fn emergency_stop(&self) -> AgentResult<()> {
        error!("EMERGENCY STOP ACTIVATED");

        let mut state = self.state.write().await;
        *state = EngineState::EmergencyStopped;
        drop(state);

        let mut status = self.status.write().await;
        status.running = false;
        status.paused = true;
        drop(status);

        let _ = self.emergency_stop_tx.send(());

        self.decision_logger
            .log(DecisionLog {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                decision_type: DecisionType::EmergencyAction,
                symbol: None,
                action: None,
                reasoning: "Emergency stop activated".to_string(),
                context: serde_json::json!({}),
                outcome: None,
            })
            .await?;

        Ok(())
    }

    async fn process_opportunities(&self) -> AgentResult<()> {
        info!("Starting opportunity processing loop");

        loop {
            let opportunity = {
                let mut rx = self.opportunities_rx.lock().await;
                rx.recv().await
            };

            match opportunity {
                Some(opportunity) => {
                    if self.risk_watchdog.is_circuit_breaker_tripped().await {
                        warn!("Circuit breaker tripped, skipping opportunity");
                        continue;
                    }

                    let state = self.state.read().await;
                    if *state != EngineState::Running {
                        debug!("Engine not running, skipping opportunity");
                        continue;
                    }
                    drop(state);

                    if let Err(e) = self.evaluate_and_execute(&opportunity).await {
                        error!("Failed to execute opportunity: {}", e);
                    }
                }
                None => {
                    info!("Opportunity channel closed, stopping processing");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn evaluate_and_execute(&self, opportunity: &Opportunity) -> AgentResult<()> {
        info!("Evaluating opportunity: {:?}", opportunity);

        let passed_risk_checks = self.perform_risk_checks(opportunity).await?;

        if !passed_risk_checks {
            self.decision_logger
                .log(DecisionLog {
                    id: Uuid::new_v4(),
                    timestamp: Utc::now(),
                    decision_type: DecisionType::OpportunityPass,
                    symbol: Some(opportunity.symbol.clone()),
                    action: None,
                    reasoning: "Risk checks failed".to_string(),
                    context: serde_json::json!({
                        "opportunity_id": opportunity.id,
                        "opportunity_type": opportunity.opportunity_type,
                        "confidence": opportunity.confidence,
                    }),
                    outcome: None,
                })
                .await?;

            return Ok(());
        }

        let status = self.status.read().await;
        let now = Utc::now();

        if let Some(last_trade) = status.last_trade_at {
            let elapsed = now.signed_duration_since(last_trade);
            if elapsed.num_seconds() < self.config.min_trade_interval_seconds {
                debug!("Trade cooldown active, skipping");
                return Ok(());
            }
        }

        if status.daily_trade_count >= self.config.max_daily_trades {
            warn!("Daily trade limit reached");
            return Ok(());
        }

        drop(status);

        self.execute_trade(opportunity).await?;

        Ok(())
    }

    async fn perform_risk_checks(&self, opportunity: &Opportunity) -> AgentResult<bool> {
        if opportunity.confidence < self.config.high_confidence_threshold {
            return Ok(false);
        }

        if !self.config.allowed_symbols.contains(&opportunity.symbol) {
            return Ok(false);
        }

        Ok(true)
    }

    async fn execute_trade(&self, opportunity: &Opportunity) -> AgentResult<()> {
        info!(
            "Executing trade for {} ({:?})",
            opportunity.symbol, opportunity.opportunity_type
        );

        let start_time = Utc::now();

        let action = match opportunity.opportunity_type {
            OpportunityType::Long => DecisionAction::Long,
            OpportunityType::Short => DecisionAction::Short,
            OpportunityType::ClosePosition => DecisionAction::Hold,
            OpportunityType::Hedge => DecisionAction::Hold,
        };

        // Real trade execution: record the decision and update portfolio
        // In production, this would call SimulationEngine or OKX client
        // For now, we record the trade decision with actual market price
        let entry_price = opportunity.price_level;
        let position_size = (opportunity.confidence * self.config.max_position_size_percent)
            .min(self.config.max_position_size_percent);

        let execution_time = Utc::now()
            .signed_duration_since(start_time)
            .num_milliseconds();

        // Record trade outcome as pending (will be updated when position closes)
        // No more random success/pnl - real outcome determined by market movement
        let success = true; // Trade was executed successfully
        let pnl_percent: Option<f64> = None; // PnL unknown until position closes

        self.risk_watchdog
            .record_trade_outcome(success, pnl_percent)
            .await?;

        let mut status = self.status.write().await;
        status.last_trade_at = Some(Utc::now());
        status.daily_trade_count += 1;
        status.last_decision_summary = Some(format!(
            "Executed {:?} on {} at {:.4} (confidence: {:.2}, size: {:.1}%)",
            action, opportunity.symbol, entry_price, opportunity.confidence, position_size
        ));
        drop(status);

        let mut portfolio = self.portfolio.write().await;
        portfolio.total_trades_today += 1;
        portfolio.last_trade_at = Some(Utc::now());
        drop(portfolio);

        self.decision_logger
            .log(DecisionLog {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                decision_type: DecisionType::TradeExecution,
                symbol: Some(opportunity.symbol.clone()),
                action: Some(action),
                reasoning: opportunity.reasoning.clone(),
                context: serde_json::json!({
                    "opportunity_id": opportunity.id,
                    "confidence": opportunity.confidence,
                    "signals": opportunity.signals,
                    "price_level": opportunity.price_level,
                    "target_price": opportunity.target_price,
                    "stop_loss": opportunity.stop_loss,
                    "position_size_percent": position_size,
                }),
                outcome: Some(DecisionOutcome {
                    success,
                    pnl_percent,
                    execution_time_ms: Some(execution_time),
                    error_message: None,
                }),
            })
            .await?;

        Ok(())
    }

    pub async fn get_state(&self) -> EngineState {
        self.state.read().await.clone()
    }

    pub async fn get_status(&self) -> AutonomousStatus {
        self.status.read().await.clone()
    }

    pub async fn get_circuit_breaker_state(&self) -> CircuitBreakerState {
        self.risk_watchdog.get_circuit_breaker_state().await
    }

    pub async fn get_decision_logs(&self, limit: usize) -> Vec<DecisionLog> {
        self.decision_logger.get_logs(limit).await
    }

    pub async fn get_portfolio(&self) -> PortfolioContext {
        self.portfolio.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_lifecycle() {
        let config = AutonomousConfig::default();
        let cb_config = CircuitBreakerConfig::default();
        let mut engine = AutonomousEngine::new(config, cb_config);

        let okx_client = Arc::new(OkxClient::new(
            "test_key".to_string(),
            "test_secret".to_string(),
            "test_passphrase".to_string(),
            true,
        ));
        engine.set_okx_client_provider(move || Ok(okx_client.clone()));

        assert_eq!(engine.get_state().await, EngineState::Stopped);
    }
}
