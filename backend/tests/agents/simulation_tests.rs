use chrono::Utc;
use moneyrobert_rs::agents::models::{AiSimulationConfig, AiSimulationTrade, ExecutionMode, MarketSnapshot};
use uuid::Uuid;

#[test]
fn execution_mode_defaults_to_paper() {
    assert_eq!(ExecutionMode::default(), ExecutionMode::Paper);
    assert_ne!(ExecutionMode::Paper, ExecutionMode::Live);
}

#[test]
fn market_snapshot_preserves_derivative_inputs() {
    let snapshot = MarketSnapshot {
        symbol: "BTC-USDT".to_string(),
        current_price: 100.0,
        open_24h: 98.0,
        high_24h: 105.0,
        low_24h: 95.0,
        close_24h: 101.0,
        volume_24h: 1_000_000.0,
        price_change_percent_24h: 3.0,
        funding_rate: Some(-0.0001),
        open_interest: Some(500_000.0),
        long_short_ratio: Some(1.2),
        rsi_14: Some(45.0),
        macd_signal: Some(0.01),
        timestamp: Utc::now(),
    };

    assert_eq!(snapshot.symbol, "BTC-USDT");
    assert_eq!(snapshot.funding_rate, Some(-0.0001));
    assert_eq!(snapshot.long_short_ratio, Some(1.2));
}

#[test]
fn simulation_trade_and_config_are_complete_snapshots() {
    let trade = AiSimulationTrade {
        id: Uuid::new_v4(),
        config_id: Uuid::new_v4(),
        symbol: "ETH-USDT".to_string(),
        mode: "paper".to_string(),
        direction: "long".to_string(),
        entry_price: 100.0,
        exit_price: None,
        quantity: 2.0,
        leverage: 1,
        stop_loss: Some(95.0),
        take_profit: Some(110.0),
        ai_confidence: Some(0.7),
        ai_reasoning: Some(serde_json::json!({"reason": "test"})),
        agent_session_id: Some(Uuid::new_v4()),
        pnl: None,
        pnl_percent: None,
        fee_percent: 0.05,
        net_pnl_percent: None,
        status: "open".to_string(),
        close_reason: None,
        holding_duration_minutes: None,
        opened_at: Utc::now(),
        closed_at: None,
    };
    assert_eq!(trade.status, "open");

    let config = AiSimulationConfig {
        id: Uuid::new_v4(),
        user_id: 1,
        symbol: "ETH-USDT".to_string(),
        mode: "paper".to_string(),
        level: 1,
        status: "active".to_string(),
        initial_balance: 10_000.0,
        current_balance: 10_000.0,
        max_position_size_percent: 10.0,
        max_leverage: 3,
        max_daily_trades: 20,
        max_daily_loss_percent: 3.0,
        max_weekly_loss_percent: 5.0,
        max_single_trade_loss_percent: 1.0,
        ai_confidence_threshold: 0.8,
        analysis_interval_minutes: 15,
        analysis_interval: "15m".to_string(),
        allowed_symbols: vec!["ETH-USDT".to_string()],
        autonomous_mode_enabled: false,
        requires_manual_confirm: true,
        total_trades: 0,
        winning_trades: 0,
        losing_trades: 0,
        win_rate: 0.0,
        avg_pnl_percent: 0.0,
        profit_loss_ratio: 0.0,
        max_drawdown_percent: 0.0,
        sharpe_ratio: 0.0,
        weekly_pnl: 0.0,
        weekly_loss_percent: 0.0,
        daily_pnl: 0.0,
        daily_loss_percent: 0.0,
        consecutive_stop_losses: 0,
        running_days: 0,
        last_trade_at: None,
        promotion_eligible: false,
        risk_confirmation_signed: false,
        risk_confirmation_signed_at: None,
        max_acceptable_loss_amount: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    assert_eq!(config.analysis_interval, "15m");
    assert_eq!(config.allowed_symbols, vec!["ETH-USDT".to_string()]);
}
