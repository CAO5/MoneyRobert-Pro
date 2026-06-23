use chrono::Utc;
use moneyrobert_rs::agents::models::*;
use uuid::Uuid;

#[test]
fn fund_manager_decision_requires_risk_context() {
    let decision = FundManagerDecision {
        session_id: Uuid::new_v4(),
        action: DecisionAction::Hold,
        symbol: "ETH-USDT".to_string(),
        confidence: 0.42,
        position_size_percent: 0.0,
        leverage: 1,
        stop_loss_percent: None,
        take_profit_percent: None,
        reasoning: "evidence is mixed".to_string(),
        agent_contributions: vec![],
        risk_assessment: RiskAssessment {
            overall_risk_level: "high".to_string(),
            max_position_risk: 0.0,
            margin_requirement: 0.0,
            risk_reward_ratio: 0.0,
            volatility_rating: "high".to_string(),
            alerts: vec!["Agent disagreement".to_string()],
            passed: false,
        },
        timestamp: Utc::now(),
    };

    assert_eq!(decision.action, DecisionAction::Hold);
    assert!(!decision.risk_assessment.passed);
    assert_eq!(decision.position_size_percent, 0.0);
}

#[test]
fn contribution_weights_are_serializable_decision_evidence() {
    let contribution = AgentContribution {
        agent_name: "新闻分析师".to_string(),
        department: AgentDepartment::News,
        sentiment: AgentSentiment::Bearish,
        confidence: 0.61,
        contribution_weight: 0.25,
        credibility_score: 0.72,
    };

    let value = serde_json::to_value(&contribution).unwrap();
    assert_eq!(value["agent_name"], "新闻分析师");
    assert_eq!(value["contribution_weight"], 0.25);
}
