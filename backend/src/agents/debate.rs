
use crate::agents::errors::{AgentError, AgentResult};
use crate::agents::llm_client::LlmClient;
use crate::agents::models::*;
use chrono::Utc;
use futures::future::join_all;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

// ============================================
// Agent Trait Definition
// ============================================

#[async_trait::async_trait]
pub trait Agent: Send + Sync {
    fn agent_id(&self) -> &str;
    fn name(&self) -> &str;
    fn department(&self) -> AgentDepartment;
    fn role(&self) -> &str;
    fn reference_institution(&self) -> &str;
    fn personality_traits(&self) -> Vec<&str>;
    fn credibility_score(&self) -> f64;

    async fn analyze(
        &self,
        context: &AnalysisContext,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis>;

    async fn debate(
        &self,
        context: &AnalysisContext,
        opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis>;
}

// ============================================
// LLM Helper
// ============================================

async fn try_llm_analysis(
    agent: &dyn Agent,
    context: &AnalysisContext,
    llm_client: &Arc<LlmClient>,
) -> AgentResult<AgentAnalysis> {
    let market_data_json = serde_json::to_string(&context.market_snapshot)
        .map_err(|e| AgentError::AnalysisError(format!("Failed to serialize market snapshot: {}", e)))?;

    let personality = agent.personality_traits();
    let department = agent.department();
    let result = llm_client
        .analyze_as_agent(
            agent.name(),
            &department,
            agent.role(),
            &personality,
            &market_data_json,
            None,
        )
        .await?;

    Ok(AgentAnalysis {
        agent_name: result.agent_name.clone(),
        department: result.department.clone(),
        sentiment: result.to_agent_sentiment(),
        confidence: result.confidence,
        content: result.analysis,
        analysis_data: serde_json::json!({
            "key_factors": result.key_factors,
            "source": "llm",
        }),
        timestamp: Utc::now(),
    })
}

// ============================================
// Analysis Context
// ============================================

#[derive(Debug, Clone)]
pub struct AnalysisContext {
    pub symbol: String,
    pub market_snapshot: MarketSnapshot,
    pub session_id: Uuid,
    pub historical_decisions: Vec<FundManagerDecision>,
}

// ============================================
// Agent Implementations - Technical Department
// ============================================

pub struct KlinePatternAnalyst {
    credibility_score: f64,
}

impl KlinePatternAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.72,
        }
    }
}

#[async_trait::async_trait]
impl Agent for KlinePatternAnalyst {
    fn agent_id(&self) -> &str {
        "tech_kline_analyst"
    }

    fn name(&self) -> &str {
        "K线形态分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::Technical
    }

    fn role(&self) -> &str {
        "形态识别专家"
    }

    fn reference_institution(&self) -> &str {
        "Two Sigma / Jump Crypto"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["保守", "细节导向", "重视历史形态"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("K线形态分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        let snapshot = &context.market_snapshot;

        let (sentiment, confidence, content) = if snapshot.price_change_percent_24h > 2.0 {
            (
                AgentSentiment::Bullish,
                0.65,
                format!(
                    "当前价格 {:.4} 形成上涨趋势，24h涨幅 {:.2}%。关注阻力位 {:.4}。",
                    snapshot.current_price,
                    snapshot.price_change_percent_24h,
                    snapshot.high_24h
                ),
            )
        } else if snapshot.price_change_percent_24h < -2.0 {
            (
                AgentSentiment::Bearish,
                0.65,
                format!(
                    "当前价格 {:.4} 形成下跌趋势，24h跌幅 {:.2}%。关注支撑位 {:.4}。",
                    snapshot.current_price,
                    snapshot.price_change_percent_24h,
                    snapshot.low_24h
                ),
            )
        } else {
            (
                AgentSentiment::Neutral,
                0.55,
                format!(
                    "当前价格 {:.4} 处于横盘整理区间 [{:.4}, {:.4}]。等待明确突破信号。",
                    snapshot.current_price, snapshot.low_24h, snapshot.high_24h
                ),
            )
        };

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment,
            confidence,
            content,
            analysis_data: serde_json::json!({
                "current_price": snapshot.current_price,
                "high_24h": snapshot.high_24h,
                "low_24h": snapshot.low_24h,
                "price_change_24h": snapshot.price_change_percent_24h,
                "patterns": ["horizontal_range"],
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        let base_analysis = self.analyze(context, db_pool, llm_client).await?;

        let mut content = base_analysis.content.clone();
        content.push_str(&format!(
            " 回应对方观点({}): 虽然对方提出{}，但K线形态显示当前处于{}状态，需谨慎对待。",
            opponent_analysis.agent_name,
            match opponent_analysis.sentiment {
                AgentSentiment::Bullish => "看多",
                AgentSentiment::Bearish => "看空",
                AgentSentiment::Neutral => "中性",
                AgentSentiment::Cautious => "谨慎",
            },
            match base_analysis.sentiment {
                AgentSentiment::Bullish => "偏多",
                AgentSentiment::Bearish => "偏空",
                AgentSentiment::Neutral => "整理",
                AgentSentiment::Cautious => "谨慎",
            }
        ));

        Ok(AgentAnalysis {
            content,
            confidence: base_analysis.confidence * 0.9,
            ..base_analysis
        })
    }
}

pub struct TechnicalIndicatorAnalyst {
    credibility_score: f64,
}

impl TechnicalIndicatorAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.68,
        }
    }
}

#[async_trait::async_trait]
impl Agent for TechnicalIndicatorAnalyst {
    fn agent_id(&self) -> &str {
        "tech_indicator_analyst"
    }

    fn name(&self) -> &str {
        "技术指标分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::Technical
    }

    fn role(&self) -> &str {
        "指标计算专家"
    }

    fn reference_institution(&self) -> &str {
        "Two Sigma / Renaissance"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["量化", "数据驱动", "系统化"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("技术指标分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        let snapshot = &context.market_snapshot;

        let rsi = snapshot.rsi_14.unwrap_or(50.0);

        let (sentiment, confidence, content) = if rsi < 30.0 {
            (
                AgentSentiment::Bullish,
                0.70,
                format!("RSI指标 {:.1} 处于超卖区间，可能出现反弹信号。", rsi),
            )
        } else if rsi > 70.0 {
            (
                AgentSentiment::Bearish,
                0.70,
                format!("RSI指标 {:.1} 处于超买区间，可能出现回调信号。", rsi),
            )
        } else {
            (
                AgentSentiment::Neutral,
                0.55,
                format!("RSI指标 {:.1} 处于中性区间，等待更明确信号。", rsi),
            )
        };

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment,
            confidence,
            content,
            analysis_data: serde_json::json!({
                "rsi_14": rsi,
                "macd_signal": snapshot.macd_signal,
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

pub struct OnChainDataAnalyst {
    credibility_score: f64,
}

impl OnChainDataAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.65,
        }
    }
}

#[async_trait::async_trait]
impl Agent for OnChainDataAnalyst {
    fn agent_id(&self) -> &str {
        "tech_onchain_analyst"
    }

    fn name(&self) -> &str {
        "链上数据分析员"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::Technical
    }

    fn role(&self) -> &str {
        "链上监控专家"
    }

    fn reference_institution(&self) -> &str {
        "Glassnode / Nansen"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["前瞻", "关注资金流动", "鲸鱼行为分析"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("链上数据分析员 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        let snapshot = &context.market_snapshot;

        let open_interest = snapshot.open_interest.unwrap_or(0.0);

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.55,
            content: format!(
                "当前持仓量 {:.1}，成交量 {:.1}。链上数据暂未发现明显异常。",
                open_interest, snapshot.volume_24h
            ),
            analysis_data: serde_json::json!({
                "open_interest": open_interest,
                "volume_24h": snapshot.volume_24h,
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

pub struct QuantModelAnalyst {
    credibility_score: f64,
}

impl QuantModelAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.70,
        }
    }
}

#[async_trait::async_trait]
impl Agent for QuantModelAnalyst {
    fn agent_id(&self) -> &str {
        "tech_quant_analyst"
    }

    fn name(&self) -> &str {
        "量化模型分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::Technical
    }

    fn role(&self) -> &str {
        "策略研发专家"
    }

    fn reference_institution(&self) -> &str {
        "Renaissance Technologies"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["严谨", "回测驱动", "多因子分析"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("量化模型分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        let snapshot = &context.market_snapshot;

        let volatility = (snapshot.high_24h - snapshot.low_24h) / snapshot.open_24h;

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.58,
            content: format!(
                "24h波动率 {:.2}%。多因子模型未发现显著信号，建议等待更明确的市场结构。",
                volatility * 100.0
            ),
            analysis_data: serde_json::json!({
                "volatility": volatility,
                "factor_score": 0.0,
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

// ============================================
// Agent Implementations - Capital Department
// ============================================

pub struct FundingRateAnalyst {
    credibility_score: f64,
}

impl FundingRateAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.74,
        }
    }
}

#[async_trait::async_trait]
impl Agent for FundingRateAnalyst {
    fn agent_id(&self) -> &str {
        "capital_funding_analyst"
    }

    fn name(&self) -> &str {
        "资金费率分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::Capital
    }

    fn role(&self) -> &str {
        "费率解读专家"
    }

    fn reference_institution(&self) -> &str {
        "Citadel / OKX Research"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["敏感", "关注多空成本", "极端信号识别"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("资金费率分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        let snapshot = &context.market_snapshot;

        let funding_rate = snapshot.funding_rate.unwrap_or(0.0);

        let (sentiment, confidence, content) = if funding_rate < -0.001 {
            (
                AgentSentiment::Bullish,
                0.70,
                format!(
                    "资金费率 {:.4}% 极低（空头拥挤），可能出现逼空行情。",
                    funding_rate * 100.0
                ),
            )
        } else if funding_rate > 0.001 {
            (
                AgentSentiment::Bearish,
                0.65,
                format!(
                    "资金费率 {:.4}% 偏高，多头成本增加，但需结合趋势判断是否反转。",
                    funding_rate * 100.0
                ),
            )
        } else {
            (
                AgentSentiment::Neutral,
                0.55,
                format!(
                    "资金费率 {:.4}% 处于正常区间，多空成本相对平衡。",
                    funding_rate * 100.0
                ),
            )
        };

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment,
            confidence,
            content,
            analysis_data: serde_json::json!({
                "funding_rate": funding_rate,
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

pub struct PositionStructureAnalyst {
    credibility_score: f64,
}

impl PositionStructureAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.67,
        }
    }
}

#[async_trait::async_trait]
impl Agent for PositionStructureAnalyst {
    fn agent_id(&self) -> &str {
        "capital_position_analyst"
    }

    fn name(&self) -> &str {
        "持仓结构分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::Capital
    }

    fn role(&self) -> &str {
        "OI分析专家"
    }

    fn reference_institution(&self) -> &str {
        "Citadel / Grayscale"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["机构视角", "资金追踪", "趋势确认"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("持仓结构分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.55,
            content: "持仓结构分析暂未发现显著异常。".to_string(),
            analysis_data: serde_json::json!({
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

pub struct LongShortGameAnalyst {
    credibility_score: f64,
}

impl LongShortGameAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.66,
        }
    }
}

#[async_trait::async_trait]
impl Agent for LongShortGameAnalyst {
    fn agent_id(&self) -> &str {
        "capital_ls_analyst"
    }

    fn name(&self) -> &str {
        "多空博弈分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::Capital
    }

    fn role(&self) -> &str {
        "多空对比专家"
    }

    fn reference_institution(&self) -> &str {
        "OKX Research"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["博弈论", "散户/机构分歧", "爆仓分析"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("多空博弈分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        let snapshot = &context.market_snapshot;

        let ls_ratio = snapshot.long_short_ratio.unwrap_or(1.0);

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.55,
            content: format!("多空比例 {:.2}，市场相对平衡。", ls_ratio),
            analysis_data: serde_json::json!({
                "long_short_ratio": ls_ratio,
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

pub struct LiquidityAnalyst {
    credibility_score: f64,
}

impl LiquidityAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.63,
        }
    }
}

#[async_trait::async_trait]
impl Agent for LiquidityAnalyst {
    fn agent_id(&self) -> &str {
        "capital_liquidity_analyst"
    }

    fn name(&self) -> &str {
        "流动性分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::Capital
    }

    fn role(&self) -> &str {
        "市场深度专家"
    }

    fn reference_institution(&self) -> &str {
        "Jump Trading"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["风险厌恶", "滑点分析", "大单冲击评估"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("流动性分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        let snapshot = &context.market_snapshot;

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.55,
            content: format!("24h成交量 {:.1}，流动性正常。", snapshot.volume_24h),
            analysis_data: serde_json::json!({
                "volume_24h": snapshot.volume_24h,
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

// ============================================
// Agent Implementations - News Department
// ============================================

pub struct SentimentAnalyst {
    credibility_score: f64,
}

impl SentimentAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.69,
        }
    }
}

#[async_trait::async_trait]
impl Agent for SentimentAnalyst {
    fn agent_id(&self) -> &str {
        "news_sentiment_analyst"
    }

    fn name(&self) -> &str {
        "舆情情绪分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::News
    }

    fn role(&self) -> &str {
        "情绪量化专家"
    }

    fn reference_institution(&self) -> &str {
        "Bloomberg / The TIE"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["敏感", "反向思维", "极端情绪预警"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("舆情情绪分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.55,
            content: "舆情情绪暂未发现显著偏向。".to_string(),
            analysis_data: serde_json::json!({
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

pub struct MacroPolicyAnalyst {
    credibility_score: f64,
}

impl MacroPolicyAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.71,
        }
    }
}

#[async_trait::async_trait]
impl Agent for MacroPolicyAnalyst {
    fn agent_id(&self) -> &str {
        "news_macro_analyst"
    }

    fn name(&self) -> &str {
        "宏观政策分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::News
    }

    fn role(&self) -> &str {
        "政策解读专家"
    }

    fn reference_institution(&self) -> &str {
        "Bloomberg / Messari"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["宏观视野", "政策敏感", "长期视角"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("宏观政策分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.55,
            content: "宏观政策面暂未发现重大变化。".to_string(),
            analysis_data: serde_json::json!({
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

pub struct KOLWhaleMonitor {
    credibility_score: f64,
}

impl KOLWhaleMonitor {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.73,
        }
    }
}

#[async_trait::async_trait]
impl Agent for KOLWhaleMonitor {
    fn agent_id(&self) -> &str {
        "news_kol_monitor"
    }

    fn name(&self) -> &str {
        "KOL/鲸鱼监控员"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::News
    }

    fn role(&self) -> &str {
        "关键人物追踪专家"
    }

    fn reference_institution(&self) -> &str {
        "Ark Invest / Whale Alert"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["敏锐", "事件驱动", "影响力评估"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("KOL/鲸鱼监控员 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.55,
            content: "KOL和鲸鱼钱包暂未发现显著异动。".to_string(),
            analysis_data: serde_json::json!({
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

pub struct EventDrivenAnalyst {
    credibility_score: f64,
}

impl EventDrivenAnalyst {
    pub fn new() -> Self {
        Self {
            credibility_score: 0.68,
        }
    }
}

#[async_trait::async_trait]
impl Agent for EventDrivenAnalyst {
    fn agent_id(&self) -> &str {
        "news_event_analyst"
    }

    fn name(&self) -> &str {
        "事件驱动分析师"
    }

    fn department(&self) -> AgentDepartment {
        AgentDepartment::News
    }

    fn role(&self) -> &str {
        "事件影响评估专家"
    }

    fn reference_institution(&self) -> &str {
        "Messari / ChainNews"
    }

    fn personality_traits(&self) -> Vec<&str> {
        vec!["快速响应", "影响量化", "时间线分析"]
    }

    fn credibility_score(&self) -> f64 {
        self.credibility_score
    }

    async fn analyze(
        &self,
        context: &AnalysisContext,
        _db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        if let Some(ref client) = llm_client {
            if client.is_configured() {
                match try_llm_analysis(self, context, client).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        warn!("事件驱动分析师 LLM调用失败，fallback到硬编码规则: {}", e);
                    }
                }
            }
        }

        Ok(AgentAnalysis {
            agent_name: self.name().to_string(),
            department: self.department(),
            sentiment: AgentSentiment::Neutral,
            confidence: 0.55,
            content: "暂未发现重大驱动事件。".to_string(),
            analysis_data: serde_json::json!({
                "source": "hardcoded",
            }),
            timestamp: Utc::now(),
        })
    }

    async fn debate(
        &self,
        context: &AnalysisContext,
        _opponent_analysis: &AgentAnalysis,
        db_pool: &PgPool,
        llm_client: Option<Arc<LlmClient>>,
    ) -> AgentResult<AgentAnalysis> {
        self.analyze(context, db_pool, llm_client).await
    }
}

// ============================================
// Debate Engine
// ============================================

pub struct DebateEngine {
    db_pool: Arc<PgPool>,
    llm_client: Option<Arc<LlmClient>>,
    tech_agents: Vec<Arc<dyn Agent>>,
    capital_agents: Vec<Arc<dyn Agent>>,
    news_agents: Vec<Arc<dyn Agent>>,
}

impl DebateEngine {
    pub fn new(db_pool: Arc<PgPool>, llm_client: Option<Arc<LlmClient>>) -> Self {
        let tech_agents: Vec<Arc<dyn Agent>> = vec![
            Arc::new(KlinePatternAnalyst::new()),
            Arc::new(TechnicalIndicatorAnalyst::new()),
            Arc::new(OnChainDataAnalyst::new()),
            Arc::new(QuantModelAnalyst::new()),
        ];

        let capital_agents: Vec<Arc<dyn Agent>> = vec![
            Arc::new(FundingRateAnalyst::new()),
            Arc::new(PositionStructureAnalyst::new()),
            Arc::new(LongShortGameAnalyst::new()),
            Arc::new(LiquidityAnalyst::new()),
        ];

        let news_agents: Vec<Arc<dyn Agent>> = vec![
            Arc::new(SentimentAnalyst::new()),
            Arc::new(MacroPolicyAnalyst::new()),
            Arc::new(KOLWhaleMonitor::new()),
            Arc::new(EventDrivenAnalyst::new()),
        ];

        Self {
            db_pool,
            llm_client,
            tech_agents,
            capital_agents,
            news_agents,
        }
    }

    pub async fn run_debate(
        &self,
        symbol: &str,
        market_snapshot: MarketSnapshot,
        config_id: Option<Uuid>,
        user_id: Option<i64>,
    ) -> AgentResult<DebateSession> {
        let session_id = Uuid::new_v4();
        let mut messages = Vec::new();
        let mut message_order = 0;

        // Load historical decisions from DB for context (dynamic memory integration)
        let historical_decisions = self.load_historical_decisions(symbol).await?;

        let context = AnalysisContext {
            symbol: symbol.to_string(),
            market_snapshot: market_snapshot.clone(),
            session_id,
            historical_decisions: historical_decisions.clone(),
        };

        let mut session = DebateSession {
            id: session_id,
            config_id,
            user_id,
            symbol: symbol.to_string(),
            status: DebateStatus::InProgress,
            messages: Vec::new(),
            final_decision: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Phase 1: Independent analysis (each agent analyzes independently)
        let tech_analyses = self.run_independent_analysis(&self.tech_agents, &context, &mut messages, &mut message_order).await?;
        let capital_analyses = self.run_independent_analysis(&self.capital_agents, &context, &mut messages, &mut message_order).await?;
        let news_analyses = self.run_independent_analysis(&self.news_agents, &context, &mut messages, &mut message_order).await?;

        // Phase 2: Intra-department debate (agents within same department debate)
        let tech_debated = self.run_intra_department_debate(&tech_analyses, &self.tech_agents, &context, &mut messages, &mut message_order).await?;
        let capital_debated = self.run_intra_department_debate(&capital_analyses, &self.capital_agents, &context, &mut messages, &mut message_order).await?;
        let news_debated = self.run_intra_department_debate(&news_analyses, &self.news_agents, &context, &mut messages, &mut message_order).await?;

        // Phase 3: Cross-department debate (bull camp vs bear camp across departments)
        let all_debated_analyses = [tech_debated, capital_debated, news_debated].concat();
        let cross_dept_analyses = self.run_cross_department_debate(&all_debated_analyses, &context, &mut messages, &mut message_order).await?;

        // Phase 4: Fund Manager Agent makes final decision (integrated, not formula-based)
        let all_analyses = [tech_analyses, capital_analyses, news_analyses].concat();
        let final_decision = self.make_final_decision_with_fund_manager(
            &cross_dept_analyses,
            &all_analyses,
            &market_snapshot,
            symbol,
            &historical_decisions,
        ).await?;

        session.messages = messages;
        session.final_decision = Some(final_decision);
        session.status = DebateStatus::Completed;
        session.updated_at = Utc::now();

        Ok(session)
    }

    /// Load historical decisions from DB for context (memory integration).
    async fn load_historical_decisions(&self, symbol: &str) -> AgentResult<Vec<FundManagerDecision>> {
        let rows = sqlx::query(
            r#"SELECT id, symbol, action, confidence, position_size_percent, leverage,
                      stop_loss_percent, take_profit_percent, reasoning, timestamp
               FROM fund_manager_decisions
               WHERE symbol = $1
               ORDER BY timestamp DESC
               LIMIT 5"#,
        )
        .bind(symbol)
        .fetch_all(&*self.db_pool)
        .await?;

        let mut decisions = Vec::new();
        for row in rows {
            decisions.push(FundManagerDecision {
                session_id: row.get("id"),
                action: parse_decision_action(&row.get::<String, _>("action")),
                symbol: row.get("symbol"),
                confidence: row.get("confidence"),
                position_size_percent: row.get("position_size_percent"),
                leverage: row.get("leverage"),
                stop_loss_percent: row.get("stop_loss_percent"),
                take_profit_percent: row.get("take_profit_percent"),
                reasoning: row.get("reasoning"),
                agent_contributions: Vec::new(),
                risk_assessment: RiskAssessment {
                    overall_risk_level: "unknown".to_string(),
                    max_position_risk: 0.0,
                    margin_requirement: 0.0,
                    risk_reward_ratio: 0.0,
                    volatility_rating: "unknown".to_string(),
                    alerts: Vec::new(),
                    passed: true,
                },
                timestamp: row.get("timestamp"),
            });
        }
        Ok(decisions)
    }

    /// Phase 3: Cross-department debate - Bull camp vs Bear camp.
    /// Agents with same sentiment across departments reinforce each other,
    /// then bull camp and bear camp debate against each other.
    async fn run_cross_department_debate(
        &self,
        analyses: &[AgentAnalysis],
        context: &AnalysisContext,
        messages: &mut Vec<DebateMessage>,
        message_order: &mut i32,
    ) -> AgentResult<Vec<AgentAnalysis>> {
        use crate::agents::models::AgentSentiment;

        // Split into bull camp and bear camp
        let bull_camp: Vec<&AgentAnalysis> = analyses.iter().filter(|a| a.sentiment == AgentSentiment::Bullish).collect();
        let bear_camp: Vec<&AgentAnalysis> = analyses.iter().filter(|a| a.sentiment == AgentSentiment::Bearish).collect();

        if bull_camp.is_empty() || bear_camp.is_empty() {
            return Ok(analyses.to_vec());
        }

        // Build camp summaries
        let bull_summary = bull_camp.iter().map(|a| format!("[{}] {}", a.agent_name, a.content)).collect::<Vec<_>>().join("\n");
        let bear_summary = bear_camp.iter().map(|a| format!("[{}] {}", a.agent_name, a.content)).collect::<Vec<_>>().join("\n");

        // Each agent in bull camp sees bear camp arguments and responds
        for analysis in analyses.iter() {
            let opponent_summary = if analysis.sentiment == AgentSentiment::Bullish {
                &bear_summary
            } else {
                &bull_summary
            };

            // Find the agent
            let agent = self.find_agent_by_name(&analysis.agent_name);
            if let Some(agent) = agent {
                // Create a synthetic opponent analysis representing the opposing camp
                let camp_opponent = AgentAnalysis {
                    agent_name: format!("{:?}_camp", if analysis.sentiment == AgentSentiment::Bullish { "bear" } else { "bull" }),
                    department: analysis.department.clone(),
                    sentiment: if analysis.sentiment == AgentSentiment::Bullish { AgentSentiment::Bearish } else { AgentSentiment::Bullish },
                    confidence: 0.7,
                    content: opponent_summary.clone(),
                    analysis_data: serde_json::json!({"camp": "opposing", "source": "cross_department_debate"}),
                    timestamp: Utc::now(),
                };

                let debate_analysis = agent.debate(context, &camp_opponent, &self.db_pool, self.llm_client.clone()).await?;

                messages.push(DebateMessage {
                    id: Uuid::new_v4(),
                    session_id: context.session_id,
                    agent_name: agent.name().to_string(),
                    agent_department: agent.department(),
                    role: format!("{} (跨部门辩论)", agent.role()),
                    content: debate_analysis.content.clone(),
                    analysis_data: debate_analysis.analysis_data.clone(),
                    confidence: debate_analysis.confidence,
                    sentiment: Some(debate_analysis.sentiment.clone()),
                    message_order: *message_order,
                    created_at: Utc::now(),
                });
                *message_order += 1;
            }
        }

        Ok(analyses.to_vec())
    }

    /// Phase 4: Fund Manager Agent makes final decision using the integrated FundManagerAgent.
    async fn make_final_decision_with_fund_manager(
        &self,
        cross_dept_analyses: &[AgentAnalysis],
        all_analyses: &[AgentAnalysis],
        snapshot: &MarketSnapshot,
        symbol: &str,
        historical_decisions: &[FundManagerDecision],
    ) -> AgentResult<FundManagerDecision> {
        use crate::agents::agents::{FundManagerAgent, FundManagerConfig, PortfolioContext};

        // Load dynamic credibility scores from agent_performance table
        let dynamic_credibilities = self.load_dynamic_credibilities().await?;

        // Calculate weighted sentiment with dynamic credibility
        let (weighted_bullish, weighted_bearish, weighted_neutral, agent_contributions) =
            self.calculate_weighted_sentiment_dynamic(all_analyses, &dynamic_credibilities);

        let total = weighted_bullish + weighted_bearish + weighted_neutral;
        let bullish_ratio = if total > 0.0 { weighted_bullish / total } else { 0.0 };
        let bearish_ratio = if total > 0.0 { weighted_bearish / total } else { 0.0 };

        // Use FundManagerAgent for decision making
        let fund_manager = FundManagerAgent::new(FundManagerConfig::default());

        // Build portfolio context (default for now, could be loaded from DB)
        let portfolio_context = PortfolioContext::default();

        // Get decision from FundManagerAgent
        let fm_decision = fund_manager
            .make_decision(
                Uuid::new_v4(),
                symbol,
                all_analyses,
                snapshot,
                &portfolio_context,
            )
            .await?;

        // Build reasoning with cross-department debate context
        let reasoning = format!(
            "{}\n\n\
             [跨部门辩论总结] 看多权重: {:.1}%, 看空权重: {:.1}%, 中性权重: {:.1}%\n\
             [动态可信度] 已加载 {} 个 Agent 的动态可信度\n\
             [历史参考] {} 条历史决策",
            fm_decision.reasoning,
            bullish_ratio * 100.0,
            bearish_ratio * 100.0,
            (1.0 - bullish_ratio - bearish_ratio).max(0.0) * 100.0,
            dynamic_credibilities.len(),
            historical_decisions.len()
        );

        Ok(FundManagerDecision {
            session_id: fm_decision.session_id,
            action: fm_decision.action,
            symbol: fm_decision.symbol,
            confidence: fm_decision.confidence,
            position_size_percent: fm_decision.position_size_percent,
            leverage: fm_decision.leverage,
            stop_loss_percent: fm_decision.stop_loss_percent,
            take_profit_percent: fm_decision.take_profit_percent,
            reasoning,
            agent_contributions,
            risk_assessment: fm_decision.risk_assessment,
            timestamp: fm_decision.timestamp,
        })
    }

    /// Load dynamic credibility scores from agent_performance table.
    async fn load_dynamic_credibilities(&self) -> AgentResult<std::collections::HashMap<String, f64>> {
        let rows = sqlx::query(
            r#"SELECT agent_id, credibility_score, weighted_accuracy
               FROM agent_performance
               WHERE updated_at > NOW() - INTERVAL '30 days'
               ORDER BY updated_at DESC"#,
        )
        .fetch_all(&*self.db_pool)
        .await?;

        let mut credibilities = std::collections::HashMap::new();
        for row in rows {
            let agent_id: String = row.get("agent_id");
            let base_credibility: f64 = row.get("credibility_score");
            let weighted_accuracy: Option<f64> = row.get("weighted_accuracy");

            // Combine base credibility with weighted accuracy (70% base + 30% recent accuracy)
            let dynamic_score = if let Some(accuracy) = weighted_accuracy {
                base_credibility * 0.7 + accuracy * 0.3
            } else {
                base_credibility
            };

            credibilities.insert(agent_id, dynamic_score.clamp(0.1, 1.0));
        }

        Ok(credibilities)
    }

    /// Calculate weighted sentiment using dynamic credibility scores.
    fn calculate_weighted_sentiment_dynamic(
        &self,
        analyses: &[AgentAnalysis],
        dynamic_credibilities: &std::collections::HashMap<String, f64>,
    ) -> (f64, f64, f64, Vec<AgentContribution>) {
        let mut weighted_bullish = 0.0;
        let mut weighted_bearish = 0.0;
        let mut weighted_neutral = 0.0;
        let mut contributions = Vec::new();

        let department_weights: HashMap<AgentDepartment, f64> = [
            (AgentDepartment::Technical, 0.35),
            (AgentDepartment::Capital, 0.35),
            (AgentDepartment::News, 0.30),
        ]
        .iter()
        .cloned()
        .collect();

        for analysis in analyses {
            let agent = self.find_agent_by_name(&analysis.agent_name);
            // Use dynamic credibility if available, otherwise fall back to agent's base score
            let credibility = dynamic_credibilities
                .get(&analysis.agent_name)
                .copied()
                .or_else(|| agent.map(|a| a.credibility_score()))
                .unwrap_or(0.5);
            let dept_weight = department_weights.get(&analysis.department).unwrap_or(&0.33);

            let contribution_weight = credibility * analysis.confidence * dept_weight;

            match analysis.sentiment {
                AgentSentiment::Bullish => {
                    weighted_bullish += contribution_weight;
                }
                AgentSentiment::Bearish => {
                    weighted_bearish += contribution_weight;
                }
                AgentSentiment::Neutral | AgentSentiment::Cautious => {
                    weighted_neutral += contribution_weight;
                }
            }

            contributions.push(AgentContribution {
                agent_name: analysis.agent_name.clone(),
                department: analysis.department.clone(),
                sentiment: analysis.sentiment.clone(),
                confidence: analysis.confidence,
                contribution_weight,
                credibility_score: credibility,
            });
        }

        (weighted_bullish, weighted_bearish, weighted_neutral, contributions)
    }

    async fn run_independent_analysis(
        &self,
        agents: &[Arc<dyn Agent>],
        context: &AnalysisContext,
        messages: &mut Vec<DebateMessage>,
        message_order: &mut i32,
    ) -> AgentResult<Vec<AgentAnalysis>> {
        let llm = self.llm_client.clone();
        let futures = agents
            .iter()
            .map(|agent| agent.analyze(context, &self.db_pool, llm.clone()));

        let analyses: Vec<AgentResult<AgentAnalysis>> = join_all(futures).await;

        let mut results = Vec::new();
        for (agent, analysis_result) in agents.iter().zip(analyses) {
            let analysis = analysis_result?;

            messages.push(DebateMessage {
                id: Uuid::new_v4(),
                session_id: context.session_id,
                agent_name: agent.name().to_string(),
                agent_department: agent.department(),
                role: agent.role().to_string(),
                content: analysis.content.clone(),
                analysis_data: analysis.analysis_data.clone(),
                confidence: analysis.confidence,
                sentiment: Some(analysis.sentiment.clone()),
                message_order: *message_order,
                created_at: Utc::now(),
            });
            *message_order += 1;

            results.push(analysis);
        }

        Ok(results)
    }

    async fn run_intra_department_debate(
        &self,
        analyses: &[AgentAnalysis],
        agents: &[Arc<dyn Agent>],
        context: &AnalysisContext,
        messages: &mut Vec<DebateMessage>,
        message_order: &mut i32,
    ) -> AgentResult<Vec<AgentAnalysis>> {
        let mut debated_analyses = analyses.to_vec();
        for (i, (agent, analysis)) in agents.iter().zip(analyses.iter()).enumerate() {
            for (j, opponent_analysis) in analyses.iter().enumerate() {
                if i != j && analysis.sentiment != opponent_analysis.sentiment {
                    let debate_analysis = agent.debate(context, opponent_analysis, &self.db_pool, self.llm_client.clone()).await?;

                    messages.push(DebateMessage {
                        id: Uuid::new_v4(),
                        session_id: context.session_id,
                        agent_name: agent.name().to_string(),
                        agent_department: agent.department(),
                        role: format!("{} (辩论回应)", agent.role()),
                        content: debate_analysis.content.clone(),
                        analysis_data: debate_analysis.analysis_data.clone(),
                        confidence: debate_analysis.confidence,
                        sentiment: Some(debate_analysis.sentiment.clone()),
                        message_order: *message_order,
                        created_at: Utc::now(),
                    });
                    *message_order += 1;

                    // Update the debated analysis with the new one
                    if let Some(d) = debated_analyses.get_mut(i) {
                        *d = debate_analysis;
                    }
                }
            }
        }
        Ok(debated_analyses)
    }

    fn make_final_decision(
        &self,
        analyses: &[AgentAnalysis],
        snapshot: &MarketSnapshot,
        symbol: &str,
    ) -> AgentResult<FundManagerDecision> {
        let (weighted_bullish, weighted_bearish, weighted_neutral, agent_contributions) = self.calculate_weighted_sentiment(analyses);

        let total = weighted_bullish + weighted_bearish + weighted_neutral;
        let bullish_ratio = weighted_bullish / total;
        let bearish_ratio = weighted_bearish / total;

        let (action, confidence) = if bullish_ratio > 0.6 {
            (DecisionAction::Long, bullish_ratio)
        } else if bearish_ratio > 0.6 {
            (DecisionAction::Short, bearish_ratio)
        } else {
            (DecisionAction::Hold, 1.0 - (bullish_ratio - bearish_ratio).abs())
        };

        let (position_size, leverage) = match action {
            DecisionAction::Long | DecisionAction::Short => {
                let size = (confidence * 10.0).min(10.0);
                let lev = if confidence > 0.75 { 3 } else { 2 };
                (size, lev)
            }
            DecisionAction::Hold => (0.0, 1),
        };

        let (stop_loss, take_profit) = match action {
            DecisionAction::Long => (
                Some(snapshot.current_price * 0.95),
                Some(snapshot.current_price * 1.10),
            ),
            DecisionAction::Short => (
                Some(snapshot.current_price * 1.05),
                Some(snapshot.current_price * 0.90),
            ),
            DecisionAction::Hold => (None, None),
        };

        let reasoning = format!(
            "基于{}位分析师的综合分析：看多权重{:.1}%，看空权重{:.1}%，中性权重{:.1}%。最终决定：{:?}。",
            analyses.len(),
            bullish_ratio * 100.0,
            bearish_ratio * 100.0,
            weighted_neutral / total * 100.0,
            action
        );

        let risk_assessment = RiskAssessment {
            overall_risk_level: if confidence > 0.7 { "medium" } else { "low" }.to_string(),
            max_position_risk: position_size,
            margin_requirement: position_size / leverage as f64,
            risk_reward_ratio: 2.0,
            volatility_rating: "normal".to_string(),
            alerts: Vec::new(),
            passed: true,
        };

        Ok(FundManagerDecision {
            session_id: Uuid::new_v4(),
            action,
            symbol: symbol.to_string(),
            confidence,
            position_size_percent: position_size,
            leverage,
            stop_loss_percent: stop_loss,
            take_profit_percent: take_profit,
            reasoning,
            agent_contributions,
            risk_assessment,
            timestamp: Utc::now(),
        })
    }

    fn calculate_weighted_sentiment(
        &self,
        analyses: &[AgentAnalysis],
    ) -> (f64, f64, f64, Vec<AgentContribution>) {
        let mut weighted_bullish = 0.0;
        let mut weighted_bearish = 0.0;
        let mut weighted_neutral = 0.0;
        let mut contributions = Vec::new();

        let department_weights: HashMap<AgentDepartment, f64> = [
            (AgentDepartment::Technical, 0.35),
            (AgentDepartment::Capital, 0.35),
            (AgentDepartment::News, 0.30),
        ]
        .iter()
        .cloned()
        .collect();

        for analysis in analyses {
            let agent = self.find_agent_by_name(&analysis.agent_name);
            let credibility = agent.map(|a| a.credibility_score()).unwrap_or(0.5);
            let dept_weight = department_weights.get(&analysis.department).unwrap_or(&0.33);

            let contribution_weight = credibility * analysis.confidence * dept_weight;

            match analysis.sentiment {
                AgentSentiment::Bullish => {
                    weighted_bullish += contribution_weight;
                }
                AgentSentiment::Bearish => {
                    weighted_bearish += contribution_weight;
                }
                AgentSentiment::Neutral | AgentSentiment::Cautious => {
                    weighted_neutral += contribution_weight;
                }
            }

            contributions.push(AgentContribution {
                agent_name: analysis.agent_name.clone(),
                department: analysis.department.clone(),
                sentiment: analysis.sentiment.clone(),
                confidence: analysis.confidence,
                contribution_weight,
                credibility_score: credibility,
            });
        }

        (weighted_bullish, weighted_bearish, weighted_neutral, contributions)
    }

    fn find_agent_by_name(&self, name: &str) -> Option<&Arc<dyn Agent>> {
        self.tech_agents
            .iter()
            .chain(self.capital_agents.iter())
            .chain(self.news_agents.iter())
            .find(|agent| agent.name() == name)
    }

    pub async fn save_session_to_db(&self, session: &DebateSession) -> AgentResult<()> {
        let mut tx = self.db_pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO debate_sessions (id, config_id, user_id, symbol, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(session.id)
        .bind(session.config_id)
        .bind(session.user_id)
        .bind(&session.symbol)
        .bind(format!("{:?}", session.status))
        .bind(session.created_at)
        .bind(session.updated_at)
        .execute(&mut *tx)
        .await?;

        for message in &session.messages {
            sqlx::query(
                r#"
                INSERT INTO debate_messages (id, session_id, agent_name, agent_department, role, content, analysis_data, confidence, sentiment, message_order, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
            )
            .bind(message.id)
            .bind(message.session_id)
            .bind(&message.agent_name)
            .bind(format!("{:?}", message.agent_department))
            .bind(&message.role)
            .bind(&message.content)
            .bind(&message.analysis_data)
            .bind(message.confidence)
            .bind(message.sentiment.as_ref().map(|s| format!("{:?}", s)))
            .bind(message.message_order)
            .bind(message.created_at)
            .execute(&mut *tx)
            .await?;
        }

        if let Some(decision) = &session.final_decision {
            sqlx::query(
                r#"
                INSERT INTO fund_manager_decisions (id, session_id, symbol, action, confidence, position_size_percent, leverage, stop_loss_percent, take_profit_percent, reasoning, agent_contributions, risk_assessment, timestamp)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                "#,
            )
            .bind(decision.session_id)
            .bind(session.id)
            .bind(&decision.symbol)
            .bind(format!("{:?}", decision.action))
            .bind(decision.confidence)
            .bind(decision.position_size_percent)
            .bind(decision.leverage)
            .bind(decision.stop_loss_percent)
            .bind(decision.take_profit_percent)
            .bind(&decision.reasoning)
            .bind(serde_json::to_value(&decision.agent_contributions)?)
            .bind(serde_json::to_value(&decision.risk_assessment)?)
            .bind(decision.timestamp)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_session_from_db(&self, session_id: Uuid) -> AgentResult<Option<DebateSession>> {
        let session_row = sqlx::query(
            r#"
            SELECT id, config_id, user_id, symbol, status, created_at, updated_at
            FROM debate_sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.db_pool)
        .await?;

        let session_row = match session_row {
            Some(row) => row,
            None => return Ok(None),
        };

        let messages = sqlx::query(
            r#"
            SELECT id, session_id, agent_name, agent_department, role, content, analysis_data, confidence, sentiment, message_order, created_at
            FROM debate_messages
            WHERE session_id = $1
            ORDER BY message_order
            "#,
        )
        .bind(session_id)
        .fetch_all(&*self.db_pool)
        .await?;

        let decision = sqlx::query(
            r#"
            SELECT id, session_id, symbol, action, confidence, position_size_percent, leverage, stop_loss_percent, take_profit_percent, reasoning, agent_contributions, risk_assessment, timestamp
            FROM fund_manager_decisions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.db_pool)
        .await?;

        let mut debate_messages = Vec::new();
        for msg_row in messages {
            debate_messages.push(DebateMessage {
                id: msg_row.get("id"),
                session_id: msg_row.get("session_id"),
                agent_name: msg_row.get("agent_name"),
                agent_department: parse_agent_department(&msg_row.get::<String, _>("agent_department")),
                role: msg_row.get("role"),
                content: msg_row.get("content"),
                analysis_data: msg_row.get("analysis_data"),
                confidence: msg_row.get("confidence"),
                sentiment: msg_row.get::<Option<String>, _>("sentiment").map(|s| parse_agent_sentiment(&s)),
                message_order: msg_row.get("message_order"),
                created_at: msg_row.get("created_at"),
            });
        }

        let final_decision = decision.map(|d_row| -> AgentResult<FundManagerDecision> {
            Ok(FundManagerDecision {
                session_id: d_row.get("id"),
                action: parse_decision_action(&d_row.get::<String, _>("action")),
                symbol: d_row.get("symbol"),
                confidence: d_row.get("confidence"),
                position_size_percent: d_row.get("position_size_percent"),
                leverage: d_row.get("leverage"),
                stop_loss_percent: d_row.get("stop_loss_percent"),
                take_profit_percent: d_row.get("take_profit_percent"),
                reasoning: d_row.get("reasoning"),
                agent_contributions: serde_json::from_value(d_row.get("agent_contributions"))?,
                risk_assessment: serde_json::from_value(d_row.get("risk_assessment"))?,
                timestamp: d_row.get("timestamp"),
            })
        }).transpose()?;

        Ok(Some(DebateSession {
            id: session_row.get("id"),
            config_id: session_row.get("config_id"),
            user_id: session_row.get("user_id"),
            symbol: session_row.get("symbol"),
            status: parse_debate_status(&session_row.get::<String, _>("status")),
            messages: debate_messages,
            final_decision,
            created_at: session_row.get("created_at"),
            updated_at: session_row.get("updated_at"),
        }))
    }
}

fn parse_agent_department(s: &str) -> AgentDepartment {
    match s {
        "Technical" => AgentDepartment::Technical,
        "Capital" => AgentDepartment::Capital,
        "News" => AgentDepartment::News,
        "FundManager" => AgentDepartment::FundManager,
        _ => AgentDepartment::Technical,
    }
}

fn parse_agent_sentiment(s: &str) -> AgentSentiment {
    match s {
        "Bullish" => AgentSentiment::Bullish,
        "Bearish" => AgentSentiment::Bearish,
        "Neutral" => AgentSentiment::Neutral,
        "Cautious" => AgentSentiment::Cautious,
        _ => AgentSentiment::Neutral,
    }
}

fn parse_decision_action(s: &str) -> DecisionAction {
    match s {
        "Long" => DecisionAction::Long,
        "Short" => DecisionAction::Short,
        "Hold" => DecisionAction::Hold,
        _ => DecisionAction::Hold,
    }
}

fn parse_debate_status(s: &str) -> DebateStatus {
    match s {
        "InProgress" => DebateStatus::InProgress,
        "Completed" => DebateStatus::Completed,
        "Failed" => DebateStatus::Failed,
        _ => DebateStatus::InProgress,
    }
}

// ============================================
// Agent Registry
// ============================================

pub struct AgentRegistry {
    agents: HashMap<String, Arc<dyn Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        let mut agents = HashMap::new();

        let tech_agents: Vec<Arc<dyn Agent>> = vec![
            Arc::new(KlinePatternAnalyst::new()),
            Arc::new(TechnicalIndicatorAnalyst::new()),
            Arc::new(OnChainDataAnalyst::new()),
            Arc::new(QuantModelAnalyst::new()),
        ];

        let capital_agents: Vec<Arc<dyn Agent>> = vec![
            Arc::new(FundingRateAnalyst::new()),
            Arc::new(PositionStructureAnalyst::new()),
            Arc::new(LongShortGameAnalyst::new()),
            Arc::new(LiquidityAnalyst::new()),
        ];

        let news_agents: Vec<Arc<dyn Agent>> = vec![
            Arc::new(SentimentAnalyst::new()),
            Arc::new(MacroPolicyAnalyst::new()),
            Arc::new(KOLWhaleMonitor::new()),
            Arc::new(EventDrivenAnalyst::new()),
        ];

        for agent in tech_agents
            .into_iter()
            .chain(capital_agents.into_iter())
            .chain(news_agents.into_iter())
        {
            agents.insert(agent.agent_id().to_string(), agent);
        }

        Self { agents }
    }

    pub fn get_agent(&self, agent_id: &str) -> Option<&Arc<dyn Agent>> {
        self.agents.get(agent_id)
    }

    pub fn get_all_agents(&self) -> Vec<&Arc<dyn Agent>> {
        self.agents.values().collect()
    }

    pub fn get_agents_by_department(&self, department: AgentDepartment) -> Vec<&Arc<dyn Agent>> {
        self.agents
            .values()
            .filter(|agent| agent.department() == department)
            .collect()
    }

    pub fn get_agent_profiles(&self) -> Vec<AgentProfile> {
        self.agents
            .values()
            .map(|agent| AgentProfile {
                name: agent.name().to_string(),
                department: agent.department(),
                role: agent.role().to_string(),
                reference_institution: agent.reference_institution().to_string(),
                credibility_score: agent.credibility_score(),
                calibration_factor: 1.0,
                created_at: Utc::now(),
            })
            .collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
