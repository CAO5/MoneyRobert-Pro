use crate::agents::models::*;
use crate::agents::errors::*;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundManagerConfig {
    pub max_position_size_percent: f64,
    pub max_leverage: i32,
    pub max_single_trade_loss_percent: f64,
    pub max_daily_loss_percent: f64,
    pub min_confidence_threshold: f64,
    pub risk_reward_ratio_min: f64,
    pub department_weights: HashMap<AgentDepartment, f64>,
}

impl Default for FundManagerConfig {
    fn default() -> Self {
        let mut department_weights = HashMap::new();
        department_weights.insert(AgentDepartment::Technical, 0.35);
        department_weights.insert(AgentDepartment::Capital, 0.35);
        department_weights.insert(AgentDepartment::News, 0.30);

        Self {
            max_position_size_percent: 10.0,
            max_leverage: 5,
            max_single_trade_loss_percent: 2.0,
            max_daily_loss_percent: 3.0,
            min_confidence_threshold: 0.55,
            risk_reward_ratio_min: 1.5,
            department_weights,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredibilityWeights {
    pub agent_weights: HashMap<String, f64>,
    pub department_weights: HashMap<AgentDepartment, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub overall_risk_level: String,
    pub max_position_risk: f64,
    pub margin_requirement: f64,
    pub risk_reward_ratio: f64,
    pub volatility_rating: String,
    pub alerts: Vec<String>,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionParameters {
    pub position_size_percent: f64,
    pub leverage: i32,
    pub stop_loss_percent: f64,
    pub take_profit_percent: Vec<f64>,
    pub entry_price_range: (f64, f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionMemoryEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub action: DecisionAction,
    pub confidence: f64,
    pub position_size_percent: f64,
    pub market_context: serde_json::Value,
    pub actual_outcome: Option<serde_json::Value>,
    pub reflection: Option<String>,
    pub success: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct FundManagerAgent {
    config: FundManagerConfig,
    credibility_weights: CredibilityWeights,
    decision_history: Vec<DecisionMemoryEntry>,
}

impl FundManagerAgent {
    pub fn new(config: FundManagerConfig) -> Self {
        let department_weights = config.department_weights.clone();
        Self {
            config,
            credibility_weights: CredibilityWeights {
                agent_weights: HashMap::new(),
                department_weights,
            },
            decision_history: Vec::new(),
        }
    }

    pub fn with_decision_history(mut self, history: Vec<DecisionMemoryEntry>) -> Self {
        self.decision_history = history;
        self
    }

    pub async fn make_decision(
        &self,
        session_id: Uuid,
        symbol: &str,
        agent_analyses: &[AgentAnalysis],
        market_snapshot: &MarketSnapshot,
        portfolio_context: &PortfolioContext,
    ) -> AgentResult<FundManagerDecision> {
        info!("FundManager making decision for {}", symbol);

        let credibility_weights = self.calculate_credibility_weights(agent_analyses)?;
        let weighted_sentiment = self.calculate_weighted_sentiment(agent_analyses, &credibility_weights)?;
        let agent_contributions = self.calculate_agent_contributions(agent_analyses, &credibility_weights)?;

        let (action, confidence) = self.determine_action(&weighted_sentiment, agent_analyses)?;

        let risk_assessment = self.assess_risk(
            &action,
            market_snapshot,
            portfolio_context,
            agent_analyses,
        )?;

        if !risk_assessment.passed {
            warn!("Risk assessment failed, vetoing decision. Alerts: {:?}", risk_assessment.alerts);
            return Ok(self.create_hold_decision(session_id, symbol, risk_assessment, agent_contributions));
        }

        let position_params = self.calculate_position_parameters(
            &action,
            confidence,
            market_snapshot,
            &risk_assessment,
        )?;

        let reasoning = self.generate_reasoning(
            &action,
            confidence,
            agent_analyses,
            &weighted_sentiment,
            &risk_assessment,
            &position_params,
        );

        let historical_reference = self.find_similar_decisions(symbol, &action, market_snapshot);

        Ok(FundManagerDecision {
            session_id,
            action,
            symbol: symbol.to_string(),
            confidence,
            position_size_percent: position_params.position_size_percent,
            leverage: position_params.leverage,
            stop_loss_percent: Some(position_params.stop_loss_percent),
            take_profit_percent: Some(position_params.take_profit_percent),
            reasoning,
            agent_contributions,
            risk_assessment,
            timestamp: Utc::now(),
        })
    }

    fn calculate_credibility_weights(&self, agent_analyses: &[AgentAnalysis]) -> AgentResult<CredibilityWeights> {
        let mut agent_weights = HashMap::new();

        for analysis in agent_analyses {
            let base_weight = self.credibility_weights.agent_weights
                .get(&analysis.agent_name)
                .copied()
                .unwrap_or(0.5);

            let calibrated_weight = base_weight * analysis.confidence;
            agent_weights.insert(analysis.agent_name.clone(), calibrated_weight);
        }

        let total_weight: f64 = agent_weights.values().sum();
        if total_weight > 0.0 {
            for weight in agent_weights.values_mut() {
                *weight /= total_weight;
            }
        }

        Ok(CredibilityWeights {
            agent_weights,
            department_weights: self.credibility_weights.department_weights.clone(),
        })
    }

    fn calculate_weighted_sentiment(
        &self,
        agent_analyses: &[AgentAnalysis],
        weights: &CredibilityWeights,
    ) -> AgentResult<WeightedSentiment> {
        let mut department_sentiments: HashMap<AgentDepartment, f64> = HashMap::new();
        let mut department_confidence: HashMap<AgentDepartment, f64> = HashMap::new();

        for analysis in agent_analyses {
            let sentiment_score = match analysis.sentiment {
                AgentSentiment::Bullish => 1.0,
                AgentSentiment::Bearish => -1.0,
                AgentSentiment::Neutral => 0.0,
                AgentSentiment::Cautious => -0.3,
            };

            let agent_weight = weights.agent_weights.get(&analysis.agent_name).copied().unwrap_or(0.0);

            *department_sentiments.entry(analysis.department.clone()).or_insert(0.0) 
                += sentiment_score * agent_weight;
            *department_confidence.entry(analysis.department.clone()).or_insert(0.0) 
                += analysis.confidence * agent_weight;
        }

        let mut overall_score = 0.0;
        let mut total_department_weight = 0.0;

        for (dept, &weight) in &weights.department_weights {
            if let Some(&sentiment) = department_sentiments.get(dept) {
                overall_score += sentiment * weight;
                total_department_weight += weight;
            }
        }

        if total_department_weight > 0.0 {
            overall_score /= total_department_weight;
        }

        Ok(WeightedSentiment {
            overall_score: overall_score.clamp(-1.0, 1.0),
            department_sentiments,
            department_confidence,
        })
    }

    fn calculate_agent_contributions(
        &self,
        agent_analyses: &[AgentAnalysis],
        weights: &CredibilityWeights,
    ) -> AgentResult<Vec<AgentContribution>> {
        let mut contributions = Vec::new();

        for analysis in agent_analyses {
            let contribution_weight = weights.agent_weights.get(&analysis.agent_name).copied().unwrap_or(0.0);
            let credibility_score = self.get_agent_credibility(&analysis.agent_name);

            contributions.push(AgentContribution {
                agent_name: analysis.agent_name.clone(),
                department: analysis.department.clone(),
                sentiment: analysis.sentiment.clone(),
                confidence: analysis.confidence,
                contribution_weight,
                credibility_score,
            });
        }

        contributions.sort_by(|a, b| b.contribution_weight.partial_cmp(&a.contribution_weight).unwrap());

        Ok(contributions)
    }

    fn determine_action(
        &self,
        weighted_sentiment: &WeightedSentiment,
        _agent_analyses: &[AgentAnalysis],
    ) -> AgentResult<(DecisionAction, f64)> {
        let score = weighted_sentiment.overall_score;
        let confidence = score.abs().clamp(0.0, 1.0);

        let action = if confidence < self.config.min_confidence_threshold {
            DecisionAction::Hold
        } else if score > 0.0 {
            DecisionAction::Long
        } else {
            DecisionAction::Short
        };

        Ok((action, confidence))
    }

    fn assess_risk(
        &self,
        action: &DecisionAction,
        market_snapshot: &MarketSnapshot,
        portfolio_context: &PortfolioContext,
        _agent_analyses: &[AgentAnalysis],
    ) -> AgentResult<RiskAssessment> {
        let mut alerts = Vec::new();
        let mut passed = true;

        let volatility_rating = self.calculate_volatility_rating(market_snapshot);
        let risk_reward_ratio = self.estimate_risk_reward_ratio(action, market_snapshot);

        if risk_reward_ratio < self.config.risk_reward_ratio_min {
            alerts.push(format!("Risk-reward ratio ({:.2}) below minimum ({:.2})", 
                risk_reward_ratio, self.config.risk_reward_ratio_min));
            passed = false;
        }

        if portfolio_context.daily_loss_percent >= self.config.max_daily_loss_percent {
            alerts.push(format!("Daily loss limit reached: {:.2}%", portfolio_context.daily_loss_percent));
            passed = false;
        }

        if portfolio_context.consecutive_stop_losses >= 3 {
            alerts.push(format!("Consecutive stop losses: {}", portfolio_context.consecutive_stop_losses));
            passed = false;
        }

        let volatility_risk = match volatility_rating.as_str() {
            "extreme" => {
                alerts.push("Extreme market volatility detected".to_string());
                passed = false;
                0.8
            }
            "high" => {
                alerts.push("High market volatility".to_string());
                0.5
            }
            _ => 0.2,
        };

        let max_position_risk = self.config.max_position_size_percent * (1.0 - volatility_risk);
        let margin_requirement = max_position_risk * 0.1;

        let overall_risk_level = if !passed {
            "critical".to_string()
        } else if alerts.len() >= 2 {
            "high".to_string()
        } else if alerts.len() == 1 {
            "medium".to_string()
        } else {
            "low".to_string()
        };

        Ok(RiskAssessment {
            overall_risk_level,
            max_position_risk,
            margin_requirement,
            risk_reward_ratio,
            volatility_rating,
            alerts,
            passed,
        })
    }

    fn calculate_position_parameters(
        &self,
        action: &DecisionAction,
        confidence: f64,
        market_snapshot: &MarketSnapshot,
        risk_assessment: &RiskAssessment,
    ) -> AgentResult<PositionParameters> {
        match action {
            DecisionAction::Hold => {
                Ok(PositionParameters {
                    position_size_percent: 0.0,
                    leverage: 1,
                    stop_loss_percent: 0.0,
                    take_profit_percent: vec![],
                    entry_price_range: (market_snapshot.current_price, market_snapshot.current_price),
                })
            }
            _ => {
                let base_position = self.config.max_position_size_percent * confidence;
                let volatility_adjustment = match risk_assessment.volatility_rating.as_str() {
                    "extreme" => 0.3,
                    "high" => 0.5,
                    "medium" => 0.75,
                    _ => 1.0,
                };
                let position_size_percent = (base_position * volatility_adjustment)
                    .min(self.config.max_position_size_percent);

                let leverage = if confidence > 0.8 {
                    self.config.max_leverage
                } else if confidence > 0.7 {
                    (self.config.max_leverage as f64 * 0.8) as i32
                } else if confidence > 0.6 {
                    (self.config.max_leverage as f64 * 0.6) as i32
                } else {
                    (self.config.max_leverage as f64 * 0.4) as i32
                }.max(1);

                let stop_loss_percent = match risk_assessment.volatility_rating.as_str() {
                    "extreme" => 5.0,
                    "high" => 3.5,
                    "medium" => 2.5,
                    _ => 2.0,
                }.min(self.config.max_single_trade_loss_percent);

                let take_profit_percent = vec![
                    stop_loss_percent * risk_assessment.risk_reward_ratio,
                    stop_loss_percent * risk_assessment.risk_reward_ratio * 1.5,
                    stop_loss_percent * risk_assessment.risk_reward_ratio * 2.0,
                ];

                let current_price = market_snapshot.current_price;
                let price_range = current_price * 0.005;
                let entry_price_range = (
                    current_price - price_range,
                    current_price + price_range,
                );

                Ok(PositionParameters {
                    position_size_percent,
                    leverage,
                    stop_loss_percent,
                    take_profit_percent,
                    entry_price_range,
                })
            }
        }
    }

    fn generate_reasoning(
        &self,
        action: &DecisionAction,
        confidence: f64,
        _agent_analyses: &[AgentAnalysis],
        weighted_sentiment: &WeightedSentiment,
        risk_assessment: &RiskAssessment,
        position_params: &PositionParameters,
    ) -> String {
        let action_str = match action {
            DecisionAction::Long => "LONG",
            DecisionAction::Short => "SHORT",
            DecisionAction::Hold => "HOLD",
        };

        let mut reasoning = format!(
            "Decision: {action_str} (confidence: {:.2}%)\n",
            confidence * 100.0
        );

        reasoning.push_str(&format!(
            "Weighted sentiment score: {:.3}\n",
            weighted_sentiment.overall_score
        ));

        reasoning.push_str("\nDepartment contributions:\n");
        for (dept, &sentiment) in &weighted_sentiment.department_sentiments {
            let dept_name = format!("{:?}", dept);
            let sentiment_label = if sentiment > 0.3 { "BULLISH" } 
                else if sentiment < -0.3 { "BEARISH" } 
                else { "NEUTRAL" };
            reasoning.push_str(&format!(
                "  - {}: {} (score: {:.2})\n",
                dept_name, sentiment_label, sentiment
            ));
        }

        reasoning.push_str("\nRisk assessment:\n");
        reasoning.push_str(&format!(
            "  - Risk level: {}\n",
            risk_assessment.overall_risk_level
        ));
        reasoning.push_str(&format!(
            "  - Risk-reward ratio: {:.2}\n",
            risk_assessment.risk_reward_ratio
        ));
        reasoning.push_str(&format!(
            "  - Volatility: {}\n",
            risk_assessment.volatility_rating
        ));

        if !risk_assessment.alerts.is_empty() {
            reasoning.push_str("\nAlerts:\n");
            for alert in &risk_assessment.alerts {
                reasoning.push_str(&format!("  - {}\n", alert));
            }
        }

        reasoning.push_str("\nPosition parameters:\n");
        reasoning.push_str(&format!(
            "  - Position size: {:.2}%\n",
            position_params.position_size_percent
        ));
        reasoning.push_str(&format!(
            "  - Leverage: {}x\n",
            position_params.leverage
        ));
        reasoning.push_str(&format!(
            "  - Stop loss: {:.2}%\n",
            position_params.stop_loss_percent
        ));
        reasoning.push_str(&format!(
            "  - Take profit targets: {:?}%\n",
            position_params.take_profit_percent
        ));

        reasoning
    }

    fn find_similar_decisions(
        &self,
        symbol: &str,
        action: &DecisionAction,
        _market_snapshot: &MarketSnapshot,
    ) -> Option<String> {
        let similar: Vec<_> = self.decision_history
            .iter()
            .filter(|d| d.symbol == symbol && &d.action == action)
            .take(5)
            .collect();

        if similar.is_empty() {
            return None;
        }

        let success_count = similar.iter().filter(|d| d.success.unwrap_or(false)).count();
        let avg_confidence: f64 = similar.iter().map(|d| d.confidence).sum::<f64>() / similar.len() as f64;

        Some(format!(
            "Historical reference: {} similar decisions, {:.1}% success rate, avg confidence {:.1}%",
            similar.len(),
            (success_count as f64 / similar.len() as f64) * 100.0,
            avg_confidence * 100.0
        ))
    }

    fn create_hold_decision(
        &self,
        session_id: Uuid,
        symbol: &str,
        risk_assessment: RiskAssessment,
        agent_contributions: Vec<AgentContribution>,
    ) -> FundManagerDecision {
        FundManagerDecision {
            session_id,
            action: DecisionAction::Hold,
            symbol: symbol.to_string(),
            confidence: 1.0,
            position_size_percent: 0.0,
            leverage: 1,
            stop_loss_percent: None,
            take_profit_percent: None,
            reasoning: format!(
                "Decision: HOLD (risk veto)\nReason: Risk assessment failed. Alerts: {:?}",
                risk_assessment.alerts
            ),
            agent_contributions,
            risk_assessment,
            timestamp: Utc::now(),
        }
    }

    fn calculate_volatility_rating(&self, market_snapshot: &MarketSnapshot) -> String {
        let price_change_abs = market_snapshot.price_change_percent_24h.abs();

        if price_change_abs > 15.0 {
            "extreme".to_string()
        } else if price_change_abs > 8.0 {
            "high".to_string()
        } else if price_change_abs > 4.0 {
            "medium".to_string()
        } else {
            "low".to_string()
        }
    }

    fn estimate_risk_reward_ratio(&self, action: &DecisionAction, market_snapshot: &MarketSnapshot) -> f64 {
        let volatility = market_snapshot.price_change_percent_24h.abs();
        let base_ratio = 2.0;

        match action {
            DecisionAction::Hold => 0.0,
            _ => {
                if volatility > 10.0 {
                    base_ratio * 0.7
                } else if volatility > 5.0 {
                    base_ratio * 0.85
                } else {
                    base_ratio
                }
            }
        }
    }

    fn get_agent_credibility(&self, agent_name: &str) -> f64 {
        self.credibility_weights.agent_weights.get(agent_name).copied().unwrap_or(0.5)
    }

    pub fn record_decision_outcome(
        &mut self,
        decision: &FundManagerDecision,
        actual_outcome: serde_json::Value,
        success: bool,
        reflection: Option<String>,
    ) {
        let entry = DecisionMemoryEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            symbol: decision.symbol.clone(),
            action: decision.action.clone(),
            confidence: decision.confidence,
            position_size_percent: decision.position_size_percent,
            market_context: serde_json::json!({}),
            actual_outcome: Some(actual_outcome),
            reflection,
            success: Some(success),
        };

        self.decision_history.push(entry);
        self.update_credibility_weights();
    }

    fn update_credibility_weights(&mut self) {
        let mut agent_performance: HashMap<String, (i32, i32)> = HashMap::new();

        for entry in &self.decision_history {
            if let Some(success) = entry.success {
                for contribution in &entry.agent_contributions {
                    let (wins, total) = agent_performance.entry(contribution.agent_name.clone())
                        .or_insert((0, 0));
                    *total += 1;
                    if success {
                        *wins += 1;
                    }
                }
            }
        }

        for (agent_name, (wins, total)) in agent_performance {
            let win_rate = if total > 0 { wins as f64 / total as f64 } else { 0.5 };
            let smoothed_rate = (win_rate * 0.7) + (0.5 * 0.3);
            self.credibility_weights.agent_weights.insert(agent_name, smoothed_rate);
        }
    }

    pub fn reflect_on_decisions(&self, lookback_days: i64) -> AgentResult<Vec<Reflection>> {
        let cutoff = Utc::now() - Duration::days(lookback_days);
        let relevant_decisions: Vec<_> = self.decision_history
            .iter()
            .filter(|d| d.timestamp >= cutoff)
            .collect();

        if relevant_decisions.is_empty() {
            return Ok(vec![]);
        }

        let mut reflections = Vec::new();

        let total_decisions = relevant_decisions.len();
        let successful_decisions = relevant_decisions.iter()
            .filter(|d| d.success.unwrap_or(false))
            .count();
        let success_rate = successful_decisions as f64 / total_decisions as f64;

        reflections.push(Reflection {
            category: "overall_performance".to_string(),
            insight: format!(
                "Overall success rate: {:.1}% ({} out of {})",
                success_rate * 100.0,
                successful_decisions,
                total_decisions
            ),
            recommendation: if success_rate < 0.5 {
                "Consider reducing position sizes and increasing confidence thresholds".to_string()
            } else if success_rate > 0.7 {
                "Performance is strong; consider maintaining current strategy".to_string()
            } else {
                "Performance is moderate; review risk management parameters".to_string()
            },
        });

        let mut action_stats: HashMap<DecisionAction, (i32, i32)> = HashMap::new();
        for decision in &relevant_decisions {
            let (wins, total) = action_stats.entry(decision.action.clone())
                .or_insert((0, 0));
            *total += 1;
            if decision.success.unwrap_or(false) {
                *wins += 1;
            }
        }

        for (action, (wins, total)) in action_stats {
            let rate = if total > 0 { wins as f64 / total as f64 } else { 0.0 };
            reflections.push(Reflection {
                category: format!("{:?}_performance", action),
                insight: format!(
                    "{:?} success rate: {:.1}% ({} out of {})",
                    action, rate * 100.0, wins, total
                ),
                recommendation: if rate < 0.4 {
                    format!("Consider being more cautious with {:?} positions", action)
                } else {
                    format!("{:?} positions performing well", action)
                },
            });
        }

        Ok(reflections)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedSentiment {
    pub overall_score: f64,
    pub department_sentiments: HashMap<AgentDepartment, f64>,
    pub department_confidence: HashMap<AgentDepartment, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioContext {
    pub current_balance: f64,
    pub daily_pnl: f64,
    pub daily_loss_percent: f64,
    pub weekly_loss_percent: f64,
    pub consecutive_stop_losses: i32,
    pub open_positions: Vec<OpenPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPosition {
    pub symbol: String,
    pub direction: DecisionAction,
    pub size_usd: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reflection {
    pub category: String,
    pub insight: String,
    pub recommendation: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_agent_analysis(name: &str, dept: AgentDepartment, sentiment: AgentSentiment, confidence: f64) -> AgentAnalysis {
        AgentAnalysis {
            agent_name: name.to_string(),
            department: dept,
            sentiment,
            confidence,
            content: "Test analysis".to_string(),
            analysis_data: serde_json::json!({}),
            timestamp: Utc::now(),
        }
    }

    fn create_test_market_snapshot() -> MarketSnapshot {
        MarketSnapshot {
            symbol: "DOGE-USDT-SWAP".to_string(),
            current_price: 0.2,
            open_24h: 0.195,
            high_24h: 0.21,
            low_24h: 0.185,
            close_24h: 0.2,
            volume_24h: 1000000000.0,
            price_change_percent_24h: 2.56,
            funding_rate: Some(0.0001),
            open_interest: Some(500000000.0),
            long_short_ratio: Some(1.2),
            rsi_14: Some(55.0),
            macd_signal: Some(0.001),
            timestamp: Utc::now(),
        }
    }

    fn create_test_portfolio_context() -> PortfolioContext {
        PortfolioContext {
            current_balance: 10000.0,
            daily_pnl: 150.0,
            daily_loss_percent: 0.5,
            weekly_loss_percent: 1.2,
            consecutive_stop_losses: 0,
            open_positions: vec![],
        }
    }

    #[test]
    fn test_fund_manager_default_config() {
        let config = FundManagerConfig::default();
        assert_eq!(config.max_position_size_percent, 10.0);
        assert_eq!(config.max_leverage, 5);
    }

    #[test]
    fn test_calculate_credibility_weights() {
        let fm = FundManagerAgent::new(FundManagerConfig::default());
        let analyses = vec![
            create_test_agent_analysis("agent1", AgentDepartment::Technical, AgentSentiment::Bullish, 0.8),
            create_test_agent_analysis("agent2", AgentDepartment::Capital, AgentSentiment::Bearish, 0.6),
        ];

        let weights = fm.calculate_credibility_weights(&analyses).unwrap();
        assert_eq!(weights.agent_weights.len(), 2);
    }

    #[test]
    fn test_weighted_sentiment_calculation() {
        let fm = FundManagerAgent::new(FundManagerConfig::default());
        let analyses = vec![
            create_test_agent_analysis("agent1", AgentDepartment::Technical, AgentSentiment::Bullish, 0.8),
            create_test_agent_analysis("agent2", AgentDepartment::Capital, AgentSentiment::Bullish, 0.7),
            create_test_agent_analysis("agent3", AgentDepartment::News, AgentSentiment::Neutral, 0.5),
        ];

        let weights = fm.calculate_credibility_weights(&analyses).unwrap();
        let sentiment = fm.calculate_weighted_sentiment(&analyses, &weights).unwrap();

        assert!(sentiment.overall_score > 0.0);
    }

    #[test]
    fn test_risk_assessment() {
        let fm = FundManagerAgent::new(FundManagerConfig::default());
        let market = create_test_market_snapshot();
        let portfolio = create_test_portfolio_context();
        let analyses = vec![];

        let risk = fm.assess_risk(&DecisionAction::Long, &market, &portfolio, &analyses).unwrap();
        assert!(risk.passed);
    }

    #[test]
    fn test_position_parameters() {
        let fm = FundManagerAgent::new(FundManagerConfig::default());
        let market = create_test_market_snapshot();
        let mut risk_assessment = RiskAssessment {
            overall_risk_level: "low".to_string(),
            max_position_risk: 10.0,
            margin_requirement: 1.0,
            risk_reward_ratio: 2.0,
            volatility_rating: "low".to_string(),
            alerts: vec![],
            passed: true,
        };

        let params = fm.calculate_position_parameters(
            &DecisionAction::Long,
            0.75,
            &market,
            &risk_assessment,
        ).unwrap();

        assert!(params.position_size_percent > 0.0);
        assert!(params.leverage >= 1);
        assert!(params.stop_loss_percent > 0.0);
    }
}
