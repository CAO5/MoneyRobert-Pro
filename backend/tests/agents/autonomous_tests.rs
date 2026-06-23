use chrono::Utc;
use moneyrobert_rs::agents::autonomous::*;
use moneyrobert_rs::agents::models::{AutonomousConfig, DecisionAction};
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn risk_severity_ordering_is_monotonic() {
    assert!(RiskSeverity::Info < RiskSeverity::Warning);
    assert!(RiskSeverity::Warning < RiskSeverity::High);
    assert!(RiskSeverity::High < RiskSeverity::Critical);
    assert!(RiskSeverity::Critical < RiskSeverity::Emergency);
}

#[test]
fn circuit_breaker_defaults_are_conservative() {
    let config = CircuitBreakerConfig::default();
    assert_eq!(config.max_daily_loss_percent, 5.0);
    assert_eq!(config.max_consecutive_losses, 3);
    assert_eq!(config.max_drawdown_percent, 10.0);
    assert_eq!(config.cool_down_period_seconds, 3600);

    let state = CircuitBreakerState::default();
    assert!(!state.tripped);
    assert_eq!(state.total_trades, 0);
}

#[test]
fn opportunity_and_decision_log_are_structured() {
    let opportunity = Opportunity {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        symbol: "BTC-USDT".to_string(),
        opportunity_type: OpportunityType::Long,
        confidence: 0.82,
        price_level: 100.0,
        target_price: Some(110.0),
        stop_loss: Some(95.0),
        reasoning: "momentum".to_string(),
        signals: vec![Signal {
            source: "RSI".to_string(),
            signal_type: SignalType::Technical,
            strength: 0.7,
            description: "oversold rebound".to_string(),
        }],
    };
    assert_eq!(opportunity.opportunity_type, OpportunityType::Long);
    assert_eq!(opportunity.signals.len(), 1);

    let log = DecisionLog {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        decision_type: DecisionType::TradeExecution,
        symbol: Some("BTC-USDT".to_string()),
        action: Some(DecisionAction::Long),
        reasoning: "risk accepted".to_string(),
        context: serde_json::json!({"confidence": 0.82}),
        outcome: Some(DecisionOutcome {
            success: true,
            pnl_percent: Some(1.2),
            execution_time_ms: Some(25),
            error_message: None,
        }),
    };
    assert_eq!(log.action, Some(DecisionAction::Long));
    assert!(log.outcome.unwrap().success);
}

#[tokio::test]
async fn decision_logger_returns_newest_first_and_caps_limit() {
    let logger = DecisionLogger::new(10);
    for idx in 0..3 {
        logger
            .log(DecisionLog {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                decision_type: DecisionType::OpportunityPass,
                symbol: Some(format!("SYM-{idx}")),
                action: None,
                reasoning: format!("reason-{idx}"),
                context: serde_json::json!({}),
                outcome: None,
            })
            .await
            .unwrap();
    }

    let logs = logger.get_logs(2).await;
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].symbol.as_deref(), Some("SYM-2"));
    assert_eq!(logs[1].symbol.as_deref(), Some("SYM-1"));
}

#[tokio::test]
async fn risk_watchdog_tracks_trade_outcomes_without_database() {
    let portfolio = Arc::new(tokio::sync::RwLock::new(PortfolioContext::default()));
    let watchdog = RiskWatchdog::new(
        AutonomousConfig::default(),
        CircuitBreakerConfig::default(),
        portfolio,
    );

    watchdog.record_trade_outcome(true, Some(2.0)).await.unwrap();
    watchdog.record_trade_outcome(false, Some(-1.0)).await.unwrap();

    let state = watchdog.get_circuit_breaker_state().await;
    assert_eq!(state.total_trades, 2);
    assert_eq!(state.winning_trades, 1);
    assert_eq!(state.losing_trades, 1);
    assert_eq!(state.consecutive_losses, 1);
}

#[tokio::test]
async fn autonomous_engine_starts_stopped() {
    let engine = AutonomousEngine::new(AutonomousConfig::default(), CircuitBreakerConfig::default());
    assert_eq!(engine.get_state().await, EngineState::Stopped);
    let status = engine.get_status().await;
    assert!(!status.running);
    assert!(!status.paused);
}


