use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::agents::config::AgentConfig;
use crate::agents::llm_client::{LlmClient, LlmConfig, LlmProvider};
use crate::agents::market::{DatabaseMarketDataProvider, MarketDataProvider};
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::routes::trading::get_okx_client;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/symbols", get(get_symbols))
        .route("/timeframes", get(get_timeframes))
        .route("/market-data", post(get_market_data))
        .route("/analyze/technical", post(analyze_technical))
        .route("/analyze/funding", post(analyze_funding))
        .route("/analyze/sentiment", post(analyze_sentiment))
        .route("/analyze/comprehensive", post(analyze_comprehensive))
        .route("/technical", post(technical_analysis))
        .route("/funding", post(funding_analysis))
        .route("/sentiment", post(sentiment_analysis))
        .route("/comprehensive", post(comprehensive_analysis))
        .route("/usage", get(get_usage))
        .route("/usage/reset", post(reset_usage))
        .route("/generate-report", post(generate_report))
        .route("/debate", post(start_debate))
        .route("/debate/{session_id}", get(get_debate_session))
        .route("/debates", get(list_debate_sessions))
}

async fn get_symbols(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Value>> {
    let symbols = sqlx::query_scalar::<_, String>(
        r#"SELECT DISTINCT symbol FROM market_data ORDER BY symbol"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(json!({"symbols": symbols})))
}

async fn get_timeframes() -> Result<Json<Value>> {
    Ok(Json(json!({
        "timeframes": ["1m", "5m", "15m", "30m", "1H", "4H", "1D", "1W"]
    })))
}

#[derive(Debug, Deserialize)]
struct MarketDataRequest {
    symbol: String,
    interval: Option<String>,
    limit: Option<i64>,
}

async fn get_market_data(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<MarketDataRequest>,
) -> Result<Json<Value>> {
    let interval = req.interval.unwrap_or_else(|| "1H".to_string());
    let limit = req.limit.unwrap_or(100).min(1000);

    let klines = sqlx::query(
        r#"SELECT id, symbol, interval, open_time, open::float8, high::float8, low::float8, close::float8, volume::float8, created_at
        FROM market_data WHERE symbol = $1 AND interval = $2 ORDER BY open_time DESC LIMIT $3"#,
    )
    .bind(&req.symbol)
    .bind(&interval)
    .bind(limit)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let klines: Vec<_> = klines.iter().map(|k| json!({
        "id": k.get::<i32, _>("id"),
        "symbol": k.get::<String, _>("symbol"),
        "interval": k.get::<String, _>("interval"),
        "open_time": k.get::<DateTime<Utc>, _>("open_time"),
        "open": k.get::<f64, _>("open"),
        "high": k.get::<f64, _>("high"),
        "low": k.get::<f64, _>("low"),
        "close": k.get::<f64, _>("close"),
        "volume": k.get::<f64, _>("volume"),
        "created_at": k.get::<DateTime<Utc>, _>("created_at"),
    })).collect();

    let funding = sqlx::query(
        r#"SELECT symbol, funding_rate::float8, next_funding_time, created_at FROM funding_rates WHERE symbol = $1 ORDER BY created_at DESC LIMIT 5"#,
    )
    .bind(&req.symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let funding: Vec<_> = funding.iter().map(|f| json!({
        "symbol": f.get::<String, _>("symbol"),
        "funding_rate": f.get::<f64, _>("funding_rate"),
        "next_funding_time": f.get::<Option<DateTime<Utc>>, _>("next_funding_time"),
        "created_at": f.get::<DateTime<Utc>, _>("created_at"),
    })).collect();

    let sentiment = sqlx::query(
        r#"SELECT sentiment_score::float8, source, created_at FROM sentiment_data WHERE symbol = $1 ORDER BY created_at DESC LIMIT 5"#,
    )
    .bind(&req.symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let sentiment: Vec<_> = sentiment.iter().map(|s| json!({
        "sentiment_score": s.get::<f64, _>("sentiment_score"),
        "source": s.get::<String, _>("source"),
        "created_at": s.get::<DateTime<Utc>, _>("created_at"),
    })).collect();

    Ok(Json(json!({
        "klines": klines,
        "funding_rates": funding,
        "sentiment": sentiment,
    })))
}

#[derive(Debug, Deserialize)]
struct AnalysisRequest {
    symbol: String,
    interval: Option<String>,
    strategy_id: Option<i32>,
}

async fn analyze_technical(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    let interval = req.interval.unwrap_or_else(|| "1H".to_string());

    let provider = DatabaseMarketDataProvider::new(state.db_pool.clone());
    let indicators = provider
        .calculate_technical_indicators(&req.symbol, &interval, &AgentConfig::default())
        .await
        .map_err(|e| AppError::Internal(format!("Technical indicator calculation failed: {}", e)))?;

    let klines = sqlx::query(
        r#"SELECT open::float8, high::float8, low::float8, close::float8, volume::float8 FROM market_data WHERE symbol = $1 AND interval = $2 ORDER BY open_time DESC LIMIT 100"#,
    )
    .bind(&req.symbol)
    .bind(&interval)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let closes: Vec<f64> = klines.iter().map(|k| k.get::<f64, _>("close")).collect();
    let sma_20 = if closes.len() >= 20 { closes[0..20].iter().sum::<f64>() / 20.0 } else { 0.0 };
    let sma_50 = if closes.len() >= 50 { closes[0..50].iter().sum::<f64>() / 50.0 } else { 0.0 };
    let current_price = closes.first().copied().unwrap_or(0.0);

    let trend = if current_price > sma_20 && current_price > sma_50 { "bullish" }
        else if current_price < sma_20 && current_price < sma_50 { "bearish" }
        else { "neutral" };

    let rsi = indicators.rsi.unwrap_or(50.0);

    let (macd_line, macd_signal, macd_histogram, macd_signal_str) = match &indicators.macd {
        Some(macd) => {
            let signal_str = if macd.histogram > 0.0 { "bullish" }
                else if macd.histogram < 0.0 { "bearish" }
                else { "neutral" };
            (Some(macd.macd_line), Some(macd.signal_line), Some(macd.histogram), signal_str)
        }
        None => (None, None, None, "neutral"),
    };

    let bollinger = indicators.bollinger_bands.as_ref().map(|bb| json!({
        "upper": bb.upper,
        "middle": bb.middle,
        "lower": bb.lower,
    }));

    Ok(Json(json!({
        "symbol": req.symbol,
        "trend": trend,
        "current_price": current_price,
        "sma_20": sma_20,
        "sma_50": sma_50,
        "support_levels": [],
        "resistance_levels": [],
        "indicators": {
            "rsi": rsi,
            "macd_line": macd_line,
            "macd_signal": macd_signal,
            "macd_histogram": macd_histogram,
            "macd_signal_str": macd_signal_str,
            "bollinger_bands": bollinger,
        },
    })))
}

async fn analyze_funding(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    let funding = sqlx::query(
        r#"SELECT funding_rate::float8, realized_rate::float8, created_at FROM funding_rates WHERE symbol = $1 ORDER BY created_at DESC LIMIT 30"#,
    )
    .bind(&req.symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let current_rate = funding.first().map(|f| f.get::<f64, _>("funding_rate")).unwrap_or(0.0);
    let avg_rate = if funding.is_empty() { 0.0 } else { funding.iter().map(|f| f.get::<f64, _>("funding_rate")).sum::<f64>() / funding.len() as f64 };

    let funding_json: Vec<_> = funding.iter().map(|f| json!({
        "funding_rate": f.get::<f64, _>("funding_rate"),
        "realized_rate": f.get::<Option<f64>, _>("realized_rate"),
        "created_at": f.get::<DateTime<Utc>, _>("created_at"),
    })).collect();

    Ok(Json(json!({
        "symbol": req.symbol,
        "current_rate": current_rate,
        "average_rate": avg_rate,
        "trend": if avg_rate > 0.01 { "longs_paying" } else if avg_rate < -0.01 { "shorts_paying" } else { "balanced" },
        "history": funding_json,
    })))
}

async fn analyze_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    let sentiment = sqlx::query(
        r#"SELECT sentiment_score::float8, source, created_at FROM sentiment_data WHERE symbol = $1 ORDER BY created_at DESC LIMIT 50"#,
    )
    .bind(&req.symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let avg = if sentiment.is_empty() { 0.5 } else { sentiment.iter().map(|s| s.get::<f64, _>("sentiment_score")).sum::<f64>() / sentiment.len() as f64 };

    Ok(Json(json!({
        "symbol": req.symbol,
        "overall_sentiment": if avg > 0.6 { "positive" } else if avg < 0.4 { "negative" } else { "neutral" },
        "sentiment_score": avg,
        "sample_count": sentiment.len(),
    })))
}

async fn analyze_comprehensive(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    let interval = req.interval.unwrap_or_else(|| "1H".to_string());

    let provider = DatabaseMarketDataProvider::new(state.db_pool.clone());
    let indicators = provider
        .calculate_technical_indicators(&req.symbol, &interval, &AgentConfig::default())
        .await
        .map_err(|e| AppError::Internal(format!("Technical indicator calculation failed: {}", e)))?;

    let klines = sqlx::query(
        r#"SELECT open::float8, high::float8, low::float8, close::float8, volume::float8 FROM market_data WHERE symbol = $1 AND interval = $2 ORDER BY open_time DESC LIMIT 100"#,
    )
    .bind(&req.symbol)
    .bind(&interval)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let closes: Vec<f64> = klines.iter().map(|k| k.get::<f64, _>("close")).collect();
    let current_price = closes.first().copied().unwrap_or(0.0);

    let funding = sqlx::query(
        r#"SELECT funding_rate::float8 FROM funding_rates WHERE symbol = $1 ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(&req.symbol)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let funding_rate = funding
        .map(|f| f.get::<f64, _>("funding_rate"))
        .unwrap_or(0.0);

    let sentiment = sqlx::query(
        r#"SELECT sentiment_score::float8 FROM sentiment_data WHERE symbol = $1 ORDER BY created_at DESC LIMIT 10"#,
    )
    .bind(&req.symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let avg_sentiment = if sentiment.is_empty() {
        0.5
    } else {
        sentiment.iter().map(|s| s.get::<f64, _>("sentiment_score")).sum::<f64>() / sentiment.len() as f64
    };

    let rsi = indicators.rsi.unwrap_or(50.0);
    let macd_info = indicators.macd.as_ref().map(|m| format!(
        "MACD线: {:.4}, 信号线: {:.4}, 柱状图: {:.4}",
        m.macd_line, m.signal_line, m.histogram
    )).unwrap_or_else(|| "MACD数据不足".to_string());

    let bb_info = indicators.bollinger_bands.as_ref().map(|bb| format!(
        "上轨: {:.2}, 中轨: {:.2}, 下轨: {:.2}",
        bb.upper, bb.middle, bb.lower
    )).unwrap_or_else(|| "布林带数据不足".to_string());

    let user_message = format!(
        "请对 {} 进行综合分析，以下是当前市场数据：\n\n\
        ## 价格信息\n\
        当前价格: {:.2}\n\
        时间周期: {}\n\n\
        ## 技术指标\n\
        RSI(14): {:.2}\n\
        {}\n\
        {}\n\n\
        ## 资金费率\n\
        当前资金费率: {:.6}\n\n\
        ## 市场情绪\n\
        情绪评分: {:.2} (0-1, 越高越乐观)\n\n\
        请基于以上数据给出综合分析，必须以JSON格式回复：\n\
        {{\"direction\": \"bullish\"|\"bearish\"|\"neutral\", \"confidence\": 0.0-1.0, \"risk_level\": \"low\"|\"medium\"|\"high\", \"analysis\": \"详细分析\", \"entry_range\": {{\"low\": 价格, \"high\": 价格}}, \"stop_loss\": 价格, \"take_profit\": [价格1, 价格2], \"leverage_suggestion\": 1-10, \"key_factors\": [\"因素1\", \"因素2\"]}}\n\
        只输出JSON，不要输出其他内容。",
        req.symbol, current_price, interval, rsi, macd_info, bb_info, funding_rate, avg_sentiment
    );

    let system_prompt = "你是一位专业的加密货币交易分析师，擅长技术分析、资金流分析和市场情绪分析。\
        你需要基于提供的市场数据给出综合分析判断。\
        你的分析必须客观、数据驱动，同时考虑风险管理。\
        你必须以JSON格式回复，不要输出其他内容。";

    let llm_result = analyze_with_llm(
        &state.db_pool,
        user.user_id,
        system_prompt,
        &user_message,
        "comprehensive_analysis",
    )
    .await;

    match llm_result {
        Some(response) => {
            let parsed = parse_llm_comprehensive_response(&response, current_price);

            let analysis_id = if let Some(strategy_id) = req.strategy_id {
                let analysis = sqlx::query(
                    r#"INSERT INTO ai_analysis (strategy_id, content, analysis_type) VALUES ($1, $2, 'comprehensive') RETURNING id"#,
                )
                .bind(strategy_id)
                .bind(&json!({
                    "price": current_price,
                    "direction": parsed.direction,
                    "confidence": parsed.confidence,
                    "llm_response": response,
                }))
                .fetch_one(&state.db_pool)
                .await
                .map_err(|e| AppError::Database(e))?;
                Some(analysis.get::<i32, _>("id"))
            } else {
                None
            };

            Ok(Json(json!({
                "analysis_id": analysis_id,
                "symbol": req.symbol,
                "direction": parsed.direction,
                "confidence": parsed.confidence,
                "risk_level": parsed.risk_level,
                "entry_range": parsed.entry_range,
                "stop_loss": parsed.stop_loss,
                "take_profit": parsed.take_profit,
                "leverage_suggestion": parsed.leverage_suggestion,
                "analysis": parsed.analysis,
                "key_factors": parsed.key_factors,
                "source": "llm",
            })))
        }
        None => {
            warn!("LLM unavailable for comprehensive analysis, falling back to rule-based analysis");

            let analysis_id = if let Some(strategy_id) = req.strategy_id {
                let analysis = sqlx::query(
                    r#"INSERT INTO ai_analysis (strategy_id, content, analysis_type) VALUES ($1, $2, 'comprehensive') RETURNING id"#,
                )
                .bind(strategy_id)
                .bind(json!({"price": current_price, "trend": "neutral"}))
                .fetch_one(&state.db_pool)
                .await
                .map_err(|e| AppError::Database(e))?;
                Some(analysis.get::<i32, _>("id"))
            } else {
                None
            };

            let direction = if rsi < 30.0 || (indicators.macd.as_ref().map_or(false, |m| m.histogram > 0.0)) {
                "bullish"
            } else if rsi > 70.0 || (indicators.macd.as_ref().map_or(false, |m| m.histogram < 0.0)) {
                "bearish"
            } else {
                "neutral"
            };

            let confidence = match direction {
                "bullish" if rsi < 30.0 => 0.75,
                "bearish" if rsi > 70.0 => 0.75,
                _ => 0.5,
            };

            let risk_level = if rsi > 70.0 || rsi < 30.0 { "high" } else { "medium" };

            Ok(Json(json!({
                "analysis_id": analysis_id,
                "symbol": req.symbol,
                "direction": direction,
                "confidence": confidence,
                "risk_level": risk_level,
                "entry_range": {"low": current_price * 0.99, "high": current_price * 1.01},
                "stop_loss": current_price * 0.97,
                "take_profit": [current_price * 1.03, current_price * 1.05],
                "leverage_suggestion": 2,
                "source": "rule_based_fallback",
            })))
        }
    }
}

struct ComprehensiveParsed {
    direction: String,
    confidence: f64,
    risk_level: String,
    entry_range: Value,
    stop_loss: f64,
    take_profit: Vec<f64>,
    leverage_suggestion: i32,
    analysis: String,
    key_factors: Vec<String>,
}

fn parse_llm_comprehensive_response(response: &str, current_price: f64) -> ComprehensiveParsed {
    let cleaned = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match serde_json::from_str::<Value>(cleaned) {
        Ok(parsed) => {
            let direction = parsed
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("neutral")
                .to_string();

            let confidence = parsed
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5)
                .clamp(0.0, 1.0);

            let risk_level = parsed
                .get("risk_level")
                .and_then(|v| v.as_str())
                .unwrap_or("medium")
                .to_string();

            let entry_range = parsed
                .get("entry_range")
                .cloned()
                .unwrap_or_else(|| json!({"low": current_price * 0.99, "high": current_price * 1.01}));

            let stop_loss = parsed
                .get("stop_loss")
                .and_then(|v| v.as_f64())
                .unwrap_or(current_price * 0.97);

            let take_profit = parsed
                .get("take_profit")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_f64())
                        .collect()
                })
                .unwrap_or_else(|| vec![current_price * 1.03, current_price * 1.05]);

            let leverage_suggestion = parsed
                .get("leverage_suggestion")
                .and_then(|v| v.as_i64())
                .unwrap_or(2) as i32;

            let analysis = parsed
                .get("analysis")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let key_factors = parsed
                .get("key_factors")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            ComprehensiveParsed {
                direction,
                confidence,
                risk_level,
                entry_range,
                stop_loss,
                take_profit,
                leverage_suggestion,
                analysis,
                key_factors,
            }
        }
        Err(e) => {
            debug!("Failed to parse LLM comprehensive response as JSON: {}, falling back to defaults", e);
            ComprehensiveParsed {
                direction: "neutral".to_string(),
                confidence: 0.5,
                risk_level: "medium".to_string(),
                entry_range: json!({"low": current_price * 0.99, "high": current_price * 1.01}),
                stop_loss: current_price * 0.97,
                take_profit: vec![current_price * 1.03, current_price * 1.05],
                leverage_suggestion: 2,
                analysis: response.to_string(),
                key_factors: vec![],
            }
        }
    }
}

async fn get_llm_client_from_db(
    pool: &sqlx::PgPool,
    user_id: i64,
) -> Result<Option<LlmClient>> {
    let row = sqlx::query(
        r#"SELECT provider, api_key_encrypted, base_url, model, max_tokens, temperature
           FROM ai_provider_configs
           WHERE user_id = $1 AND is_active = true AND is_default = true
           LIMIT 1"#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    if let Some(row) = row {
        let provider_str = row.get::<String, _>("provider");
        let api_key = row.get::<String, _>("api_key_encrypted");
        let base_url = row.get::<Option<String>, _>("base_url");
        let model = row.get::<Option<String>, _>("model");
        let max_tokens = row.get::<Option<i32>, _>("max_tokens");
        let temperature = row.get::<Option<f64>, _>("temperature");

        let provider = match provider_str.to_lowercase().as_str() {
            "deepseek" => LlmProvider::DeepSeek,
            "anthropic" => LlmProvider::Anthropic,
            "custom" => LlmProvider::Custom,
            _ => LlmProvider::OpenAI,
        };

        let default_url = match &provider {
            LlmProvider::OpenAI => "https://api.openai.com/v1".to_string(),
            LlmProvider::DeepSeek => "https://api.deepseek.com/v1".to_string(),
            LlmProvider::Anthropic => "https://api.anthropic.com/v1".to_string(),
            LlmProvider::Custom => "http://localhost:11434/v1".to_string(),
        };

        let default_model = match &provider {
            LlmProvider::OpenAI => "gpt-4o-mini".to_string(),
            LlmProvider::DeepSeek => "deepseek-chat".to_string(),
            LlmProvider::Anthropic => "claude-3-haiku-20240307".to_string(),
            LlmProvider::Custom => "local-model".to_string(),
        };

        if api_key.is_empty() && provider != LlmProvider::Custom {
            debug!("User {} has DB config but no API key, skipping", user_id);
            return Ok(None);
        }

        let config = LlmConfig {
            provider,
            api_key,
            base_url: base_url.unwrap_or(default_url),
            model: model.unwrap_or(default_model),
            max_tokens: max_tokens.unwrap_or(2048),
            temperature: temperature.unwrap_or(0.7),
        };

        let client = LlmClient::new(config);
        return Ok(Some(client));
    }

    Ok(None)
}

fn get_llm_client_from_env() -> Option<LlmClient> {
    match LlmClient::from_env() {
        Ok(client) => {
            if client.is_configured() {
                Some(client)
            } else {
                debug!("LLM client from env not configured (missing API key)");
                None
            }
        }
        Err(e) => {
            debug!("Failed to create LLM client from env: {}", e);
            None
        }
    }
}

async fn analyze_with_llm(
    pool: &sqlx::PgPool,
    user_id: i64,
    system_prompt: &str,
    user_message: &str,
    agent_name: &str,
) -> Option<String> {
    let client = match get_llm_client_from_db(pool, user_id).await {
        Ok(Some(client)) => {
            debug!("Using LLM client from user DB config for user {}", user_id);
            client
        }
        Ok(None) => {
            debug!("No DB config for user {}, falling back to env config", user_id);
            match get_llm_client_from_env() {
                Some(client) => client,
                None => {
                    warn!("No LLM configuration available (neither DB nor env)");
                    return None;
                }
            }
        }
        Err(e) => {
            warn!("Failed to get LLM client from DB: {}, falling back to env", e);
            match get_llm_client_from_env() {
                Some(client) => client,
                None => {
                    warn!("No LLM configuration available (neither DB nor env)");
                    return None;
                }
            }
        }
    };

    match client.chat_with_system(system_prompt, user_message).await {
        Ok(response) => {
            record_llm_usage(pool, user_id, agent_name, &client).await;
            Some(response)
        }
        Err(e) => {
            warn!("LLM chat failed: {}", e);
            None
        }
    }
}

async fn record_llm_usage(pool: &sqlx::PgPool, user_id: i64, agent_name: &str, client: &LlmClient) {
    let config = client.config();
    let result = sqlx::query(
        r#"INSERT INTO llm_usage_logs (provider, model, prompt_tokens, completion_tokens, total_tokens, agent_name, user_id, created_at)
           VALUES ($1, $2, 0, 0, 0, $3, $4, NOW())"#,
    )
    .bind(format!("{:?}", config.provider))
    .bind(&config.model)
    .bind(agent_name)
    .bind(user_id)
    .execute(pool)
    .await;

    if let Err(e) = result {
        debug!("Failed to record LLM usage log: {}", e);
    }
}

async fn technical_analysis(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    analyze_technical(user, State(state), Json(req)).await
}

async fn funding_analysis(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    analyze_funding(user, State(state), Json(req)).await
}

async fn sentiment_analysis(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    analyze_sentiment(user, State(state), Json(req)).await
}

async fn comprehensive_analysis(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    analyze_comprehensive(user, State(state), Json(req)).await
}

async fn get_usage(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Value>> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM ai_prediction_trades WHERE user_id = $1 AND created_at > NOW() - INTERVAL '24 hours'"#,
    )
    .bind(user.user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(json!({"daily_usage": count, "limit": 100})))
}

async fn reset_usage(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Value>> {
    Ok(Json(json!({"message": "Usage counter reset"})))
}

async fn generate_report(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<Value>> {
    let report = sqlx::query(
        r#"INSERT INTO reports (user_id, title, content, format, status) VALUES ($1, $2, $3, 'markdown', 'generated') RETURNING id, title"#,
    )
    .bind(user.user_id)
    .bind(format!("AI Analysis Report - {}", req.symbol))
    .bind(format!("Automated analysis report for {}", req.symbol))
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(json!({"report_id": report.get::<i32, _>("id"), "title": report.get::<String, _>("title")})))
}

// ==================== Debate Session Endpoints ====================

#[derive(Debug, Deserialize)]
struct DebateRequest {
    symbol: String,
    interval: Option<String>,
}

struct AgentDef {
    id: &'static str,
    name: &'static str,
    department: &'static str,
    role: &'static str,
    personality: &'static str,
}

const AGENTS: &[AgentDef] = &[
    AgentDef { id: "tech_bull", name: "技术分析师A", department: "technical", role: "看多技术分析师", personality: "擅长发现技术面看多信号，关注支撑位突破、均线金叉、RSI超卖反弹" },
    AgentDef { id: "tech_bear", name: "技术分析师B", department: "technical", role: "看空技术分析师", personality: "擅长发现技术面看空信号，关注阻力位压制、均线死叉、RSI超买回落" },
    AgentDef { id: "capital_bull", name: "资金分析师A", department: "capital", role: "看多资金分析师", personality: "擅长发现资金面看多信号，关注资金流入、多头增仓、资金费率偏低" },
    AgentDef { id: "capital_bear", name: "资金分析师B", department: "capital", role: "看空资金分析师", personality: "擅长发现资金面看空信号，关注资金流出、空头增仓、资金费率偏高" },
    AgentDef { id: "news_bull", name: "新闻分析师A", department: "news", role: "看多新闻分析师", personality: "擅长发现消息面看多信号，关注利好政策、行业合作、市场情绪回暖" },
    AgentDef { id: "news_bear", name: "新闻分析师B", department: "news", role: "看空新闻分析师", personality: "擅长发现消息面看空信号，关注监管风险、安全事件、市场恐慌情绪" },
];

async fn start_debate(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<DebateRequest>,
) -> Result<Json<Value>> {
    let symbol = req.symbol.clone();
    let interval = req.interval.unwrap_or_else(|| "1H".to_string());

    // 1. Get OKX client for real market data
    let okx_client = get_okx_client(&state, user.user_id).await?;

    // 2. Fetch real market data from OKX
    let ticker = okx_client.get_ticker(&symbol).await
        .map_err(|e| AppError::Internal(format!("Failed to fetch ticker: {}", e)))?;

    let candles = okx_client.get_candles(&symbol, &interval, Some(100)).await
        .map_err(|e| AppError::Internal(format!("Failed to fetch candles: {}", e)))?;

    // Fetch funding rate via get_raw (non-blocking, fallback to default)
    let funding_data = match okx_client.get_raw(
        "/api/v5/public/funding-rate",
        Some(&[("instId", symbol.clone())]),
    ).await {
        Ok(data) => data,
        Err(e) => {
            tracing::warn!("Failed to fetch funding rate, using default: {}", e);
            serde_json::json!({})
        }
    };

    // Fetch long-short ratio via get_raw
    // OKX API: /api/v5/rubik/stat/contracts/long-short-account-ratio?ccy=BTC&period=5m
    // ccy = currency code extracted from symbol (e.g. "DOGE-USDT-SWAP" -> "DOGE")
    let ccy = symbol.split('-').next().unwrap_or(&symbol).to_string();
    let long_short_data = match okx_client.get_raw(
        "/api/v5/rubik/stat/contracts/long-short-account-ratio",
        Some(&[("ccy", ccy), ("period", "5m".to_string())]),
    ).await {
        Ok(data) => data,
        Err(e) => {
            tracing::warn!("Failed to fetch long-short ratio: {}", e);
            serde_json::json!({})
        }
    };

    // Extract market data values
    let current_price: f64 = ticker.last.as_ref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let open_24h: f64 = ticker.open_24h.as_ref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let high_24h: f64 = ticker.high_24h.as_ref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let low_24h: f64 = ticker.low_24h.as_ref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let vol_24h: f64 = ticker.vol_24h.as_ref().and_then(|v| v.parse().ok()).unwrap_or(0.0);

    let funding_rate = funding_data
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|item| item.get("fundingRate"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let long_short_ratio = long_short_data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.as_array())
        .and_then(|pair| pair.get(1))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| long_short_data.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|item| item.as_array()).and_then(|pair| pair.get(1)).and_then(|v| v.as_f64()))
        .unwrap_or(1.0);

    // Build candle summary for prompts
    let recent_candles: Vec<Value> = candles.iter().take(20).map(|c| json!({
        "open": c.o,
        "high": c.h,
        "low": c.l,
        "close": c.c,
        "volume": c.vol,
    })).collect();

    let market_data_str = format!(
        "## 实时市场数据 (来源: OKX)\n\n\
        ### 行情概览\n\
        交易对: {}\n\
        当前价格: {:.2}\n\
        24h开盘: {:.2}\n\
        24h最高: {:.2}\n\
        24h最低: {:.2}\n\
        24h涨跌: {:.2}%\n\
        24h成交量: {:.2}\n\n\
        ### 资金费率\n\
        当前资金费率: {:.6}\n\n\
        ### 多空比\n\
        多空账户比: {:.4}\n\n\
        ### 最近K线数据 (周期: {})\n\
        {}\n",
        symbol,
        current_price,
        open_24h,
        high_24h,
        low_24h,
        if open_24h > 0.0 { (current_price - open_24h) / open_24h * 100.0 } else { 0.0 },
        vol_24h,
        funding_rate,
        long_short_ratio,
        interval,
        serde_json::to_string_pretty(&recent_candles).unwrap_or_default(),
    );

    // 3. Create debate session in DB
    let session_row = sqlx::query(
        r#"INSERT INTO debate_sessions (user_id, symbol, status, progress, agent_opinions, department_reports, fund_manager_decision, created_at, updated_at)
        VALUES ($1, $2, 'in_progress', 'fetching_market_data', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user.user_id)
    .bind(&symbol)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let session_id: Uuid = session_row.get::<Uuid, _>("id");

    // Update progress
    let _ = sqlx::query(
        r#"UPDATE debate_sessions SET progress = 'analyzing_agents', updated_at = NOW() WHERE id = $1"#,
    )
    .bind(session_id)
    .execute(&state.db_pool)
    .await;

    // 4. Call LLM for each of 6 agents
    let mut agent_opinions: Vec<Value> = Vec::new();

    for agent_def in AGENTS {
        let system_prompt = format!(
            "你是{}的{}，名叫{}。你的性格特点是：{}。\n\
            你需要基于提供的OKX实时市场数据，从你的专业角度进行分析。\n\
            你必须以JSON格式回复，格式如下：\n\
            {{\"sentiment\": \"bullish\"|\"bearish\"|\"neutral\"|\"cautious\", \"confidence\": 0.0-1.0, \"analysis\": \"你的详细分析\", \"key_factors\": [\"因素1\", \"因素2\"]}}\n\
            sentiment必须是bullish(看多)、bearish(看空)、neutral(中性)、cautious(谨慎)之一。\n\
            confidence必须是0到1之间的数字，表示你的信心程度。\n\
            只输出JSON，不要输出其他内容。",
            match agent_def.department {
                "technical" => "技术分析部",
                "capital" => "资金分析部",
                "news" => "新闻分析部",
                _ => "分析部",
            },
            agent_def.role,
            agent_def.name,
            agent_def.personality,
        );

        let llm_result = analyze_with_llm(
            &state.db_pool,
            user.user_id,
            &system_prompt,
            &market_data_str,
            agent_def.id,
        )
        .await;

        let opinion = match llm_result {
            Some(response) => {
                let parsed = parse_agent_json_response(&response);
                json!({
                    "agent_id": agent_def.id,
                    "agent_name": agent_def.name,
                    "department": agent_def.department,
                    "sentiment": parsed.sentiment,
                    "confidence": parsed.confidence,
                    "analysis": parsed.analysis,
                    "key_factors": parsed.key_factors,
                    "source": "llm",
                })
            }
            None => {
                json!({
                    "agent_id": agent_def.id,
                    "agent_name": agent_def.name,
                    "department": agent_def.department,
                    "sentiment": "neutral",
                    "confidence": 0.3,
                    "analysis": "LLM不可用，无法生成分析",
                    "key_factors": [],
                    "source": "llm_unavailable",
                })
            }
        };

        agent_opinions.push(opinion);
    }

    // Update progress
    let _ = sqlx::query(
        r#"UPDATE debate_sessions SET progress = 'generating_reports', agent_opinions = $2, updated_at = NOW() WHERE id = $1"#,
    )
    .bind(session_id)
    .bind(serde_json::to_value(&agent_opinions).unwrap_or(json!([])))
    .execute(&state.db_pool)
    .await;

    // 5. Call LLM for each department report
    let mut department_reports: Vec<Value> = Vec::new();

    for dept in &["technical", "capital", "news"] {
        let dept_name = match *dept {
            "technical" => "技术分析部",
            "capital" => "资金分析部",
            "news" => "新闻分析部",
            _ => "分析部",
        };

        let dept_opinions: Vec<&Value> = agent_opinions.iter()
            .filter(|o| o.get("department").and_then(|v| v.as_str()) == Some(*dept))
            .collect();

        let opinions_str = serde_json::to_string_pretty(&dept_opinions)
            .unwrap_or_default();

        let system_prompt = format!(
            "你是{}的部门汇总分析师。你需要综合部门内各分析师的意见，给出部门汇总报告。\n\
            你必须以JSON格式回复，格式如下：\n\
            {{\"consensus\": \"bullish\"|\"bearish\"|\"neutral\", \"bull_summary\": \"看多理由汇总\", \"bear_summary\": \"看空理由汇总\"}}\n\
            只输出JSON，不要输出其他内容。",
            dept_name,
        );

        let user_message = format!(
            "## {} 分析师意见\n\n{}\n\n\
            请综合以上分析师意见，给出部门汇总报告。",
            dept_name, opinions_str,
        );

        let llm_result = analyze_with_llm(
            &state.db_pool,
            user.user_id,
            &system_prompt,
            &user_message,
            &format!("{}_dept_report", dept),
        )
        .await;

        let report = match llm_result {
            Some(response) => {
                let parsed = parse_dept_report_response(&response);
                json!({
                    "department": dept,
                    "consensus": parsed.consensus,
                    "bull_summary": parsed.bull_summary,
                    "bear_summary": parsed.bear_summary,
                })
            }
            None => {
                // Fallback: derive from agent opinions
                let sentiments: Vec<&str> = dept_opinions.iter()
                    .filter_map(|o| o.get("sentiment").and_then(|v| v.as_str()))
                    .collect();
                let bull_count = sentiments.iter().filter(|&&s| s == "bullish").count();
                let bear_count = sentiments.iter().filter(|&&s| s == "bearish").count();
                let consensus = if bull_count > bear_count { "bullish" }
                    else if bear_count > bull_count { "bearish" }
                    else { "neutral" };

                json!({
                    "department": dept,
                    "consensus": consensus,
                    "bull_summary": "LLM不可用，基于分析师多数意见汇总",
                    "bear_summary": "LLM不可用，基于分析师多数意见汇总",
                })
            }
        };

        department_reports.push(report);
    }

    // Update progress
    let _ = sqlx::query(
        r#"UPDATE debate_sessions SET progress = 'fund_manager_deciding', department_reports = $2, updated_at = NOW() WHERE id = $1"#,
    )
    .bind(session_id)
    .bind(serde_json::to_value(&department_reports).unwrap_or(json!([])))
    .execute(&state.db_pool)
    .await;

    // 6. Call LLM for fund manager decision
    let all_opinions_str = serde_json::to_string_pretty(&agent_opinions).unwrap_or_default();
    let all_reports_str = serde_json::to_string_pretty(&department_reports).unwrap_or_default();

    let fund_manager_system_prompt = format!(
        "你是基金经理，负责综合各部门的分析报告，做出最终交易决策。\n\
        你需要基于以下信息做出决策：\n\
        1. 各分析师的意见和信心度\n\
        2. 各部门的汇总报告\n\
        3. 当前市场价格: {:.2}\n\n\
        你必须以JSON格式回复，格式如下：\n\
        {{\"action\": \"long\"|\"short\"|\"hold\", \"confidence\": 0.0-1.0, \"entry_range\": {{\"low\": 价格, \"high\": 价格}}, \"stop_loss\": 价格, \"take_profit\": [价格1, 价格2], \"leverage\": 1-10, \"reasoning\": \"决策理由\"}}\n\
        只输出JSON，不要输出其他内容。",
        current_price,
    );

    let fund_manager_message = format!(
        "## 交易对: {}\n\n\
        ## 各分析师意见\n{}\n\n\
        ## 各部门汇总报告\n{}\n\n\
        请综合以上信息，做出最终交易决策。",
        symbol, all_opinions_str, all_reports_str,
    );

    let fund_manager_result = analyze_with_llm(
        &state.db_pool,
        user.user_id,
        &fund_manager_system_prompt,
        &fund_manager_message,
        "fund_manager",
    )
    .await;

    let fund_manager_decision = match fund_manager_result {
        Some(response) => {
            let parsed = parse_fund_manager_response(&response, current_price);
            json!({
                "action": parsed.action,
                "confidence": parsed.confidence,
                "entry_range": parsed.entry_range,
                "stop_loss": parsed.stop_loss,
                "take_profit": parsed.take_profit,
                "leverage": parsed.leverage,
                "reasoning": parsed.reasoning,
            })
        }
        None => {
            // Fallback: derive from department reports
            let dept_consensuses: Vec<&str> = department_reports.iter()
                .filter_map(|r| r.get("consensus").and_then(|v| v.as_str()))
                .collect();
            let bull_count = dept_consensuses.iter().filter(|&&c| c == "bullish").count();
            let bear_count = dept_consensuses.iter().filter(|&&c| c == "bearish").count();
            let action = if bull_count > bear_count { "long" }
                else if bear_count > bull_count { "short" }
                else { "hold" };

            json!({
                "action": action,
                "confidence": 0.4,
                "entry_range": {"low": current_price * 0.99, "high": current_price * 1.01},
                "stop_loss": current_price * 0.97,
                "take_profit": [current_price * 1.03, current_price * 1.05],
                "leverage": 2,
                "reasoning": "LLM不可用，基于部门多数意见决策",
            })
        }
    };

    // 7. Update the debate session with all results
    sqlx::query(
        r#"UPDATE debate_sessions SET
            status = 'completed',
            progress = 'completed',
            agent_opinions = $2,
            department_reports = $3,
            fund_manager_decision = $4,
            updated_at = NOW()
        WHERE id = $1"#,
    )
    .bind(session_id)
    .bind(serde_json::to_value(&agent_opinions).unwrap_or(json!([])))
    .bind(serde_json::to_value(&department_reports).unwrap_or(json!([])))
    .bind(&fund_manager_decision)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    // 8. Return the complete debate session
    Ok(Json(json!({
        "session_id": session_id.to_string(),
        "symbol": symbol,
        "status": "completed",
        "progress": "completed",
        "agent_opinions": agent_opinions,
        "department_reports": department_reports,
        "fund_manager_decision": fund_manager_decision,
    })))
}

struct AgentParsedResponse {
    sentiment: String,
    confidence: f64,
    analysis: String,
    key_factors: Vec<String>,
}

fn parse_agent_json_response(response: &str) -> AgentParsedResponse {
    let cleaned = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match serde_json::from_str::<Value>(cleaned) {
        Ok(parsed) => {
            let sentiment = parsed
                .get("sentiment")
                .and_then(|v| v.as_str())
                .unwrap_or("neutral")
                .to_string();

            let confidence = parsed
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5)
                .clamp(0.0, 1.0);

            let analysis = parsed
                .get("analysis")
                .and_then(|v| v.as_str())
                .unwrap_or("分析结果解析失败")
                .to_string();

            let key_factors = parsed
                .get("key_factors")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            AgentParsedResponse {
                sentiment,
                confidence,
                analysis,
                key_factors,
            }
        }
        Err(_) => AgentParsedResponse {
            sentiment: "neutral".to_string(),
            confidence: 0.3,
            analysis: response.to_string(),
            key_factors: vec![],
        },
    }
}

struct DeptReportParsed {
    consensus: String,
    bull_summary: String,
    bear_summary: String,
}

fn parse_dept_report_response(response: &str) -> DeptReportParsed {
    let cleaned = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match serde_json::from_str::<Value>(cleaned) {
        Ok(parsed) => {
            let consensus = parsed
                .get("consensus")
                .and_then(|v| v.as_str())
                .unwrap_or("neutral")
                .to_string();

            let bull_summary = parsed
                .get("bull_summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let bear_summary = parsed
                .get("bear_summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            DeptReportParsed {
                consensus,
                bull_summary,
                bear_summary,
            }
        }
        Err(_) => DeptReportParsed {
            consensus: "neutral".to_string(),
            bull_summary: String::new(),
            bear_summary: String::new(),
        },
    }
}

struct FundManagerParsed {
    action: String,
    confidence: f64,
    entry_range: Value,
    stop_loss: f64,
    take_profit: Vec<f64>,
    leverage: i32,
    reasoning: String,
}

fn parse_fund_manager_response(response: &str, current_price: f64) -> FundManagerParsed {
    let cleaned = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match serde_json::from_str::<Value>(cleaned) {
        Ok(parsed) => {
            let action = parsed
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("hold")
                .to_string();

            let confidence = parsed
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5)
                .clamp(0.0, 1.0);

            let entry_range = parsed
                .get("entry_range")
                .cloned()
                .unwrap_or_else(|| json!({"low": current_price * 0.99, "high": current_price * 1.01}));

            let stop_loss = parsed
                .get("stop_loss")
                .and_then(|v| v.as_f64())
                .unwrap_or(current_price * 0.97);

            let take_profit = parsed
                .get("take_profit")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_f64())
                        .collect()
                })
                .unwrap_or_else(|| vec![current_price * 1.03, current_price * 1.05]);

            let leverage = parsed
                .get("leverage")
                .and_then(|v| v.as_i64())
                .unwrap_or(2) as i32;

            let reasoning = parsed
                .get("reasoning")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            FundManagerParsed {
                action,
                confidence,
                entry_range,
                stop_loss,
                take_profit,
                leverage,
                reasoning,
            }
        }
        Err(_) => FundManagerParsed {
            action: "hold".to_string(),
            confidence: 0.3,
            entry_range: json!({"low": current_price * 0.99, "high": current_price * 1.01}),
            stop_loss: current_price * 0.97,
            take_profit: vec![current_price * 1.03, current_price * 1.05],
            leverage: 2,
            reasoning: response.to_string(),
        },
    }
}

async fn get_debate_session(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row = sqlx::query(
        r#"SELECT id, user_id, symbol, status, progress, agent_opinions, department_reports, fund_manager_decision, created_at::text, updated_at::text
        FROM debate_sessions WHERE id = $1 AND user_id = $2"#,
    )
    .bind(session_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let row = row.ok_or_else(|| AppError::NotFound("Debate session not found".to_string()))?;

    Ok(Json(json!({
        "session_id": row.get::<Uuid, _>("id").to_string(),
        "symbol": row.try_get::<String, _>("symbol").unwrap_or_default(),
        "status": row.try_get::<String, _>("status").unwrap_or_default(),
        "progress": row.try_get::<String, _>("progress").unwrap_or_default(),
        "agent_opinions": row.try_get::<Value, _>("agent_opinions").unwrap_or(json!([])),
        "department_reports": row.try_get::<Value, _>("department_reports").unwrap_or(json!([])),
        "fund_manager_decision": row.try_get::<Value, _>("fund_manager_decision").unwrap_or(json!({})),
        "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
        "updated_at": row.try_get::<String, _>("updated_at").unwrap_or_default(),
    })))
}

async fn list_debate_sessions(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        r#"SELECT id, symbol, status, progress, created_at::text, updated_at::text
        FROM debate_sessions WHERE user_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<Value> = rows.iter().map(|row| json!({
        "session_id": row.get::<Uuid, _>("id").to_string(),
        "symbol": row.try_get::<String, _>("symbol").unwrap_or_default(),
        "status": row.try_get::<String, _>("status").unwrap_or_default(),
        "progress": row.try_get::<String, _>("progress").unwrap_or_default(),
        "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
        "updated_at": row.try_get::<String, _>("updated_at").unwrap_or_default(),
    })).collect();

    Ok(Json(json!({"items": items})))
}
