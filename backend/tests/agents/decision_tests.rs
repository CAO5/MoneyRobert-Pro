
use moneyrobert_rs::agents::debate::*;
use moneyrobert_rs::agents::models::*;
use chrono::Utc;
use uuid::Uuid;
use sqlx::PgPool;
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_market_snapshot(symbol: &amp;str, price: f64) -&gt; MarketSnapshot {
        MarketSnapshot {
            symbol: symbol.to_string(),
            current_price: price,
            open_24h: price * 0.98,
            high_24h: price * 1.05,
            low_24h: price * 0.95,
            close_24h: price * 1.01,
            volume_24h: 100000000.0,
            price_change_percent_24h: 1.0,
            funding_rate: Some(-0.0001),
            open_interest: Some(500000000.0),
            long_short_ratio: Some(1.2),
            rsi_14: Some(45.0),
            macd_signal: Some(-0.001),
            timestamp: Utc::now(),
        }
    }

    fn create_mock_analysis(
        agent_name: &amp;str,
        department: AgentDepartment,
        sentiment: AgentSentiment,
        confidence: f64,
    ) -&gt; AgentAnalysis {
        AgentAnalysis {
            agent_name: agent_name.to_string(),
            department,
            sentiment,
            confidence,
            content: format!("{} analysis", agent_name),
            analysis_data: serde_json::json!({}),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_decision_action_variants() {
        let long = DecisionAction::Long;
        let short = DecisionAction::Short;
        let hold = DecisionAction::Hold;
        
        assert_ne!(long, short);
        assert_ne!(long, hold);
        assert_ne!(short, hold);
    }

    #[test]
    fn test_agent_sentiment_variants() {
        let sentiments = vec![
            AgentSentiment::Bullish,
            AgentSentiment::Bearish,
            AgentSentiment::Neutral,
            AgentSentiment::Cautious,
        ];
        
        assert_eq!(sentiments.len(), 4);
    }

    #[test]
    fn test_fund_manager_decision_creation() {
        let decision = FundManagerDecision {
            session_id: Uuid::new_v4(),
            action: DecisionAction::Long,
            symbol: "DOGE-USDT".to_string(),
            confidence: 0.75,
            position_size_percent: 10.0,
            leverage: 3,
            stop_loss_percent: Some(0.20),
            take_profit_percent: Some(0.25),
            reasoning: "Strong bullish signal".to_string(),
            agent_contributions: Vec::new(),
            risk_assessment: RiskAssessment {
                overall_risk_level: "medium".to_string(),
                max_position_risk: 10.0,
                margin_requirement: 3.33,
                risk_reward_ratio: 2.0,
                volatility_rating: "normal".to_string(),
                alerts: Vec::new(),
            },
            timestamp: Utc::now(),
        };
        
        assert_eq!(decision.action, DecisionAction::Long);
        assert_eq!(decision.symbol, "DOGE-USDT");
        assert_eq!(decision.confidence, 0.75);
    }

    #[test]
    fn test_debate_session_creation() {
        let session = DebateSession {
            id: Uuid::new_v4(),
            config_id: Some(Uuid::new_v4()),
            user_id: Some(123),
            symbol: "DOGE-USDT".to_string(),
            status: DebateStatus::InProgress,
            messages: Vec::new(),
            final_decision: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        assert_eq!(session.symbol, "DOGE-USDT");
        assert_eq!(session.status, DebateStatus::InProgress);
    }

    #[test]
    fn test_debate_message_creation() {
        let message = DebateMessage {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            agent_name: "K线形态分析师".to_string(),
            agent_department: AgentDepartment::Technical,
            role: "形态识别专家".to_string(),
            content: "Bullish signal detected".to_string(),
            analysis_data: serde_json::json!({}),
            confidence: 0.75,
            sentiment: Some(AgentSentiment::Bullish),
            message_order: 1,
            created_at: Utc::now(),
        };
        
        assert_eq!(message.agent_name, "K线形态分析师");
        assert_eq!(message.agent_department, AgentDepartment::Technical);
    }

    #[test]
    fn test_agent_contribution_creation() {
        let contribution = AgentContribution {
            agent_name: "K线形态分析师".to_string(),
            department: AgentDepartment::Technical,
            sentiment: AgentSentiment::Bullish,
            confidence: 0.75,
            contribution_weight: 0.25,
            credibility_score: 0.72,
        };
        
        assert_eq!(contribution.agent_name, "K线形态分析师");
        assert!(contribution.contribution_weight &gt; 0.0);
    }

    #[test]
    fn test_risk_assessment_creation() {
        let risk = RiskAssessment {
            overall_risk_level: "medium".to_string(),
            max_position_risk: 10.0,
            margin_requirement: 3.33,
            risk_reward_ratio: 2.0,
            volatility_rating: "normal".to_string(),
            alerts: vec!["Low volatility".to_string()],
        };
        
        assert_eq!(risk.overall_risk_level, "medium");
        assert_eq!(risk.risk_reward_ratio, 2.0);
    }
}
