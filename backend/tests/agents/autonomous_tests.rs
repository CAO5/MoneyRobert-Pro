
use moneyrobert_rs::agents::autonomous::*;
use moneyrobert_rs::agents::models::*;
use chrono::Utc;
use uuid::Uuid;
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_state_variants() {
        let states = vec![
            EngineState::Stopped,
            EngineState::Starting,
            EngineState::Running,
            EngineState::Paused,
            EngineState::EmergencyStopped,
            EngineState::CircuitBreaker,
        ];
        
        assert_eq!(states.len(), 6);
    }

    #[test]
    fn test_engine_state_equality() {
        assert_eq!(EngineState::Stopped, EngineState::Stopped);
        assert_eq!(EngineState::Running, EngineState::Running);
        assert_eq!(EngineState::EmergencyStopped, EngineState::EmergencyStopped);
        
        assert_ne!(EngineState::Stopped, EngineState::Running);
        assert_ne!(EngineState::Running, EngineState::Paused);
        assert_ne!(EngineState::Paused, EngineState::CircuitBreaker);
    }

    #[test]
    fn test_market_event_type_variants() {
        let types = vec![
            MarketEventType::PriceUpdate,
            MarketEventType::VolumeSpike,
            MarketEventType::OrderBookUpdate,
            MarketEventType::FundingRateUpdate,
            MarketEventType::OpenInterestChange,
            MarketEventType::LongShortRatioChange,
            MarketEventType::VolatilitySpike,
        ];
        
        assert_eq!(types.len(), 7);
    }

    #[test]
    fn test_opportunity_type_variants() {
        let types = vec![
            OpportunityType::Long,
            OpportunityType::Short,
            OpportunityType::ClosePosition,
            OpportunityType::Hedge,
        ];
        
        assert_eq!(types.len(), 4);
    }

    #[test]
    fn test_signal_type_variants() {
        let types = vec![
            SignalType::Technical,
            SignalType::Fundamental,
            SignalType::Sentiment,
            SignalType::OnChain,
            SignalType::OrderFlow,
        ];
        
        assert_eq!(types.len(), 5);
    }

    #[test]
    fn test_risk_severity_variants() {
        let severities = vec![
            RiskSeverity::Info,
            RiskSeverity::Warning,
            RiskSeverity::High,
            RiskSeverity::Critical,
            RiskSeverity::Emergency,
        ];
        
        assert_eq!(severities.len(), 5);
    }

    #[test]
    fn test_risk_severity_ordering() {
        assert!(RiskSeverity::Info &lt; RiskSeverity::Warning);
        assert!(RiskSeverity::Warning &lt; RiskSeverity::High);
        assert!(RiskSeverity::High &lt; RiskSeverity::Critical);
        assert!(RiskSeverity::Critical &lt; RiskSeverity::Emergency);
    }

    #[test]
    fn test_risk_alert_type_variants() {
        let types = vec![
            RiskAlertType::DrawdownWarning,
            RiskAlertType::LossLimitBreach,
            RiskAlertType::PositionLimitBreach,
            RiskAlertType::VolatilityBreach,
            RiskAlertType::LiquidityRisk,
            RiskAlertType::ExecutionFailure,
            RiskAlertType::MarketDisruption,
            RiskAlertType::SystemHealth,
        ];
        
        assert_eq!(types.len(), 8);
    }

    #[test]
    fn test_decision_type_variants() {
        let types = vec![
            DecisionType::TradeExecution,
            DecisionType::RiskMitigation,
            DecisionType::OpportunityPass,
            DecisionType::EmergencyAction,
            DecisionType::CircuitBreakerAction,
        ];
        
        assert_eq!(types.len(), 5);
    }

    #[test]
    fn test_market_event_creation() {
        let event = MarketEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: MarketEventType::PriceUpdate,
            symbol: "DOGE-USDT".to_string(),
            data: serde_json::json!({ "price": 0.22 }),
        };
        
        assert_eq!(event.symbol, "DOGE-USDT");
        assert_eq!(event.event_type, MarketEventType::PriceUpdate);
    }

    #[test]
    fn test_opportunity_creation() {
        let opportunity = Opportunity {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            symbol: "DOGE-USDT".to_string(),
            opportunity_type: OpportunityType::Long,
            confidence: 0.85,
            price_level: 0.22,
            target_price: Some(0.25),
            stop_loss: Some(0.20),
            reasoning: "Strong bullish momentum".to_string(),
            signals: vec![
                Signal {
                    source: "RSI".to_string(),
                    signal_type: SignalType::Technical,
                    strength: 0.8,
                    description: "Oversold condition".to_string(),
                },
            ],
        };
        
        assert_eq!(opportunity.symbol, "DOGE-USDT");
        assert_eq!(opportunity.opportunity_type, OpportunityType::Long);
        assert_eq!(opportunity.confidence, 0.85);
        assert_eq!(opportunity.signals.len(), 1);
    }

    #[test]
    fn test_signal_creation() {
        let signal = Signal {
            source: "MACD".to_string(),
            signal_type: SignalType::Technical,
            strength: 0.75,
            description: "Bullish crossover".to_string(),
        };
        
        assert_eq!(signal.source, "MACD");
        assert_eq!(signal.signal_type, SignalType::Technical);
        assert_eq!(signal.strength, 0.75);
    }

    #[test]
    fn test_risk_alert_creation() {
        let alert = RiskAlert {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            severity: RiskSeverity::Warning,
            alert_type: RiskAlertType::DrawdownWarning,
            symbol: Some("DOGE-USDT".to_string()),
            message: "Drawdown approaching limit".to_string(),
            details: serde_json::json!({ "current_drawdown": 2.5, "limit": 3.0 }),
        };
        
        assert_eq!(alert.severity, RiskSeverity::Warning);
        assert_eq!(alert.alert_type, RiskAlertType::DrawdownWarning);
        assert_eq!(alert.symbol, Some("DOGE-USDT".to_string()));
    }

    #[test]
    fn test_circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        
        assert_eq!(config.max_daily_loss_percent, 5.0);
        assert_eq!(config.max_consecutive_losses, 3);
        assert_eq!(config.max_drawdown_percent, 10.0);
        assert_eq!(config.cool_down_period_seconds, 3600);
    }

    #[test]
    fn test_circuit_breaker_config_creation() {
        let config = CircuitBreakerConfig {
            max_daily_loss_percent: 8.0,
            max_consecutive_losses: 5,
            max_drawdown_percent: 15.0,
            cool_down_period_seconds: 7200,
        };
        
        assert_eq!(config.max_daily_loss_percent, 8.0);
        assert_eq!(config.max_consecutive_losses, 5);
        assert_eq!(config.cool_down_period_seconds, 7200);
    }

    #[test]
    fn test_circuit_breaker_state_default() {
        let state = CircuitBreakerState::default();
        
        assert!(!state.tripped);
        assert_eq!(state.consecutive_losses, 0);
        assert_eq!(state.daily_loss_percent, 0.0);
        assert_eq!(state.current_drawdown_percent, 0.0);
        assert_eq!(state.total_trades, 0);
    }

    #[test]
    fn test_circuit_breaker_state_creation() {
        let state = CircuitBreakerState {
            tripped: true,
            trip_time: Some(Utc::now()),
            trip_reason: Some("Daily loss limit exceeded".to_string()),
            consecutive_losses: 4,
            daily_loss_percent: 5.2,
            current_drawdown_percent: 8.5,
            total_trades: 100,
            winning_trades: 55,
            losing_trades: 45,
        };
        
        assert!(state.tripped);
        assert_eq!(state.consecutive_losses, 4);
        assert_eq!(state.daily_loss_percent, 5.2);
    }

    #[test]
    fn test_decision_log_creation() {
        let log = DecisionLog {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            decision_type: DecisionType::TradeExecution,
            symbol: Some("DOGE-USDT".to_string()),
            action: Some(DecisionAction::Long),
            reasoning: "Strong bullish signal".to_string(),
            context: serde_json::json!({ "confidence": 0.85 }),
            outcome: Some(DecisionOutcome {
                success: true,
                pnl_percent: Some(2.5),
                execution_time_ms: Some(150),
                error_message: None,
            }),
        };
        
        assert_eq!(log.decision_type, DecisionType::TradeExecution);
        assert_eq!(log.symbol, Some("DOGE-USDT".to_string()));
        assert_eq!(log.action, Some(DecisionAction::Long));
        assert!(log.outcome.is_some());
    }

    #[test]
    fn test_decision_outcome_creation_success() {
        let outcome = DecisionOutcome {
            success: true,
            pnl_percent: Some(3.5),
            execution_time_ms: Some(120),
            error_message: None,
        };
        
        assert!(outcome.success);
        assert_eq!(outcome.pnl_percent, Some(3.5));
        assert_eq!(outcome.execution_time_ms, Some(120));
        assert!(outcome.error_message.is_none());
    }

    #[test]
    fn test_decision_outcome_creation_failure() {
        let outcome = DecisionOutcome {
            success: false,
            pnl_percent: None,
            execution_time_ms: None,
            error_message: Some("Insufficient margin".to_string()),
        };
        
        assert!(!outcome.success);
        assert!(outcome.pnl_percent.is_none());
        assert!(outcome.error_message.is_some());
    }

    #[test]
    fn test_portfolio_context_default() {
        let portfolio = PortfolioContext::default();
        
        assert_eq!(portfolio.total_balance, 10000.0);
        assert_eq!(portfolio.available_balance, 10000.0);
        assert!(portfolio.positions.is_empty());
        assert_eq!(portfolio.daily_pnl_percent, 0.0);
        assert_eq!(portfolio.total_trades_today, 0);
    }

    #[test]
    fn test_portfolio_context_creation() {
        let portfolio = PortfolioContext {
            total_balance: 15000.0,
            available_balance: 10000.0,
            positions: vec![],
            daily_pnl_percent: 5.0,
            daily_loss_percent: 0.0,
            current_drawdown_percent: 2.5,
            total_trades_today: 15,
            last_trade_at: Some(Utc::now()),
        };
        
        assert_eq!(portfolio.total_balance, 15000.0);
        assert_eq!(portfolio.available_balance, 10000.0);
        assert_eq!(portfolio.daily_pnl_percent, 5.0);
        assert_eq!(portfolio.total_trades_today, 15);
    }

    #[test]
    fn test_position_creation() {
        let position = Position {
            id: Uuid::new_v4(),
            symbol: "DOGE-USDT".to_string(),
            direction: "long".to_string(),
            entry_price: 0.22,
            current_price: 0.23,
            size: 1000.0,
            leverage: 3,
            unrealized_pnl_percent: 4.5,
            stop_loss: Some(0.20),
            take_profit: Some(0.25),
            opened_at: Utc::now(),
        };
        
        assert_eq!(position.symbol, "DOGE-USDT");
        assert_eq!(position.direction, "long");
        assert_eq!(position.leverage, 3);
        assert_eq!(position.unrealized_pnl_percent, 4.5);
    }

    #[tokio::test]
    async fn test_autonomous_engine_creation() {
        let config = AutonomousConfig::default();
        let cb_config = CircuitBreakerConfig::default();
        let engine = AutonomousEngine::new(config, cb_config);
        
        assert_eq!(engine.get_state().await, EngineState::Stopped);
        let status = engine.get_status().await;
        assert!(!status.running);
    }

    #[tokio::test]
    async fn test_decision_logger_creation() {
        let logger = DecisionLogger::new(1000);
        
        let logs = logger.get_logs(10).await;
        assert!(logs.is_empty());
    }

    #[tokio::test]
    async fn test_risk_watchdog_state() {
        let config = AutonomousConfig::default();
        let cb_config = CircuitBreakerConfig::default();
        let portfolio = Arc::new(tokio::sync::RwLock::new(PortfolioContext::default()));
        let watchdog = RiskWatchdog::new(config, cb_config, portfolio);
        
        let state = watchdog.get_circuit_breaker_state().await;
        assert!(!state.tripped);
        assert_eq!(state.consecutive_losses, 0);
    }

    #[tokio::test]
    async fn test_risk_watchdog_record_trade_outcome_success() {
        let config = AutonomousConfig::default();
        let cb_config = CircuitBreakerConfig::default();
        let portfolio = Arc::new(tokio::sync::RwLock::new(PortfolioContext::default()));
        let watchdog = RiskWatchdog::new(config, cb_config, portfolio);
        
        watchdog.record_trade_outcome(true, Some(2.5)).await.unwrap();
        
        let state = watchdog.get_circuit_breaker_state().await;
        assert_eq!(state.total_trades, 1);
        assert_eq!(state.winning_trades, 1);
        assert_eq!(state.consecutive_losses, 0);
    }

    #[tokio::test]
    async fn test_risk_watchdog_record_trade_outcome_loss() {
        let config = AutonomousConfig::default();
        let cb_config = CircuitBreakerConfig::default();
        let portfolio = Arc::new(tokio::sync::RwLock::new(PortfolioContext::default()));
        let watchdog = RiskWatchdog::new(config, cb_config, portfolio);
        
        watchdog.record_trade_outcome(false, Some(-1.5)).await.unwrap();
        
        let state = watchdog.get_circuit_breaker_state().await;
        assert_eq!(state.total_trades, 1);
        assert_eq!(state.losing_trades, 1);
        assert_eq!(state.consecutive_losses, 1);
    }
}
