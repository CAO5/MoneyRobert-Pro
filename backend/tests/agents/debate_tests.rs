use chrono::Utc;
use moneyrobert_rs::agents::models::*;
use uuid::Uuid;

#[test]
fn debate_session_can_hold_messages_and_final_decision() {
    let session_id = Uuid::new_v4();
    let message = DebateMessage {
        id: Uuid::new_v4(),
        session_id,
        agent_name: "技术指标分析师".to_string(),
        agent_department: AgentDepartment::Technical,
        role: "indicator".to_string(),
        content: "bullish divergence".to_string(),
        analysis_data: serde_json::json!({"rsi": 28}),
        confidence: 0.7,
        sentiment: Some(AgentSentiment::Bullish),
        message_order: 1,
        created_at: Utc::now(),
    };

    let decision = FundManagerDecision {
        session_id,
        action: DecisionAction::Long,
        symbol: "BTC-USDT".to_string(),
        confidence: 0.68,
        position_size_percent: 3.0,
        leverage: 1,
        stop_loss_percent: Some(0.02),
        take_profit_percent: Some(0.04),
        reasoning: "positive expected value".to_string(),
        agent_contributions: vec![AgentContribution {
            agent_name: message.agent_name.clone(),
            department: AgentDepartment::Technical,
            sentiment: AgentSentiment::Bullish,
            confidence: 0.7,
            contribution_weight: 0.4,
            credibility_score: 0.8,
        }],
        risk_assessment: RiskAssessment {
            overall_risk_level: "medium".to_string(),
            max_position_risk: 1.0,
            margin_requirement: 3.0,
            risk_reward_ratio: 2.0,
            volatility_rating: "normal".to_string(),
            alerts: vec![],
            passed: true,
        },
        timestamp: Utc::now(),
    };

    let session = DebateSession {
        id: session_id,
        config_id: None,
        user_id: Some(1),
        symbol: "BTC-USDT".to_string(),
        status: DebateStatus::Completed,
        messages: vec![message],
        final_decision: Some(decision),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.final_decision.unwrap().action, DecisionAction::Long);
}

#[test]
fn agent_analysis_preserves_department_sentiment_and_payload() {
    let analysis = AgentAnalysis {
        agent_name: "资金费率分析师".to_string(),
        department: AgentDepartment::Capital,
        sentiment: AgentSentiment::Cautious,
        confidence: 0.55,
        content: "funding is crowded".to_string(),
        analysis_data: serde_json::json!({"funding_rate": 0.001}),
        timestamp: Utc::now(),
    };

    assert_eq!(analysis.department, AgentDepartment::Capital);
    assert_eq!(analysis.sentiment, AgentSentiment::Cautious);
    assert_eq!(analysis.analysis_data["funding_rate"], 0.001);
}
