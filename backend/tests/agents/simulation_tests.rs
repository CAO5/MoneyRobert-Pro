
use moneyrobert_rs::agents::simulation::*;
use moneyrobert_rs::agents::models::*;
use chrono::Utc;
use uuid::Uuid;
use sqlx::PgPool;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_pnl_long_profit() {
        let (pnl, pnl_percent) = SimulationEngine::calculate_pnl("long", 100.0, 110.0, 1.0, 1);
        assert_eq!(pnl, 10.0);
        assert_eq!(pnl_percent, 10.0);
    }

    #[test]
    fn test_calculate_pnl_short_profit() {
        let (pnl, pnl_percent) = SimulationEngine::calculate_pnl("short", 100.0, 90.0, 1.0, 1);
        assert_eq!(pnl, 10.0);
        assert_eq!(pnl_percent, 10.0);
    }

    #[test]
    fn test_calculate_pnl_long_loss() {
        let (pnl, pnl_percent) = SimulationEngine::calculate_pnl("long", 100.0, 90.0, 1.0, 1);
        assert_eq!(pnl, -10.0);
        assert_eq!(pnl_percent, -10.0);
    }

    #[test]
    fn test_calculate_pnl_short_loss() {
        let (pnl, pnl_percent) = SimulationEngine::calculate_pnl("short", 100.0, 110.0, 1.0, 1);
        assert_eq!(pnl, -10.0);
        assert_eq!(pnl_percent, -10.0);
    }

    #[test]
    fn test_calculate_pnl_with_leverage() {
        let (pnl, pnl_percent) = SimulationEngine::calculate_pnl("long", 100.0, 110.0, 1.0, 5);
        assert_eq!(pnl, 50.0);
        assert_eq!(pnl_percent, 50.0);
    }

    #[test]
    fn test_should_trigger_stop_loss_long() {
        assert!(SimulationEngine::should_trigger_stop_loss("long", 95.0, 94.0));
        assert!(!SimulationEngine::should_trigger_stop_loss("long", 95.0, 96.0));
        assert!(!SimulationEngine::should_trigger_stop_loss("long", 95.0, 95.0));
    }

    #[test]
    fn test_should_trigger_stop_loss_short() {
        assert!(SimulationEngine::should_trigger_stop_loss("short", 105.0, 106.0));
        assert!(!SimulationEngine::should_trigger_stop_loss("short", 105.0, 104.0));
        assert!(!SimulationEngine::should_trigger_stop_loss("short", 105.0, 105.0));
    }

    #[test]
    fn test_should_trigger_take_profit_long() {
        assert!(SimulationEngine::should_trigger_take_profit("long", 110.0, 111.0));
        assert!(!SimulationEngine::should_trigger_take_profit("long", 110.0, 109.0));
        assert!(!SimulationEngine::should_trigger_take_profit("long", 110.0, 110.0));
    }

    #[test]
    fn test_should_trigger_take_profit_short() {
        assert!(SimulationEngine::should_trigger_take_profit("short", 90.0, 89.0));
        assert!(!SimulationEngine::should_trigger_take_profit("short", 90.0, 91.0));
        assert!(!SimulationEngine::should_trigger_take_profit("short", 90.0, 90.0));
    }

    #[test]
    fn test_execution_mode_variants() {
        let paper = ExecutionMode::Paper;
        let demo = ExecutionMode::Demo;
        let live = ExecutionMode::Live;
        
        assert_ne!(paper, demo);
        assert_ne!(paper, live);
        assert_ne!(demo, live);
    }

    #[test]
    fn test_execution_mode_default() {
        let mode = ExecutionMode::default();
        assert_eq!(mode, ExecutionMode::Paper);
    }

    #[test]
    fn test_market_snapshot_creation() {
        let snapshot = MarketSnapshot {
            symbol: "DOGE-USDT".to_string(),
            current_price: 0.22,
            open_24h: 0.2156,
            high_24h: 0.231,
            low_24h: 0.209,
            close_24h: 0.2222,
            volume_24h: 100000000.0,
            price_change_percent_24h: 1.0,
            funding_rate: Some(-0.0001),
            open_interest: Some(500000000.0),
            long_short_ratio: Some(1.2),
            rsi_14: Some(45.0),
            macd_signal: Some(-0.001),
            timestamp: Utc::now(),
        };
        
        assert_eq!(snapshot.symbol, "DOGE-USDT");
        assert_eq!(snapshot.current_price, 0.22);
    }

    #[test]
    fn test_ai_simulation_trade_creation() {
        let trade = AiSimulationTrade {
            id: Uuid::new_v4(),
            config_id: Uuid::new_v4(),
            symbol: "DOGE-USDT".to_string(),
            mode: "paper".to_string(),
            direction: "long".to_string(),
            entry_price: 0.22,
            exit_price: None,
            quantity: 1000.0,
            leverage: 3,
            stop_loss: Some(0.20),
            take_profit: Some(0.25),
            ai_confidence: Some(0.75),
            ai_reasoning: Some(serde_json::json!({})),
            agent_session_id: Some(Uuid::new_v4()),
            pnl: None,
            pnl_percent: None,
            fee_percent: 0.1,
            net_pnl_percent: None,
            status: "open".to_string(),
            close_reason: None,
            holding_duration_minutes: None,
            opened_at: Utc::now(),
            closed_at: None,
        };
        
        assert_eq!(trade.symbol, "DOGE-USDT");
        assert_eq!(trade.direction, "long");
        assert_eq!(trade.status, "open");
    }

    #[test]
    fn test_ai_simulation_config_creation() {
        let config = AiSimulationConfig {
            id: Uuid::new_v4(),
            user_id: 123,
            symbol: "DOGE-USDT".to_string(),
            mode: "paper".to_string(),
            level: 1,
            status: "active".to_string(),
            initial_balance: 10000.0,
            current_balance: 10000.0,
            max_position_size_percent: 10.0,
            max_leverage: 5,
            max_daily_trades: 20,
            max_daily_loss_percent: 3.0,
            max_weekly_loss_percent: 5.0,
            max_single_trade_loss_percent: 1.0,
            ai_confidence_threshold: 0.8,
            analysis_interval_minutes: 15,
            allowed_symbols: vec!["DOGE-USDT".to_string()],
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
        
        assert_eq!(config.user_id, 123);
        assert_eq!(config.initial_balance, 10000.0);
        assert_eq!(config.status, "active");
    }
}
