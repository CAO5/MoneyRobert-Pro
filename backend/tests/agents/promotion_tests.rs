
use moneyrobert_rs::agents::promotion::*;
use moneyrobert_rs::agents::models::*;
use chrono::Utc;
use uuid::Uuid;
use sqlx::PgPool;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_config(
        level: i32,
        total_trades: i32,
        win_rate: f64,
        profit_loss_ratio: f64,
        running_days: i32,
        max_drawdown_percent: f64,
        consecutive_stop_losses: i32,
    ) -&gt; AiSimulationConfig {
        AiSimulationConfig {
            id: Uuid::new_v4(),
            user_id: 123,
            symbol: "DOGE-USDT".to_string(),
            mode: "paper".to_string(),
            level,
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
            total_trades,
            winning_trades: (total_trades as f64 * win_rate) as i32,
            losing_trades: total_trades - (total_trades as f64 * win_rate) as i32,
            win_rate,
            avg_pnl_percent: 0.5,
            profit_loss_ratio,
            max_drawdown_percent,
            sharpe_ratio: 1.2,
            weekly_pnl: 100.0,
            weekly_loss_percent: 1.0,
            daily_pnl: 20.0,
            daily_loss_percent: 0.5,
            consecutive_stop_losses,
            running_days,
            last_trade_at: None,
            promotion_eligible: false,
            risk_confirmation_signed: false,
            risk_confirmation_signed_at: None,
            max_acceptable_loss_amount: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_get_level_requirements_level_0() {
        let requirements = PromotionSystem::get_level_requirements(0);
        assert_eq!(requirements.level, 0);
        assert_eq!(requirements.min_trades, 0);
        assert_eq!(requirements.min_win_rate, 0.0);
        assert_eq!(requirements.min_profit_loss_ratio, 0.0);
        assert_eq!(requirements.min_running_days, 0);
    }

    #[test]
    fn test_get_level_requirements_level_1() {
        let requirements = PromotionSystem::get_level_requirements(1);
        assert_eq!(requirements.level, 1);
        assert_eq!(requirements.min_trades, 50);
        assert_eq!(requirements.min_win_rate, 0.55);
        assert_eq!(requirements.min_profit_loss_ratio, 1.2);
        assert_eq!(requirements.min_running_days, 14);
    }

    #[test]
    fn test_get_level_requirements_level_2() {
        let requirements = PromotionSystem::get_level_requirements(2);
        assert_eq!(requirements.level, 2);
        assert_eq!(requirements.min_trades, 100);
        assert_eq!(requirements.min_win_rate, 0.58);
        assert_eq!(requirements.min_profit_loss_ratio, 1.5);
        assert_eq!(requirements.min_running_days, 30);
    }

    #[test]
    fn test_get_level_requirements_level_3() {
        let requirements = PromotionSystem::get_level_requirements(3);
        assert_eq!(requirements.level, 3);
        assert_eq!(requirements.min_trades, 200);
        assert_eq!(requirements.min_win_rate, 0.60);
        assert_eq!(requirements.min_profit_loss_ratio, 1.8);
        assert_eq!(requirements.min_running_days, 60);
    }

    #[test]
    fn test_get_next_level_from_0() {
        assert_eq!(PromotionSystem::get_next_level(0), Some(1));
    }

    #[test]
    fn test_get_next_level_from_1() {
        assert_eq!(PromotionSystem::get_next_level(1), Some(2));
    }

    #[test]
    fn test_get_next_level_from_2() {
        assert_eq!(PromotionSystem::get_next_level(2), Some(3));
    }

    #[test]
    fn test_get_next_level_from_3() {
        assert_eq!(PromotionSystem::get_next_level(3), None);
    }

    #[tokio::test]
    async fn test_check_promotion_eligibility_max_level() {
        let system = PromotionSystem::new(PgPool::connect("postgres://user:pass@localhost/test").await.unwrap());
        let config = create_mock_config(3, 200, 0.60, 1.8, 60, 5.0, 2);
        
        let result = system.check_promotion_eligibility(&amp;config).await;
        assert!(result.is_ok());
        
        if let Ok(eligibility) = result {
            assert_eq!(eligibility.current_level, 3);
            assert_eq!(eligibility.next_level, None);
            assert!(!eligibility.eligible);
        }
    }

    #[test]
    fn test_rolling_stats_creation() {
        let stats = RollingStats {
            total_trades: 100,
            winning_trades: 58,
            losing_trades: 42,
            win_rate: 0.58,
            avg_pnl_percent: 0.5,
            profit_loss_ratio: 1.5,
            max_drawdown_percent: 5.0,
            running_days: 30,
            daily_loss_percent: 1.0,
            consecutive_days_without_risk_trigger: 10,
            weekly_loss_percent: 2.0,
        };
        
        assert_eq!(stats.total_trades, 100);
        assert_eq!(stats.win_rate, 0.58);
    }

    #[test]
    fn test_promotion_eligibility_creation() {
        let eligibility = PromotionEligibility {
            eligible: true,
            current_level: 1,
            next_level: Some(2),
            stats: RollingStats {
                total_trades: 100,
                winning_trades: 58,
                losing_trades: 42,
                win_rate: 0.58,
                avg_pnl_percent: 0.5,
                profit_loss_ratio: 1.5,
                max_drawdown_percent: 5.0,
                running_days: 30,
                daily_loss_percent: 1.0,
                consecutive_days_without_risk_trigger: 10,
                weekly_loss_percent: 2.0,
            },
            requirements_met: true,
            missing_requirements: Vec::new(),
        };
        
        assert_eq!(eligibility.current_level, 1);
        assert_eq!(eligibility.next_level, Some(2));
        assert!(eligibility.eligible);
    }

    #[test]
    fn test_demotion_trigger_creation() {
        let trigger = DemotionTrigger {
            from_level: 2,
            to_level: 1,
            reason: "Excessive drawdown".to_string(),
        };
        
        assert_eq!(trigger.from_level, 2);
        assert_eq!(trigger.to_level, 1);
    }
}
