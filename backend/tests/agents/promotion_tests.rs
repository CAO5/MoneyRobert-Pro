use moneyrobert_rs::agents::models::{DemotionTrigger, PromotionEligibility, RollingStats};
use moneyrobert_rs::agents::promotion::PromotionSystem;

#[test]
fn level_requirements_become_stricter() {
    let level_1 = PromotionSystem::get_level_requirements(1);
    let level_2 = PromotionSystem::get_level_requirements(2);
    let level_3 = PromotionSystem::get_level_requirements(3);

    assert!(level_2.min_trades > level_1.min_trades);
    assert!(level_3.min_profit_loss_ratio > level_2.min_profit_loss_ratio);
    assert!(level_3.max_drawdown_percent < level_1.max_drawdown_percent);
}

#[test]
fn next_level_stops_at_live_level() {
    assert_eq!(PromotionSystem::get_next_level(0), Some(1));
    assert_eq!(PromotionSystem::get_next_level(1), Some(2));
    assert_eq!(PromotionSystem::get_next_level(2), Some(3));
    assert_eq!(PromotionSystem::get_next_level(3), None);
}

#[test]
fn promotion_eligibility_snapshot_is_explicit() {
    let stats = RollingStats {
        total_trades: 100,
        winning_trades: 60,
        losing_trades: 40,
        win_rate: 0.60,
        avg_pnl_percent: 0.5,
        profit_loss_ratio: 1.6,
        max_drawdown_percent: 5.0,
        running_days: 30,
        daily_loss_percent: 0.5,
        consecutive_days_without_risk_trigger: 12,
        weekly_loss_percent: 1.2,
    };
    let eligibility = PromotionEligibility {
        eligible: true,
        current_level: 1,
        next_level: Some(2),
        stats,
        requirements_met: true,
        missing_requirements: vec![],
    };

    assert!(eligibility.eligible);
    assert_eq!(eligibility.next_level, Some(2));
    assert_eq!(eligibility.stats.total_trades, 100);
}

#[test]
fn demotion_trigger_records_direction_and_reason() {
    let trigger = DemotionTrigger {
        from_level: 2,
        to_level: 1,
        reason: "Excessive drawdown".to_string(),
    };
    assert_eq!(trigger.from_level, 2);
    assert_eq!(trigger.to_level, 1);
    assert!(trigger.reason.contains("drawdown"));
}
