
use moneyrobert_rs::agents::debate::*;
use moneyrobert_rs::agents::models::*;
use moneyrobert_rs::agents::errors::AgentResult;
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

    #[tokio::test]
    async fn test_kline_analyst_creation() {
        let analyst = KlinePatternAnalyst::new();
        assert_eq!(analyst.agent_id(), "tech_kline_analyst");
        assert_eq!(analyst.name(), "K线形态分析师");
        assert_eq!(analyst.department(), AgentDepartment::Technical);
    }

    #[tokio::test]
    async fn test_kline_analyst_bullish_signal() {
        let analyst = KlinePatternAnalyst::new();
        let snapshot = create_mock_market_snapshot("DOGE-USDT", 0.25);
        let snapshot = MarketSnapshot {
            price_change_percent_24h: 3.0,
            ..snapshot
        };
        
        let context = AnalysisContext {
            symbol: "DOGE-USDT".to_string(),
            market_snapshot: snapshot,
            session_id: Uuid::new_v4(),
            historical_decisions: Vec::new(),
        };
        
        let pool = PgPool::connect("postgres://user:pass@localhost/test").await;
        let pool = match pool {
            Ok(p) =&gt; p,
            Err(_) =&gt; {
                return;
            }
        };
        
        let result = analyst.analyze(&amp;context, &amp;pool).await;
        assert!(result.is_ok());
        if let Ok(analysis) = result {
            assert_eq!(analysis.sentiment, AgentSentiment::Bullish);
        }
    }

    #[tokio::test]
    async fn test_technical_indicator_analyst_creation() {
        let analyst = TechnicalIndicatorAnalyst::new();
        assert_eq!(analyst.agent_id(), "tech_indicator_analyst");
        assert_eq!(analyst.name(), "技术指标分析师");
    }

    #[tokio::test]
    async fn test_technical_indicator_rsi_oversold() {
        let analyst = TechnicalIndicatorAnalyst::new();
        let snapshot = create_mock_market_snapshot("DOGE-USDT", 0.22);
        let snapshot = MarketSnapshot {
            rsi_14: Some(25.0),
            ..snapshot
        };
        
        let context = AnalysisContext {
            symbol: "DOGE-USDT".to_string(),
            market_snapshot: snapshot,
            session_id: Uuid::new_v4(),
            historical_decisions: Vec::new(),
        };
        
        let pool = PgPool::connect("postgres://user:pass@localhost/test").await;
        let pool = match pool {
            Ok(p) =&gt; p,
            Err(_) =&gt; {
                return;
            }
        };
        
        let result = analyst.analyze(&amp;context, &amp;pool).await;
        assert!(result.is_ok());
        if let Ok(analysis) = result {
            assert_eq!(analysis.sentiment, AgentSentiment::Bullish);
        }
    }

    #[tokio::test]
    async fn test_technical_indicator_rsi_overbought() {
        let analyst = TechnicalIndicatorAnalyst::new();
        let snapshot = create_mock_market_snapshot("DOGE-USDT", 0.22);
        let snapshot = MarketSnapshot {
            rsi_14: Some(75.0),
            ..snapshot
        };
        
        let context = AnalysisContext {
            symbol: "DOGE-USDT".to_string(),
            market_snapshot: snapshot,
            session_id: Uuid::new_v4(),
            historical_decisions: Vec::new(),
        };
        
        let pool = PgPool::connect("postgres://user:pass@localhost/test").await;
        let pool = match pool {
            Ok(p) =&gt; p,
            Err(_) =&gt; {
                return;
            }
        };
        
        let result = analyst.analyze(&amp;context, &amp;pool).await;
        assert!(result.is_ok());
        if let Ok(analysis) = result {
            assert_eq!(analysis.sentiment, AgentSentiment::Bearish);
        }
    }

    #[tokio::test]
    async fn test_funding_rate_analyst_creation() {
        let analyst = FundingRateAnalyst::new();
        assert_eq!(analyst.agent_id(), "capital_funding_analyst");
        assert_eq!(analyst.name(), "资金费率分析师");
    }

    #[tokio::test]
    async fn test_funding_rate_bullish_signal() {
        let analyst = FundingRateAnalyst::new();
        let snapshot = create_mock_market_snapshot("DOGE-USDT", 0.22);
        let snapshot = MarketSnapshot {
            funding_rate: Some(-0.001),
            ..snapshot
        };
        
        let context = AnalysisContext {
            symbol: "DOGE-USDT".to_string(),
            market_snapshot: snapshot,
            session_id: Uuid::new_v4(),
            historical_decisions: Vec::new(),
        };
        
        let pool = PgPool::connect("postgres://user:pass@localhost/test").await;
        let pool = match pool {
            Ok(p) =&gt; p,
            Err(_) =&gt; {
                return;
            }
        };
        
        let result = analyst.analyze(&amp;context, &amp;pool).await;
        assert!(result.is_ok());
        if let Ok(analysis) = result {
            assert_eq!(analysis.sentiment, AgentSentiment::Bullish);
        }
    }

    #[test]
    fn test_agent_registry_creation() {
        let registry = AgentRegistry::new();
        let profiles = registry.get_agent_profiles();
        assert!(!profiles.is_empty());
    }

    #[test]
    fn test_agent_registry_get_by_department() {
        let registry = AgentRegistry::new();
        let tech_agents = registry.get_agents_by_department(AgentDepartment::Technical);
        assert!(!tech_agents.is_empty());
    }
}
