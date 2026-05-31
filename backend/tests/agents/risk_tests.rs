
use moneyrobert_rs::agents::risk::*;
use moneyrobert_rs::agents::config::AgentConfig;
use moneyrobert_rs::agents::models::*;
use chrono::Utc;
use uuid::Uuid;
use sqlx::PgPool;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_agent_config() -&gt; AgentConfig {
        AgentConfig {
            autonomous: AutonomousConfig {
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
                allowed_symbols: vec!["DOGE-USDT".to_string()],
                emergency_stop: false,
            },
        }
    }

    #[test]
    fn test_risk_level_variants() {
        let levels = vec![
            RiskLevel::Low,
            RiskLevel::Medium,
            RiskLevel::High,
            RiskLevel::Critical,
        ];
        
        assert_eq!(levels.len(), 4);
    }

    #[test]
    fn test_risk_level_equality() {
        assert_eq!(RiskLevel::Low, RiskLevel::Low);
        assert_eq!(RiskLevel::Medium, RiskLevel::Medium);
        assert_eq!(RiskLevel::High, RiskLevel::High);
        assert_eq!(RiskLevel::Critical, RiskLevel::Critical);
        
        assert_ne!(RiskLevel::Low, RiskLevel::Medium);
        assert_ne!(RiskLevel::Medium, RiskLevel::High);
        assert_ne!(RiskLevel::High, RiskLevel::Critical);
    }

    #[test]
    fn test_risk_check_result_creation() {
        let result = RiskCheckResult {
            passed: true,
            alerts: Vec::new(),
            risk_level: RiskLevel::Low,
        };
        
        assert!(result.passed);
        assert_eq!(result.risk_level, RiskLevel::Low);
        assert!(result.alerts.is_empty());
    }

    #[test]
    fn test_risk_check_result_with_alerts() {
        let result = RiskCheckResult {
            passed: false,
            alerts: vec!["Position size too large".to_string()],
            risk_level: RiskLevel::High,
        };
        
        assert!(!result.passed);
        assert_eq!(result.risk_level, RiskLevel::High);
        assert_eq!(result.alerts.len(), 1);
    }

    #[test]
    fn test_check_position_size_within_limit() {
        let config = create_mock_agent_config();
        let checker = RiskChecker::new(PgPool::connect("postgres://user:pass@localhost/test").await.unwrap(), config);
        
        let result = checker.check_position_size(5.0);
        assert!(result.passed);
        assert_eq!(result.risk_level, RiskLevel::Low);
        assert!(result.alerts.is_empty());
    }

    #[test]
    fn test_check_position_size_approaching_limit() {
        let config = create_mock_agent_config();
        let checker = RiskChecker::new(PgPool::connect("postgres://user:pass@localhost/test").await.unwrap(), config);
        
        let result = checker.check_position_size(9.0);
        assert!(result.passed);
        assert_eq!(result.risk_level, RiskLevel::Medium);
        assert_eq!(result.alerts.len(), 1);
    }

    #[test]
    fn test_check_position_size_exceeds_limit() {
        let config = create_mock_agent_config();
        let checker = RiskChecker::new(PgPool::connect("postgres://user:pass@localhost/test").await.unwrap(), config);
        
        let result = checker.check_position_size(15.0);
        assert!(!result.passed);
        assert_eq!(result.risk_level, RiskLevel::High);
        assert_eq!(result.alerts.len(), 1);
    }

    #[test]
    fn test_check_leverage_within_limit() {
        let config = create_mock_agent_config();
        let checker = RiskChecker::new(PgPool::connect("postgres://user:pass@localhost/test").await.unwrap(), config);
        
        let result = checker.check_leverage(3);
        assert!(result.passed);
        assert_eq!(result.risk_level, RiskLevel::Low);
        assert!(result.alerts.is_empty());
    }

    #[test]
    fn test_check_leverage_approaching_limit() {
        let config = create_mock_agent_config();
        let checker = RiskChecker::new(PgPool::connect("postgres://user:pass@localhost/test").await.unwrap(), config);
        
        let result = checker.check_leverage(4);
        assert!(result.passed);
        assert_eq!(result.risk_level, RiskLevel::Medium);
        assert_eq!(result.alerts.len(), 1);
    }

    #[test]
    fn test_check_leverage_exceeds_limit() {
        let config = create_mock_agent_config();
        let checker = RiskChecker::new(PgPool::connect("postgres://user:pass@localhost/test").await.unwrap(), config);
        
        let result = checker.check_leverage(10);
        assert!(!result.passed);
        assert_eq!(result.risk_level, RiskLevel::High);
        assert_eq!(result.alerts.len(), 1);
    }

    #[test]
    fn test_autonomous_config_default() {
        let config = AutonomousConfig::default();
        
        assert_eq!(config.max_position_size_percent, 10.0);
        assert_eq!(config.max_leverage, 5);
        assert_eq!(config.max_daily_trades, 20);
        assert_eq!(config.max_daily_loss_percent, 3.0);
        assert!(!config.enabled);
        assert!(!config.emergency_stop);
    }

    #[test]
    fn test_autonomous_config_creation() {
        let config = AutonomousConfig {
            enabled: true,
            max_position_size_percent: 15.0,
            max_leverage: 10,
            max_daily_trades: 30,
            max_hourly_trades: 10,
            min_trade_interval_seconds: 600,
            max_daily_loss_percent: 5.0,
            max_weekly_loss_percent: 10.0,
            max_single_trade_loss_percent: 2.0,
            high_confidence_threshold: 0.9,
            allowed_symbols: vec!["BTC-USDT".to_string(), "ETH-USDT".to_string()],
            emergency_stop: false,
        };
        
        assert!(config.enabled);
        assert_eq!(config.max_position_size_percent, 15.0);
        assert_eq!(config.max_leverage, 10);
        assert_eq!(config.allowed_symbols.len(), 2);
    }

    #[test]
    fn test_autonomous_status_default() {
        let status = AutonomousStatus::default();
        
        assert!(!status.running);
        assert!(!status.paused);
        assert_eq!(status.daily_trade_count, 0);
        assert_eq!(status.hourly_trade_count, 0);
        assert_eq!(status.daily_pnl, 0.0);
        assert_eq!(status.weekly_pnl, 0.0);
        assert_eq!(status.consecutive_stop_losses, 0);
        assert!(status.last_trade_at.is_none());
        assert!(status.last_decision_summary.is_none());
    }

    #[test]
    fn test_autonomous_status_creation() {
        let status = AutonomousStatus {
            running: true,
            paused: false,
            last_trade_at: Some(Utc::now()),
            daily_trade_count: 15,
            hourly_trade_count: 3,
            daily_pnl: 150.0,
            weekly_pnl: 500.0,
            consecutive_stop_losses: 0,
            last_decision_summary: Some("Long position opened".to_string()),
        };
        
        assert!(status.running);
        assert!(!status.paused);
        assert_eq!(status.daily_trade_count, 15);
        assert_eq!(status.weekly_pnl, 500.0);
        assert!(status.last_decision_summary.is_some());
    }
}
