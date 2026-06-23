use moneyrobert_rs::agents::config::AgentConfig;
use moneyrobert_rs::agents::risk::{RiskCheckResult, RiskChecker, RiskLevel};
use sqlx::postgres::PgPoolOptions;

#[test]
fn risk_levels_order_by_severity() {
    assert!(RiskLevel::Low < RiskLevel::Medium);
    assert!(RiskLevel::Medium < RiskLevel::High);
    assert!(RiskLevel::High < RiskLevel::Critical);
}

#[test]
fn risk_check_result_carries_alerts_and_level() {
    let result = RiskCheckResult {
        passed: false,
        alerts: vec!["Position too large".to_string()],
        risk_level: RiskLevel::High,
    };
    assert!(!result.passed);
    assert_eq!(result.alerts.len(), 1);
    assert_eq!(result.risk_level, RiskLevel::High);
}

#[tokio::test]
async fn aggregate_risk_rejects_position_and_leverage_without_database_queries() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://user:pass@localhost/test")
        .unwrap();
    let checker = RiskChecker::new(pool, AgentConfig::default());

    let result = checker
        .check_all_risk_factors(1, None, 99.0, 99)
        .await
        .unwrap();

    assert!(!result.passed);
    assert_eq!(result.risk_level, RiskLevel::High);
    assert!(result.alerts.iter().any(|alert| alert.contains("Position size")));
    assert!(result.alerts.iter().any(|alert| alert.contains("Leverage")));
}
