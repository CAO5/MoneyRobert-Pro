use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::convert::Infallible;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::agents::config::AgentConfig;
use crate::agents::llm_client::{LlmClient, LlmConfig, LlmProvider};
use crate::agents::market::{DatabaseMarketDataProvider, MarketDataProvider};
use crate::utils::encryption::decrypt;
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
        .route("/debate", post(start_debate_stream))
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
        当前价格: {:.6}\n\
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
        Ok(response) => {
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
        Err(e) => {
            warn!("LLM unavailable for comprehensive analysis: {}, falling back to rule-based analysis", e);

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
        let encrypted_key = row.get::<String, _>("api_key_encrypted");
        let api_key = decrypt(&encrypted_key)?;
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

        // Read proxy config from DB for real-time proxy support
        let proxy_url = crate::state::get_proxy_config_from_db(pool).await;
        let client = LlmClient::new_with_proxy(config, proxy_url);
        return Ok(Some(client));
    }

    Ok(None)
}

fn get_llm_client_from_env() -> Option<LlmClient> {
    get_llm_client_from_env_with_proxy(None)
}

fn get_llm_client_from_env_with_proxy(proxy_url: Option<&str>) -> Option<LlmClient> {
    match LlmClient::from_env() {
        Ok(client) => {
            if client.is_configured() {
                // Rebuild with proxy from DB if available
                if let Some(url) = proxy_url {
                    let config = client.config().clone();
                    Some(LlmClient::new_with_proxy(config, Some(url.to_string())))
                } else {
                    Some(client)
                }
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
) -> Result<String> {
    // Read proxy config from DB once for this request
    let proxy_url = crate::state::get_proxy_config_from_db(pool).await;

    let client = match get_llm_client_from_db(pool, user_id).await {
        Ok(Some(client)) => {
            debug!("Using LLM client from user DB config for user {}", user_id);
            client
        }
        Ok(None) => {
            debug!("No DB config for user {}, falling back to env config", user_id);
            match get_llm_client_from_env_with_proxy(proxy_url.as_deref()) {
                Some(client) => client,
                None => {
                    return Err(AppError::Validation(
                        "未配置 AI 模型，请在系统设置中配置 API Key（支持 OpenAI/DeepSeek/Anthropic）".to_string()
                    ));
                }
            }
        }
        Err(e) => {
            warn!("Failed to get LLM client from DB: {}, falling back to env", e);
            match get_llm_client_from_env_with_proxy(proxy_url.as_deref()) {
                Some(client) => client,
                None => {
                    return Err(AppError::Validation(
                        "未配置 AI 模型，请在系统设置中配置 API Key（支持 OpenAI/DeepSeek/Anthropic）".to_string()
                    ));
                }
            }
        }
    };

    match client.chat_with_system(system_prompt, user_message).await {
        Ok(response) => {
            record_llm_usage(pool, user_id, agent_name, &client).await;
            Ok(response)
        }
        Err(e) => {
            warn!("LLM chat failed for agent {}: {}", agent_name, e);
            let err_msg = format!("{}", e);
            let user_msg = if err_msg.contains("401") || err_msg.contains("Authentication") || err_msg.contains("invalid") {
                "AI 模型认证失败，API Key 无效或已过期，请在系统设置中重新配置".to_string()
            } else if err_msg.contains("403") {
                "AI 模型访问被拒绝，请检查 API Key 权限".to_string()
            } else if err_msg.contains("429") {
                "AI 模型请求过于频繁，请稍后重试".to_string()
            } else if err_msg.contains("timeout") || err_msg.contains("TimedOut") {
                "AI 模型请求超时，请检查网络连接".to_string()
            } else {
                format!("AI 模型调用失败: {}。请检查 API Key 和网络配置", e)
            };
            Err(AppError::Validation(user_msg))
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
    // Reset usage by deleting AI prediction trades older than 24 hours for this user
    let deleted = sqlx::query(
        r#"DELETE FROM ai_predictions
           WHERE user_id = $1 AND created_at < NOW() - INTERVAL '24 hours'"#,
    )
    .bind(user.user_id)
    .execute(&state.db_pool)
    .await?;

    Ok(Json(json!({
        "message": "Usage counter reset",
        "deleted_count": deleted.rows_affected()
    })))
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
    AgentDef { id: "tech_bull", name: "技术分析师A", department: "technical", role: "技术分析师", personality: "擅长识别趋势突破、支撑阻力位、均线系统和量价关系，客观判断技术面方向" },
    AgentDef { id: "tech_bear", name: "技术分析师B", department: "technical", role: "技术分析师", personality: "擅长识别超买超卖、背离信号、形态破位和波动率异常，客观判断技术面风险" },
    AgentDef { id: "capital_bull", name: "资金分析师A", department: "capital", role: "资金分析师", personality: "擅长分析资金流入流出、持仓变化、订单簿深度和买盘力量，客观评估资金面" },
    AgentDef { id: "capital_bear", name: "资金分析师B", department: "capital", role: "资金分析师", personality: "擅长分析资金费率极端值、多空比拥挤度、杠杆率和清算风险，客观评估资金面风险" },
    AgentDef { id: "news_bull", name: "新闻分析师A", department: "news", role: "新闻分析师", personality: "擅长识别利好催化剂、行业合作和市场情绪回暖信号，客观评估消息面" },
    AgentDef { id: "news_bear", name: "新闻分析师B", department: "news", role: "新闻分析师", personality: "擅长识别监管风险、安全事件和系统性风险信号，客观评估消息面风险" },
];

async fn start_debate_old(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<DebateRequest>,
) -> Result<Json<Value>> {
    let symbol = req.symbol.clone();
    let interval = req.interval.unwrap_or_else(|| "1H".to_string());

    // 1. Get OKX client for real market data
    let okx_client = get_okx_client(&state, user.user_id).await?;

    // 2. Fetch real market data from OKX
    let ticker = match okx_client.get_ticker(&symbol).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Failed to fetch ticker for debate: {}", e);
            return Err(AppError::Validation(format!(
                "无法连接 OKX 获取行情数据，请检查网络代理配置是否正确。错误详情: {}",
                e
            )));
        }
    };

    let candles = match okx_client.get_candles(&symbol, &interval, Some(100)).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to fetch candles for debate: {}", e);
            return Err(AppError::Validation(format!(
                "无法连接 OKX 获取K线数据，请检查网络代理配置是否正确。错误详情: {}",
                e
            )));
        }
    };

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

    let ls_result = parse_long_short_ratio(&long_short_data);
    let long_short_ratio = ls_result.long_short_ratio;
    let long_pct = ls_result.long_pct;
    let short_pct = ls_result.short_pct;

    // Build candle summary for prompts
    let recent_candles: Vec<Value> = candles.iter().take(20).rev().map(|c| {
        let ts = c.ts.as_deref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        json!({
            "time": ts,
            "open": c.o,
            "high": c.h,
            "low": c.l,
            "close": c.c,
            "volume": c.vol,
        })
    }).collect();

    let market_data_str = format!(
        "## 实时市场数据 (来源: OKX)\n\n\
        ### 行情概览\n\
        交易对: {}\n\
        当前价格: {:.6}\n\
        24h开盘: {:.6}\n\
        24h最高: {:.6}\n\
        24h最低: {:.6}\n\
        24h涨跌: {:.4}%\n\
        24h成交量: {:.4}\n\n\
        ### 资金费率\n\
        当前资金费率: {:.8}\n\n\
        ### 多空比\n\
        多空账户比: {}\n\
        多头占比: {}\n\
        空头占比: {}\n\n\
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
        long_short_ratio.map(|r| format!("{:.4}", r)).unwrap_or_else(|| "数据不可用".to_string()),
        long_pct.map(|p| format!("{:.2}%", p)).unwrap_or_else(|| "数据不可用".to_string()),
        short_pct.map(|p| format!("{:.2}%", p)).unwrap_or_else(|| "数据不可用".to_string()),
        interval,
        serde_json::to_string_pretty(&recent_candles).unwrap_or_default(),
    );

    // 3. Create debate session in DB with market snapshot for auditing
    let market_snapshot = json!({
        "symbol": symbol,
        "current_price": current_price,
        "open_24h": open_24h,
        "high_24h": high_24h,
        "low_24h": low_24h,
        "vol_24h": vol_24h,
        "funding_rate": funding_rate,
        "long_short_ratio": long_short_ratio,
        "long_pct": long_pct,
        "short_pct": short_pct,
        "candles_count": candles.len(),
        "data_source": "okx_realtime",
    });

    let session_row = sqlx::query(
        r#"INSERT INTO debate_sessions (user_id, symbol, status, progress, agent_opinions, department_reports, fund_manager_decision, market_snapshot, created_at, updated_at)
        VALUES ($1, $2, 'in_progress', 'fetching_market_data', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb, $3, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user.user_id)
    .bind(&symbol)
    .bind(&market_snapshot)
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
            "你是{}的分析师，名叫{}。你的分析视角：{}。\n\
            你需要基于提供的OKX实时市场数据，从你的专业角度进行客观分析。\n\
            关键：你是分析师而非辩手，目标是给出最准确的判断，而非捍卫某个方向。\n\
            如果数据不支持你通常关注的信号方向，你应该如实报告。\n\
            当数据信号不明确时，给出neutral而非强行选择方向。\n\
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
            Ok(response) => {
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
            Err(e) => {
                // First agent failure returns error to user immediately
                if agent_opinions.is_empty() {
                    return Err(e);
                }
                json!({
                    "agent_id": agent_def.id,
                    "agent_name": agent_def.name,
                    "department": agent_def.department,
                    "sentiment": "neutral",
                    "confidence": 0.3,
                    "analysis": format!("LLM调用失败: {}", e),
                    "key_factors": [],
                    "source": "llm_error",
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
            Ok(response) => {
                let parsed = parse_dept_report_response(&response);
                json!({
                    "department": dept,
                    "consensus": parsed.consensus,
                    "bull_summary": parsed.bull_summary,
                    "bear_summary": parsed.bear_summary,
                })
            }
            Err(_) => {
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
        3. 当前市场价格: {:.6}\n\n\
        重要：推理中必须使用精确价格（如0.084740而非0.08），不得简化或四舍五入价格，否则会导致错误的支撑/阻力判断。\n\n\
        决策原则：\n\
        - 做多和做空应该有同等门槛，不要因为'避险'而偏向做空\n\
        - 如果多空信号势均力敌，选择hold比强行选方向更合理\n\
        - 多空比极端值需要结合趋势方向判断，不能简单认为'拥挤=反转'\n\
        - 分析师给出neutral时，代表数据不明确，不应被忽略\n\
        - 不要将'谨慎'等同于'看空'\n\n\
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
        Ok(response) => {
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
        Err(_) => {
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

async fn start_debate_stream(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<DebateRequest>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>> {
    let symbol = req.symbol.clone();
    let interval = req.interval.clone().unwrap_or_else(|| "1H".to_string());

    // 1. Get OKX client for real market data (before streaming starts)
    let okx_client = get_okx_client(&state, user.user_id).await?;

    // 2. Fetch real market data from OKX (before streaming starts)
    let ticker = match okx_client.get_ticker(&symbol).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Failed to fetch ticker for debate: {}", e);
            return Err(AppError::Validation(format!(
                "无法连接 OKX 获取行情数据，请检查网络代理配置是否正确。错误详情: {}",
                e
            )));
        }
    };

    let candles = match okx_client.get_candles(&symbol, &interval, Some(100)).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to fetch candles for debate: {}", e);
            return Err(AppError::Validation(format!(
                "无法连接 OKX 获取K线数据，请检查网络代理配置是否正确。错误详情: {}",
                e
            )));
        }
    };

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

    let ls_result = parse_long_short_ratio(&long_short_data);
    let long_short_ratio = ls_result.long_short_ratio;
    let long_pct = ls_result.long_pct;
    let short_pct = ls_result.short_pct;

    let recent_candles: Vec<Value> = candles.iter().take(20).rev().map(|c| {
        let ts = c.ts.as_deref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        json!({
            "time": ts,
            "open": c.o,
            "high": c.h,
            "low": c.l,
            "close": c.c,
            "volume": c.vol,
        })
    }).collect();

    let market_data_str = format!(
        "## 实时市场数据 (来源: OKX)\n\n\
        ### 行情概览\n\
        交易对: {}\n\
        当前价格: {:.6}\n\
        24h开盘: {:.6}\n\
        24h最高: {:.6}\n\
        24h最低: {:.6}\n\
        24h涨跌: {:.4}%\n\
        24h成交量: {:.4}\n\n\
        ### 资金费率\n\
        当前资金费率: {:.8}\n\n\
        ### 多空比\n\
        多空账户比: {}\n\
        多头占比: {}\n\
        空头占比: {}\n\n\
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
        long_short_ratio.map(|r| format!("{:.4}", r)).unwrap_or_else(|| "数据不可用".to_string()),
        long_pct.map(|p| format!("{:.2}%", p)).unwrap_or_else(|| "数据不可用".to_string()),
        short_pct.map(|p| format!("{:.2}%", p)).unwrap_or_else(|| "数据不可用".to_string()),
        interval,
        serde_json::to_string_pretty(&recent_candles).unwrap_or_default(),
    );

    // 3. Create debate session in DB with market snapshot for auditing
    let market_snapshot = json!({
        "symbol": symbol,
        "current_price": current_price,
        "open_24h": open_24h,
        "high_24h": high_24h,
        "low_24h": low_24h,
        "vol_24h": vol_24h,
        "funding_rate": funding_rate,
        "long_short_ratio": long_short_ratio,
        "long_pct": long_pct,
        "short_pct": short_pct,
        "candles_count": candles.len(),
        "data_source": "okx_realtime",
    });

    let session_row = sqlx::query(
        r#"INSERT INTO debate_sessions (user_id, symbol, status, progress, agent_opinions, department_reports, fund_manager_decision, market_snapshot, created_at, updated_at)
        VALUES ($1, $2, 'in_progress', 'fetching_market_data', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb, $3, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user.user_id)
    .bind(&symbol)
    .bind(&market_snapshot)
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

    // 4. Build SSE stream using mpsc channel
    let (tx, rx) = tokio::sync::mpsc::channel::<std::result::Result<Event, Infallible>>(32);

    // Clone everything needed for the spawned task
    let pool = state.db_pool.clone();
    let user_id = user.user_id;
    let session_id_str = session_id.to_string();
    let symbol_for_task = symbol.clone();

    // Send initial events before spawning
    let market_event = Event::default().data(serde_json::to_string(&json!({
        "type": "market_data",
        "price": current_price,
        "open_24h": open_24h,
        "high_24h": high_24h,
        "low_24h": low_24h,
        "vol_24h": vol_24h,
        "funding_rate": funding_rate,
        "long_short_ratio": long_short_ratio,
        "long_pct": long_pct,
        "short_pct": short_pct,
        "change_24h": if open_24h > 0.0 { (current_price - open_24h) / open_24h * 100.0 } else { 0.0 },
    })).unwrap_or_default());

    let session_event = Event::default().data(serde_json::to_string(&json!({
        "type": "session_created",
        "session_id": session_id_str,
        "symbol": symbol,
    })).unwrap_or_default());

    // Send session_created and market_data events
    let tx_init = tx.clone();
    tokio::spawn(async move {
        let _ = tx_init.send(Ok(session_event)).await;
        let _ = tx_init.send(Ok(market_event)).await;
    });

    // Spawn the main work task
    tokio::spawn(async move {
        // 5. Call LLM for each of 6 agents
        let mut agent_opinions: Vec<Value> = Vec::new();

        for agent_def in AGENTS {
            // Send agent_thinking event
            let thinking_event = Event::default().data(serde_json::to_string(&json!({
                "type": "agent_thinking",
                "agent_id": agent_def.id,
                "agent_name": agent_def.name,
                "department": agent_def.department,
            })).unwrap_or_default());
            let _ = tx.send(Ok(thinking_event)).await;

            let system_prompt = format!(
                "你是{}的{}，名叫{}。你的分析视角：{}。\n\
                你需要基于提供的OKX实时市场数据，从你的专业角度进行客观分析。\n\
                关键：你是分析师而非辩手，目标是给出最准确的判断，而非捍卫某个方向。\n\
                如果数据不支持你通常关注的信号方向，你应该如实报告。\n\
                当数据信号不明确时，给出neutral而非强行选择方向。\n\
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
                &pool,
                user_id,
                &system_prompt,
                &market_data_str,
                agent_def.id,
            )
            .await;

            let opinion = match llm_result {
                Ok(response) => {
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
                Err(e) => {
                    json!({
                        "agent_id": agent_def.id,
                        "agent_name": agent_def.name,
                        "department": agent_def.department,
                        "sentiment": "neutral",
                        "confidence": 0.3,
                        "analysis": format!("LLM调用失败: {}", e),
                        "key_factors": [],
                        "source": "llm_error",
                    })
                }
            };

            // Send agent_opinion event
            let opinion_event = Event::default().data(serde_json::to_string(&json!({
                "type": "agent_opinion",
                "agent_id": agent_def.id,
                "agent_name": agent_def.name,
                "department": agent_def.department,
                "sentiment": opinion.get("sentiment"),
                "confidence": opinion.get("confidence"),
                "analysis": opinion.get("analysis"),
                "key_factors": opinion.get("key_factors"),
            })).unwrap_or_default());
            let _ = tx.send(Ok(opinion_event)).await;

            agent_opinions.push(opinion);
        }

        // Update progress: agents done
        let _ = sqlx::query(
            r#"UPDATE debate_sessions SET progress = 'generating_reports', agent_opinions = $2, updated_at = NOW() WHERE id = $1"#,
        )
        .bind(session_id)
        .bind(serde_json::to_value(&agent_opinions).unwrap_or(json!([])))
        .execute(&pool)
        .await;

        // 6. Call LLM for each department report
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
                &pool,
                user_id,
                &system_prompt,
                &user_message,
                &format!("{}_dept_report", dept),
            )
            .await;

            let report = match llm_result {
                Ok(response) => {
                    let parsed = parse_dept_report_response(&response);
                    json!({
                        "department": dept,
                        "consensus": parsed.consensus,
                        "bull_summary": parsed.bull_summary,
                        "bear_summary": parsed.bear_summary,
                    })
                }
                Err(_) => {
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

            // Send dept_report event
            let dept_event = Event::default().data(serde_json::to_string(&json!({
                "type": "dept_report",
                "department": dept,
                "consensus": report.get("consensus"),
                "bull_summary": report.get("bull_summary"),
                "bear_summary": report.get("bear_summary"),
            })).unwrap_or_default());
            let _ = tx.send(Ok(dept_event)).await;

            department_reports.push(report);
        }

        // Update progress: department reports done
        let _ = sqlx::query(
            r#"UPDATE debate_sessions SET progress = 'fund_manager_deciding', department_reports = $2, updated_at = NOW() WHERE id = $1"#,
        )
        .bind(session_id)
        .bind(serde_json::to_value(&department_reports).unwrap_or(json!([])))
        .execute(&pool)
        .await;

        // 7. Call LLM for fund manager decision
        let all_opinions_str = serde_json::to_string_pretty(&agent_opinions).unwrap_or_default();
        let all_reports_str = serde_json::to_string_pretty(&department_reports).unwrap_or_default();

        let fund_manager_system_prompt = format!(
            "你是基金经理，负责综合各部门的分析报告，做出最终交易决策。\n\
            你需要基于以下信息做出决策：\n\
            1. 各分析师的意见和信心度\n\
            2. 各部门的汇总报告\n\
            3. 当前市场价格: {:.6}\n\n\
            重要：推理中必须使用精确价格（如0.084740而非0.08），不得简化或四舍五入价格，否则会导致错误的支撑/阻力判断。\n\n\
            决策原则（必须遵守）：\n\
            - 做多和做空应该有同等门槛，不要因为'避险'而偏向做空\n\
            - 如果多空信号势均力敌，选择hold比强行选方向更合理\n\
            - 多空比极端值需要结合趋势方向判断，不能简单认为'拥挤=反转'\n\
            - 分析师给出neutral时，代表数据不明确，不应被忽略\n\
            - 不要将'谨慎'等同于'看空'\n\n\
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
            symbol_for_task, all_opinions_str, all_reports_str,
        );

        let fund_manager_result = analyze_with_llm(
            &pool,
            user_id,
            &fund_manager_system_prompt,
            &fund_manager_message,
            "fund_manager",
        )
        .await;

        let fund_manager_decision = match fund_manager_result {
            Ok(response) => {
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
            Err(_) => {
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

        // Send fund_manager event
        let fm_event = Event::default().data(serde_json::to_string(&json!({
            "type": "fund_manager",
            "action": fund_manager_decision.get("action"),
            "confidence": fund_manager_decision.get("confidence"),
            "entry_range": fund_manager_decision.get("entry_range"),
            "stop_loss": fund_manager_decision.get("stop_loss"),
            "take_profit": fund_manager_decision.get("take_profit"),
            "leverage": fund_manager_decision.get("leverage"),
            "reasoning": fund_manager_decision.get("reasoning"),
        })).unwrap_or_default());
        let _ = tx.send(Ok(fm_event)).await;

        // 8. Update the debate session with all results
        let _ = sqlx::query(
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
        .execute(&pool)
        .await;

        // 9. Send debate_complete event
        let complete_event = Event::default().data(serde_json::to_string(&json!({
            "type": "debate_complete",
            "session_id": session_id.to_string(),
        })).unwrap_or_default());
        let _ = tx.send(Ok(complete_event)).await;
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
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

// ==================== Market Analysis Helper Functions ====================

use crate::exchanges::okx::OkxClient;
 use crate::exchanges::okx::OkxCandle;

/// Extended market context with additional indicators
struct MarketContextExtended {
    trend: String,
    trend_strength: f64,
    volatility: String,
    volume_profile: String,
    key_levels: (f64, f64, f64),
    ma5: f64,
    ma10: f64,
    ma20: f64,
    rsi_14: f64,
    macd_signal: String,
    atr: f64,
    price_change_1h: f64,
    price_change_4h: f64,
}

/// Analyze market context from candles with extended indicators
fn analyze_market_context(candles: &[OkxCandle], current_price: f64) -> (String, f64, String, String, (f64, f64, f64)) {
    let ctx = analyze_market_context_extended(candles, current_price);
    (ctx.trend, ctx.trend_strength, ctx.volatility, ctx.volume_profile, ctx.key_levels)
}

fn analyze_market_context_extended(candles: &[OkxCandle], current_price: f64) -> MarketContextExtended {
    if candles.len() < 10 {
        return MarketContextExtended {
            trend: "unknown".to_string(), trend_strength: 0.5,
            volatility: "medium".to_string(), volume_profile: "stable".to_string(),
            key_levels: (current_price * 0.95, current_price, current_price * 1.05),
            ma5: current_price, ma10: current_price, ma20: current_price,
            rsi_14: 50.0, macd_signal: "neutral".to_string(),
            atr: current_price * 0.01, price_change_1h: 0.0, price_change_4h: 0.0,
        };
    }

    // Parse all closes in ASC order (oldest first)
    let all_closes: Vec<f64> = candles.iter().rev()
        .filter_map(|c| c.c.as_ref().and_then(|s| s.parse::<f64>().ok()))
        .collect();

    // Moving averages
    let ma5: f64 = all_closes.iter().rev().take(5).sum::<f64>() / 5.0;
    let ma10: f64 = all_closes.iter().rev().take(10).sum::<f64>() / 10.0;
    let ma20: f64 = if all_closes.len() >= 20 {
        all_closes.iter().rev().take(20).sum::<f64>() / 20.0
    } else {
        all_closes.iter().sum::<f64>() / all_closes.len() as f64
    };

    let trend = if ma5 > ma20 * 1.02 {
        "bull".to_string()
    } else if ma5 < ma20 * 0.98 {
        "bear".to_string()
    } else {
        "range".to_string()
    };

    let trend_strength = ((ma5 - ma20) / ma20).abs().min(0.1) * 10.0;

    // RSI calculation (14-period)
    let rsi_14 = if all_closes.len() >= 15 {
        let changes: Vec<f64> = all_closes.windows(2).map(|w| w[1] - w[0]).collect();
        let recent_changes: Vec<f64> = changes.iter().rev().take(14).copied().collect();
        let avg_gain: f64 = recent_changes.iter().filter(|&&c| c > 0.0).map(|&c| c).sum::<f64>() / 14.0;
        let avg_loss: f64 = recent_changes.iter().filter(|&&c| c < 0.0).map(|&c| c.abs()).sum::<f64>() / 14.0;
        if avg_loss == 0.0 { 100.0 } else {
            let rs = avg_gain / avg_loss;
            100.0 - (100.0 / (1.0 + rs))
        }
    } else {
        50.0
    };

    // MACD signal (simplified: EMA12 vs EMA26 trend)
    let macd_signal = if all_closes.len() >= 26 {
        let ema12 = calc_ema(&all_closes, 12);
        let ema26 = calc_ema(&all_closes, 26);
        let macd_line = ema12 - ema26;
        if macd_line > 0.0 { "bullish".to_string() } else { "bearish".to_string() }
    } else {
        "neutral".to_string()
    };

    // ATR (14-period)
    let atr: f64 = candles.iter().rev().take(14)
        .filter_map(|c| {
            let h = c.h.as_ref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let l = c.l.as_ref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let o = c.o.as_ref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let close = c.c.as_ref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            Some((h - l).max((close - o).abs()))
        })
        .sum::<f64>() / 14.0;
    let volatility_ratio = atr / current_price;
    let volatility = if volatility_ratio < 0.01 {
        "low".to_string()
    } else if volatility_ratio < 0.03 {
        "medium".to_string()
    } else {
        "high".to_string()
    };

    // Volume profile
    let recent_vol: Vec<f64> = candles.iter().rev().take(10)
        .filter_map(|c| c.vol.as_ref().and_then(|s| s.parse::<f64>().ok()))
        .collect();
    let avg_vol = recent_vol.iter().sum::<f64>() / recent_vol.len().max(1) as f64;
    let volume_profile = if recent_vol.is_empty() || avg_vol == 0.0 {
        "stable".to_string()
    } else if recent_vol[0] > avg_vol * 1.3 {
        "increasing".to_string()
    } else if recent_vol[0] < avg_vol * 0.7 {
        "decreasing".to_string()
    } else {
        "stable".to_string()
    };

    // Key levels
    let high = candles.iter()
        .filter_map(|c| c.h.as_ref().and_then(|s| s.parse::<f64>().ok()))
        .fold(f64::MIN, f64::max);
    let low = candles.iter()
        .filter_map(|c| c.l.as_ref().and_then(|s| s.parse::<f64>().ok()))
        .fold(f64::MAX, f64::min);
    let key_levels = (low, (high + low) / 2.0, high);

    // Price changes
    let price_change_1h = if all_closes.len() >= 2 {
        let prev = all_closes[all_closes.len() - 2];
        if prev > 0.0 { (current_price - prev) / prev * 100.0 } else { 0.0 }
    } else { 0.0 };
    let price_change_4h = if all_closes.len() >= 5 {
        let prev = all_closes[all_closes.len() - 5];
        if prev > 0.0 { (current_price - prev) / prev * 100.0 } else { 0.0 }
    } else { 0.0 };

    MarketContextExtended {
        trend, trend_strength, volatility, volume_profile, key_levels,
        ma5, ma10, ma20, rsi_14, macd_signal, atr,
        price_change_1h, price_change_4h,
    }
}

/// Calculate EMA (Exponential Moving Average)
fn calc_ema(data: &[f64], period: usize) -> f64 {
    if data.len() < period { return data.last().copied().unwrap_or(0.0); }
    let k = 2.0 / (period as f64 + 1.0);
    let mut ema: f64 = data[..period].iter().sum::<f64>() / period as f64;
    for price in &data[period..] {
        ema = price * k + ema * (1.0 - k);
    }
    ema
}

/// Multi-timeframe data structure
#[derive(Debug, Clone)]
struct MultiTimeframeData {
    m5_trend: String,
    m15_trend: String,
    h1_trend: String,
    h4_trend: String,
    d1_trend: String,
    alignment: f64,
    alignment_details: String,
}

/// Fetch and analyze multi-timeframe data
async fn fetch_multi_timeframe_data(okx_client: &OkxClient, symbol: &str, current_price: f64) -> MultiTimeframeData {
    let intervals = [("5m", 5), ("15m", 15), ("1H", 60), ("4H", 240), ("1D", 1440)];
    let mut trends: Vec<(String, String)> = Vec::new();

    for (interval, _mins) in &intervals {
        let candles = okx_client.get_candles(symbol, interval, Some(20)).await.unwrap_or_default();
        if candles.len() >= 5 {
            let (trend, _, _, _, _) = analyze_market_context(&candles, current_price);
            trends.push((interval.to_string(), trend));
        } else {
            trends.push((interval.to_string(), "unknown".to_string()));
        }
    }

    let m5_trend = trends.get(0).map(|t| t.1.clone()).unwrap_or_else(|| "unknown".to_string());
    let m15_trend = trends.get(1).map(|t| t.1.clone()).unwrap_or_else(|| "unknown".to_string());
    let h1_trend = trends.get(2).map(|t| t.1.clone()).unwrap_or_else(|| "unknown".to_string());
    let h4_trend = trends.get(3).map(|t| t.1.clone()).unwrap_or_else(|| "unknown".to_string());
    let d1_trend = trends.get(4).map(|t| t.1.clone()).unwrap_or_else(|| "unknown".to_string());

    // Calculate alignment
    let aligned_count = trends.iter().filter(|t| t.1 != "unknown" && t.1 == trends[0].1).count();
    let alignment = aligned_count as f64 / trends.len() as f64;

    let alignment_details = match alignment {
        a if a >= 0.8 => "强一致性".to_string(),
        a if a >= 0.6 => "中等一致性".to_string(),
        a if a >= 0.4 => "存在分歧".to_string(),
        _ => "方向不明".to_string(),
    };

    MultiTimeframeData {
        m5_trend,
        m15_trend,
        h1_trend,
        h4_trend,
        d1_trend,
        alignment,
        alignment_details,
    }
}

/// Calculate enhanced confidence with market context calibration and debate quality
fn calculate_enhanced_confidence(
    base_confidence: f64,
    agent_opinions: &[Value],
    volatility: &str,
    mtf_alignment: f64,
    historical_accuracy: f64,
) -> f64 {
    // Agent agreement factor
    let sentiments: Vec<&str> = agent_opinions.iter()
        .filter_map(|o| o.get("sentiment").and_then(|v| v.as_str()))
        .collect();
    let bull_count = sentiments.iter().filter(|&&s| s == "bullish").count() as f64;
    let bear_count = sentiments.iter().filter(|&&s| s == "bearish").count() as f64;
    let total = sentiments.len() as f64;
    let agreement = if total > 0.0 { (bull_count.max(bear_count) / total).max(0.5) } else { 0.5 };

    // Debate quality factor: agents that maintained their position after rebuttal are more reliable
    let sentiment_changed_count = agent_opinions.iter()
        .filter(|o| o.get("sentiment_changed").and_then(|v| v.as_bool()).unwrap_or(false))
        .count() as f64;
    let debate_stability = if total > 0.0 {
        1.0 - (sentiment_changed_count / total) * 0.3 // Small penalty for flip-flopping
    } else {
        1.0
    };

    // Volatility factor (reduce confidence in high volatility)
    let volatility_factor = match volatility {
        "high" => 0.85,
        "medium" => 0.95,
        "low" => 1.0,
        _ => 0.95,
    };

    // Multi-timeframe alignment factor
    let alignment_factor = 0.7 + (mtf_alignment * 0.3);

    // Historical accuracy factor
    let historical_factor = 0.8 + (historical_accuracy.min(1.0) * 0.2);

    // Final confidence calculation
    // 45% base + 20% agreement + 15% alignment + 10% historical + 10% debate stability
    let final_confidence = base_confidence * 0.45 +
                          agreement * 0.20 +
                          alignment_factor * 0.15 +
                          historical_factor * 0.10 +
                          debate_stability * 0.10;

    // Apply volatility dampening
    let calibrated_confidence = final_confidence * volatility_factor;

    // Clamp to reasonable range
    calibrated_confidence.max(0.2).min(0.98)
}

/// Extract trend patterns from historical decisions by querying actual success rate
async fn trends_from_decisions(pool: &sqlx::PgPool, user_id: i64, symbol: &str) -> f64 {
    // Query actual historical accuracy from decision_memory
    let result = sqlx::query_scalar::<_, f64>(
        r#"SELECT COALESCE(AVG(CASE WHEN success THEN 1.0 ELSE 0.0 END), 0.5)
        FROM decision_memory
        WHERE user_id = $1 AND symbol = $2 AND success IS NOT NULL
        AND created_at > NOW() - INTERVAL '30 days'"#
    )
    .bind(user_id)
    .bind(symbol)
    .fetch_one(pool)
    .await;

    match result {
        Ok(accuracy) => {
            // Apply sample size weighting: with few samples, regress toward 0.5
            let count_result = sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM decision_memory
                WHERE user_id = $1 AND symbol = $2 AND success IS NOT NULL
                AND created_at > NOW() - INTERVAL '30 days'"#
            )
            .bind(user_id)
            .bind(symbol)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

            let sample_weight = (count_result as f64 / 20.0).min(1.0); // Full weight at 20+ samples
            // Blend actual accuracy with neutral 0.5 based on sample size
            accuracy * sample_weight + 0.5 * (1.0 - sample_weight)
        }
        Err(_) => 0.5,
    }
}

/// Parsed result from OKX long-short-account-ratio API
struct LongShortRatioResult {
    /// The long/short ratio (e.g., 3.04 means longs outnumber shorts 3:1)
    long_short_ratio: Option<f64>,
    /// Long account percentage (calculated from ratio, e.g., 75.2%)
    long_pct: Option<f64>,
    /// Short account percentage (calculated from ratio, e.g., 24.8%)
    short_pct: Option<f64>,
}

/// Parse OKX long-short-account-ratio API response.
///
/// OKX API `/api/v5/rubik/stat/contracts/long-short-account-ratio` returns:
/// `{"code":"0","data":[["timestamp","longShortRatio"],...],"msg":""}`
///
/// Each data item is a 2-element array: [timestamp_ms, longShortRatio]
/// - longShortRatio > 1 means longs outnumber shorts
/// - longShortRatio < 1 means shorts outnumber longs
///
/// From the ratio, we derive:
/// - long_pct = ratio / (ratio + 1) * 100
/// - short_pct = 1 / (ratio + 1) * 100
fn parse_long_short_ratio(data: &serde_json::Value) -> LongShortRatioResult {
    let arr = data.get("data").and_then(|d| d.as_array());
    if arr.is_none() {
        tracing::warn!("OKX long-short-account-ratio API returned no data array");
        return LongShortRatioResult { long_short_ratio: None, long_pct: None, short_pct: None };
    }
    let arr = arr.unwrap();
    if arr.is_empty() {
        tracing::warn!("OKX long-short-account-ratio API returned empty data");
        return LongShortRatioResult { long_short_ratio: None, long_pct: None, short_pct: None };
    }

    let first = arr.first().unwrap();
    let parts = first.as_array();

    if parts.is_none() {
        tracing::warn!("OKX long-short-account-ratio data item is not an array: {:?}", first);
        return LongShortRatioResult { long_short_ratio: None, long_pct: None, short_pct: None };
    }
    let parts = parts.unwrap();

    // OKX returns 2-element arrays: [timestamp, longShortRatio]
    // Try index 1 first (the ratio value)
    let ratio = parts.get(1)
        .and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok())
        .or_else(|| parts.get(1).and_then(|v| v.as_f64()));

    match ratio {
        Some(r) if r > 0.0 => {
            let long_pct = r / (r + 1.0) * 100.0;
            let short_pct = 1.0 / (r + 1.0) * 100.0;
            tracing::info!("Parsed long-short ratio: ratio={:.4}, long_pct={:.2}%, short_pct={:.2}%", r, long_pct, short_pct);
            LongShortRatioResult {
                long_short_ratio: Some(r),
                long_pct: Some(long_pct),
                short_pct: Some(short_pct),
            }
        }
        Some(r) => {
            tracing::warn!("OKX long-short-account-ratio returned invalid ratio: {}", r);
            LongShortRatioResult { long_short_ratio: None, long_pct: None, short_pct: None }
        }
        None => {
            tracing::warn!("Failed to parse long-short ratio from OKX response: {:?}", parts);
            LongShortRatioResult { long_short_ratio: None, long_pct: None, short_pct: None }
        }
    }
}

// ==================== Reusable Debate Function for Simulation ====================

#[derive(Debug, Clone)]
pub struct DebateResult {
    pub session_id: Uuid,
    pub action: String,       // "long", "short", "hold"
    pub confidence: f64,
    pub stop_loss: f64,
    pub take_profit: Vec<f64>,
    pub leverage: i32,
    pub reasoning: String,
    pub agent_opinions: Vec<Value>,
    pub department_reports: Vec<Value>,
}

pub async fn run_debate_for_simulation(
    pool: &sqlx::PgPool,
    user_id: i64,
    symbol: &str,
    current_price: f64,
    interval: &str,
) -> Result<DebateResult> {

    // 1. Get OKX client by querying DB directly (avoid creating AppState)
    let key_row = sqlx::query(
        r#"SELECT key, secret, passphrase, metadata FROM api_keys WHERE user_id = $1 AND is_active = true AND key_type = 'exchange' LIMIT 1"#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("未配置交易所 API 密钥，请先在系统设置中添加".to_string()))?;

    let encrypted_key: String = key_row.get("key");
    let encrypted_secret: String = key_row.get("secret");
    let encrypted_passphrase: String = key_row.get("passphrase");
    let api_key = decrypt(&encrypted_key).map_err(|e| AppError::Internal(format!("解密 API Key 失败: {}", e)))?;
    let secret = decrypt(&encrypted_secret).map_err(|e| AppError::Internal(format!("解密 Secret 失败: {}", e)))?;
    let passphrase = decrypt(&encrypted_passphrase).unwrap_or_default();
    let metadata: serde_json::Value = key_row.get("metadata");
    let is_demo = metadata.get("is_demo").and_then(|v| v.as_bool()).unwrap_or(false);
    let okx_client = {
        let proxy_url = crate::state::get_proxy_config_from_db(pool).await;
        crate::exchanges::okx::OkxClient::new_with_proxy(api_key, secret, passphrase, is_demo, proxy_url)
    };

    // 2. Fetch market data
    let ticker = okx_client.get_ticker(symbol).await
        .map_err(|e| AppError::Validation(format!("获取行情数据失败: {}", e)))?;
    let candles = okx_client.get_candles(symbol, interval, Some(100)).await
        .unwrap_or_default();

    let funding_data = okx_client.get_raw("/api/v5/public/funding-rate", Some(&[("instId", symbol.to_string())])).await
        .unwrap_or_else(|_| json!({}));
    let ccy = symbol.split('-').next().unwrap_or(symbol).to_string();
    let long_short_data = okx_client.get_raw(
        "/api/v5/rubik/stat/contracts/long-short-account-ratio",
        Some(&[("ccy", ccy), ("period", "5m".to_string())]),
    ).await.unwrap_or_else(|_| json!({}));

    let open_24h: f64 = ticker.open_24h.as_ref().and_then(|v| v.parse().ok()).unwrap_or(current_price);
    let high_24h: f64 = ticker.high_24h.as_ref().and_then(|v| v.parse().ok()).unwrap_or(current_price);
    let low_24h: f64 = ticker.low_24h.as_ref().and_then(|v| v.parse().ok()).unwrap_or(current_price);
    let vol_24h: f64 = ticker.vol_24h.as_ref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let funding_rate = funding_data.get("data").and_then(|d| d.get(0))
        .and_then(|item| item.get("fundingRate")).and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let ls_result = parse_long_short_ratio(&long_short_data);
    let long_short_ratio = ls_result.long_short_ratio;
    let long_pct = ls_result.long_pct;
    let short_pct = ls_result.short_pct;

    // Fetch orderbook depth data (top 5 bids/asks)
    let orderbook_data = okx_client.get_raw(
        "/api/v5/market/books",
        Some(&[("instId", symbol.to_string()), ("sz", "5".to_string())]),
    ).await.unwrap_or_else(|_| json!({}));

    // Parse orderbook depth
    let (bid_depth_info, ask_depth_info, bid_ask_imbalance) = {
        let books = orderbook_data.get("data").and_then(|d| d.get(0));
        if let Some(book) = books {
            let bids = book.get("bids").and_then(|b| b.as_array()).cloned().unwrap_or_default();
            let asks = book.get("asks").and_then(|a| a.as_array()).cloned().unwrap_or_default();

            let bid_total: f64 = bids.iter().take(5).filter_map(|b| {
                let price: f64 = b.get(0)?.as_str()?.parse().ok()?;
                let size: f64 = b.get(1)?.as_str()?.parse().ok()?;
                Some(price * size)
            }).sum();
            let ask_total: f64 = asks.iter().take(5).filter_map(|a| {
                let price: f64 = a.get(0)?.as_str()?.parse().ok()?;
                let size: f64 = a.get(1)?.as_str()?.parse().ok()?;
                Some(price * size)
            }).sum();

            let bid_info: Vec<String> = bids.iter().take(5).filter_map(|b| {
                let price = b.get(0)?.as_str()?;
                let size = b.get(1)?.as_str()?;
                Some(format!("{} x {}", price, size))
            }).collect();
            let ask_info: Vec<String> = asks.iter().take(5).filter_map(|a| {
                let price = a.get(0)?.as_str()?;
                let size = a.get(1)?.as_str()?;
                Some(format!("{} x {}", price, size))
            }).collect();

            let imbalance = if bid_total + ask_total > 0.0 {
                (bid_total - ask_total) / (bid_total + ask_total)
            } else {
                0.0
            };

            (bid_info.join(", "), ask_info.join(", "), imbalance)
        } else {
            ("数据不可用".to_string(), "数据不可用".to_string(), 0.0)
        }
    };

    // Fetch open interest data
    let open_interest_data = okx_client.get_raw(
        "/api/v5/public/open-interest",
        Some(&[("instType", "SWAP".to_string()), ("instId", symbol.to_string())]),
    ).await.unwrap_or_else(|_| json!({}));

    let (open_interest, oi_change_hint) = {
        let oi_item = open_interest_data.get("data").and_then(|d| d.get(0));
        if let Some(item) = oi_item {
            let oi = item.get("oi").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
            let oi_ccy = item.get("oiCcy").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
            let hint = match oi {
                Some(val) if val > 0.0 => format!("持仓量: {} (币本位: {})", val, oi_ccy.unwrap_or(0.0)),
                _ => "持仓量数据不可用".to_string(),
            };
            (oi, hint)
        } else {
            (None, "持仓量数据不可用".to_string())
        }
    };

    // OKX returns candles in DESC order (newest first). Reverse to ASC for AI analysis.
    // Pass 50 candles for more comprehensive analysis
    let recent_candles: Vec<Value> = candles.iter().take(50).rev().map(|c| {
        let ts = c.ts.as_deref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        json!({
            "time": ts, "open": c.o, "high": c.h, "low": c.l, "close": c.c, "volume": c.vol,
        })
    }).collect();

    // Analyze market context: trend, volatility, volume profile
    let ctx = analyze_market_context_extended(&candles, current_price);

    // Fetch multi-timeframe data
    let multi_timeframe_data = fetch_multi_timeframe_data(&okx_client, symbol, current_price).await;

    let market_data_str = format!(
        "## 实时市场数据
交易对: {}
当前价格: {:.6}

### 技术指标
趋势: {} (强度: {:.1}%)
MA5: {:.6}, MA10: {:.6}, MA20: {:.6}
RSI(14): {:.1}
MACD信号: {}
ATR(14): {:.6}
波动性: {}
成交量: {}
关键支撑/阻力位: {:.6}, {:.6}, {:.6}
近1h涨跌: {:.4}%
近4h涨跌: {:.4}%

### 多时间框架分析
5分钟趋势: {}
15分钟趋势: {}
1小时趋势: {}
4小时趋势: {}
日线趋势: {}
周期一致性: {:.1}% ({})
近期K线: {}

24h涨跌: {:.4}%
24h最高/最低: {:.6}/{:.6}
24h成交量: {:.4}
资金费率: {:.8}
多空比: {}
多空详情: 多头占比{}, 空头占比{}

### 订单簿深度
买盘(前5): {}
卖盘(前5): {}
买卖力量比: {:.4} (正值=买盘强, 负值=卖盘强)

### 持仓量
{}",
        symbol, current_price,
        ctx.trend, ctx.trend_strength * 100.0,
        ctx.ma5, ctx.ma10, ctx.ma20,
        ctx.rsi_14,
        ctx.macd_signal,
        ctx.atr,
        ctx.volatility, ctx.volume_profile,
        ctx.key_levels.0, ctx.key_levels.1, ctx.key_levels.2,
        ctx.price_change_1h, ctx.price_change_4h,
        multi_timeframe_data.m5_trend,
        multi_timeframe_data.m15_trend,
        multi_timeframe_data.h1_trend,
        multi_timeframe_data.h4_trend,
        multi_timeframe_data.d1_trend,
        multi_timeframe_data.alignment * 100.0,
        multi_timeframe_data.alignment_details,
        serde_json::to_string_pretty(&recent_candles).unwrap_or_default(),
        if open_24h > 0.0 { (current_price - open_24h) / open_24h * 100.0 } else { 0.0 },
        high_24h, low_24h, vol_24h, funding_rate,
        long_short_ratio.map(|r| format!("{:.4}", r)).unwrap_or_else(|| "数据不可用".to_string()),
        long_pct.map(|p| format!("{:.2}%", p)).unwrap_or_else(|| "数据不可用".to_string()),
        short_pct.map(|p| format!("{:.2}%", p)).unwrap_or_else(|| "数据不可用".to_string()),
        bid_depth_info, ask_depth_info, bid_ask_imbalance,
        oi_change_hint,
    );

    // 3. Fetch historical decision memory for context (learning feedback)
    let recent_decisions = sqlx::query(
        r#"SELECT symbol, action, confidence, actual_outcome, actual_pnl_percent, success, close_reason, reflection, created_at
        FROM decision_memory
        WHERE user_id = $1 AND symbol = $2 AND success IS NOT NULL
        ORDER BY created_at DESC LIMIT 10"#
    )
    .bind(user_id)
    .bind(symbol)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let history_str = if recent_decisions.is_empty() {
        String::new()
    } else {
        let mut lines = Vec::new();
        for row in &recent_decisions {
            let action: String = row.get("action");
            let confidence: f64 = row.get("confidence");
            let success: bool = row.get("success");
            let pnl: Option<f64> = row.get("actual_pnl_percent");
            let close_reason: Option<String> = row.get("close_reason");
            lines.push(format!(
                "- {} (置信度{:.0}%): {} 盈亏{:.2}% 原因:{}",
                action, confidence * 100.0,
                if success { "盈利" } else { "亏损" },
                pnl.unwrap_or(0.0),
                close_reason.unwrap_or_default(),
            ));
        }
        format!("最近{}次同类决策:\n{}", recent_decisions.len(), lines.join("\n"))
    };

    // Fetch agent credibility weights from agent_performance
    let agent_perfs = sqlx::query(
        r#"SELECT agent_name, agent_department, accuracy, credibility_score, calibration_factor, total_analyses
        FROM agent_performance WHERE total_analyses > 0"#
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut credibility_map: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for row in &agent_perfs {
        let name: String = row.get("agent_name");
        let credibility: f64 = row.get("credibility_score");
        credibility_map.insert(name, credibility);
    }

    let credibility_str = if credibility_map.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = credibility_map.iter()
            .map(|(name, score)| format!("{}: {:.0}%", name, score * 100.0))
            .collect();
        format!("\n各分析师历史可信度: {}", items.join(", "))
    };

    // 4. Create debate session with market snapshot for auditing
    let market_snapshot = json!({
        "symbol": symbol,
        "current_price": current_price,
        "open_24h": open_24h,
        "high_24h": high_24h,
        "low_24h": low_24h,
        "vol_24h": vol_24h,
        "funding_rate": funding_rate,
        "long_short_ratio": long_short_ratio,
        "long_pct": long_pct,
        "short_pct": short_pct,
        "trend": ctx.trend,
        "trend_strength": ctx.trend_strength,
        "rsi_14": ctx.rsi_14,
        "macd_signal": ctx.macd_signal,
        "atr": ctx.atr,
        "ma5": ctx.ma5,
        "ma10": ctx.ma10,
        "ma20": ctx.ma20,
        "candles_count": candles.len(),
        "data_source": "okx_realtime",
        "bid_ask_imbalance": bid_ask_imbalance,
        "open_interest": open_interest,
    });

    let session_row = sqlx::query(
        r#"INSERT INTO debate_sessions (user_id, symbol, status, progress, agent_opinions, department_reports, fund_manager_decision, market_snapshot, created_at, updated_at)
        VALUES ($1, $2, 'in_progress', 'auto_simulation', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb, $3, NOW(), NOW())
        RETURNING id"#
    )
    .bind(user_id)
    .bind(symbol)
    .bind(&market_snapshot)
    .fetch_one(pool)
    .await?;

    let session_id: Uuid = session_row.get::<Uuid, _>("id");

    // 5. Run 6 agents - Round 1: Initial analysis (with improved prompts for objectivity)
    let mut agent_opinions: Vec<Value> = Vec::new();
    for agent_def in AGENTS {
        let agent_credibility = credibility_map.get(agent_def.name).copied().unwrap_or(0.5);
        let dept_label = match agent_def.department {
            "technical" => "技术分析部", "capital" => "资金分析部", "news" => "新闻分析部", _ => "分析部",
        };
        let system_prompt = format!(
            "你是{}的分析师，名叫{}。你的分析视角：{}。\n\
            你的历史可信度评分: {:.0}%（基于历史预测准确率）\n\n\
            关键原则（必须严格遵守）：\n\
            1. 你是专业分析师，不是多空辩手。你的目标是给出最准确的判断，而非捍卫某个方向\n\
            2. 你必须基于数据客观分析。如果数据不支持你通常关注的信号方向，你应该如实报告\n\
            3. 优先关注最强烈、最明确的信号，忽略牵强附会的微弱信号\n\
            4. 使用精确价格数字（如0.084740而非0.08），避免四舍五入导致误判\n\
            5. 订单簿深度和持仓量数据是重要参考，注意买卖力量对比\n\
            6. 多空比极端值（如多头占比>70%或<30%）需要谨慎解读，不一定意味着反转\n\
            7. 如果数据信号不明确，给出较低的置信度，而非强行选择方向\n\n\
            基于OKX实时市场数据分析。必须JSON回复：\n\
            {{\"sentiment\": \"bullish\"|\"bearish\"|\"neutral\", \"confidence\": 0.5-1.0, \"analysis\": \"分析\", \"key_factors\": [\"因素\"]}}\n\
            重要：当数据信号不明确时，必须给出\"neutral\"而非强行选择方向。neutral是有效且负责任的判断。\n\
            只输出JSON。",
            dept_label,
            agent_def.name, agent_def.personality,
            agent_credibility * 100.0,
        );

        let opinion = match analyze_with_llm(pool, user_id, &system_prompt, &market_data_str, agent_def.id).await {
            Ok(response) => {
                let parsed = parse_agent_json_response(&response);
                json!({
                    "agent_id": agent_def.id, "agent_name": agent_def.name,
                    "department": agent_def.department, "sentiment": parsed.sentiment,
                    "confidence": parsed.confidence, "analysis": parsed.analysis,
                    "key_factors": parsed.key_factors, "source": "llm",
                })
            }
            Err(e) => json!({
                "agent_id": agent_def.id, "agent_name": agent_def.name,
                "department": agent_def.department, "sentiment": "neutral",
                "confidence": 0.3, "analysis": format!("LLM失败: {}", e),
                "key_factors": [], "source": "llm_error",
            }),
        };
        agent_opinions.push(opinion);
    }

    // 6. Cross-debate round: Let each agent see their department opponent's arguments and re-evaluate
    let mut revised_opinions: Vec<Value> = Vec::new();
    for agent_def in AGENTS {
        let agent_idx = AGENTS.iter().position(|a| a.id == agent_def.id).unwrap();
        let my_opinion = &agent_opinions[agent_idx];

        // Find the opponent in the same department (bull vs bear pair)
        let opponent_opinion = agent_opinions.iter().find(|o| {
            o.get("department").and_then(|v| v.as_str()) == Some(agent_def.department)
            && o.get("agent_id").and_then(|v| v.as_str()) != Some(agent_def.id)
        });

        if let Some(opponent) = opponent_opinion {
            let opponent_name = opponent.get("agent_name").and_then(|v| v.as_str()).unwrap_or("对手");
            let opponent_sentiment = opponent.get("sentiment").and_then(|v| v.as_str()).unwrap_or("neutral");
            let opponent_analysis = opponent.get("analysis").and_then(|v| v.as_str()).unwrap_or("");
            let opponent_factors = opponent.get("key_factors")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|f| f.as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();

            let my_sentiment = my_opinion.get("sentiment").and_then(|v| v.as_str()).unwrap_or("neutral");
            let my_analysis = my_opinion.get("analysis").and_then(|v| v.as_str()).unwrap_or("");

            let rebuttal_prompt = format!(
                "你是{}。你刚才给出了{}观点：{}\n\n\
                你的同部门对手{}给出了{}观点：{}\n\
                对手的关键论据：{}\n\n\
                请客观重新审视：\n\
                1. 对手的论据中哪些是合理的？你是否忽略了这些信号？\n\
                2. 你的原始论据中哪些可能存在偏差或过度解读？\n\
                3. 综合双方论据后，你是否需要调整你的判断或置信度？\n\n\
                重要：\n\
                - 如果对手论据更有说服力，降低置信度或翻转方向\n\
                - 但不要因为对手存在就自动让步——如果你有更强的数据支撑，坚持你的判断\n\
                - 多空比极端值不一定意味着反转，需要结合其他信号综合判断\n\
                - 避免对同一数据做过度解读（如'多头拥挤=必跌'是过度简化）\n\
                - 使用精确价格数字，不要四舍五入\n\n\
                必须JSON回复：{{\"sentiment\": \"bullish\"|\"bearish\", \"confidence\": 0.5-1.0, \"analysis\": \"反驳后的重新分析\", \"key_factors\": [\"因素\"], \"revised\": true}}\n\
                只输出JSON。",
                agent_def.name,
                my_sentiment, my_analysis,
                opponent_name, opponent_sentiment, opponent_analysis,
                opponent_factors,
            );

            let revised = match analyze_with_llm(pool, user_id, &rebuttal_prompt, &market_data_str, &format!("{}_rebuttal", agent_def.id)).await {
                Ok(response) => {
                    let parsed = parse_agent_json_response(&response);
                    json!({
                        "agent_id": agent_def.id, "agent_name": agent_def.name,
                        "department": agent_def.department, "sentiment": parsed.sentiment,
                        "confidence": parsed.confidence, "analysis": parsed.analysis,
                        "key_factors": parsed.key_factors, "source": "llm_rebuttal",
                        "original_sentiment": my_sentiment,
                        "sentiment_changed": my_sentiment != parsed.sentiment,
                    })
                }
                Err(_) => {
                    // If rebuttal fails, keep original opinion
                    let mut revised = my_opinion.clone();
                    revised["source"] = json!("llm_rebuttal_fallback");
                    revised["original_sentiment"] = json!(my_sentiment);
                    revised["sentiment_changed"] = json!(false);
                    revised
                }
            };
            revised_opinions.push(revised);
        } else {
            // No opponent found, keep original
            revised_opinions.push(my_opinion.clone());
        }
    }

    // Use revised opinions for further analysis
    agent_opinions = revised_opinions;

    // 7. Department reports (with cross-debate results)
    let mut department_reports: Vec<Value> = Vec::new();
    for dept in &["technical", "capital", "news"] {
        let dept_name = match *dept { "technical" => "技术分析部", "capital" => "资金分析部", "news" => "新闻分析部", _ => "分析部" };
        let dept_opinions: Vec<&Value> = agent_opinions.iter()
            .filter(|o| o.get("department").and_then(|v| v.as_str()) == Some(*dept)).collect();

        let report = match analyze_with_llm(
            pool, user_id,
            &format!("你是{}汇总分析师。综合部门内交叉辩论后的意见给出JSON：{{\"consensus\":\"bullish\"|\"bearish\", \"bull_summary\":\"理由\", \"bear_summary\":\"理由\", \"debate_highlights\":\"辩论中暴露的关键分歧\"}}。注意：分析师已看过对手论据并重新评估，关注是否有分析师翻转了方向。只输出JSON。", dept_name),
            &format!("{}交叉辩论后意见:\n{}", dept_name, serde_json::to_string_pretty(&dept_opinions).unwrap_or_default()),
            &format!("{}_dept_report", dept),
        ).await {
            Ok(response) => {
                let parsed = parse_dept_report_response(&response);
                json!({ "department": dept, "consensus": parsed.consensus, "bull_summary": parsed.bull_summary, "bear_summary": parsed.bear_summary })
            }
            Err(_) => {
                let sentiments: Vec<&str> = dept_opinions.iter().filter_map(|o| o.get("sentiment").and_then(|v| v.as_str())).collect();
                let bull = sentiments.iter().filter(|&&s| s == "bullish").count();
                let bear = sentiments.iter().filter(|&&s| s == "bearish").count();
                json!({ "department": dept, "consensus": if bull > bear { "bullish" } else if bear > bull { "bearish" } else { "neutral" }, "bull_summary": "基于多数意见", "bear_summary": "基于多数意见" })
            }
        };
        department_reports.push(report);
    }

    // 8. Fund manager decision (with historical reflection and orderbook/OI context)
    let fund_manager_decision = match analyze_with_llm(
        pool, user_id,
        &format!("你是基金经理。综合各部门交叉辩论后的报告做决策。当前价: {:.6}。{}{}\n重要：推理中必须使用精确价格（如0.084740而非0.08），不得简化或四舍五入价格，否则会导致错误的支撑/阻力判断。\n\n决策要点：\n1. 优先关注交叉辩论后仍保持一致的信号（这些更可靠）\n2. 关注辩论中翻转方向的分析师（说明对手论据更有说服力）\n3. 订单簿深度和持仓量是重要参考数据\n4. 多空比极端值需要结合趋势方向判断，不能简单认为'拥挤=反转'\n5. 如果多空信号势均力敌，选择hold比强行选方向更合理\n6. 做多和做空应该有同等门槛，不要因为'避险'而偏向做空\n\n必须JSON：{{\"action\":\"long\"|\"short\"|\"hold\",\"confidence\":0.0-1.0,\"stop_loss\":价格,\"take_profit\":[价格],\"leverage\":1-5,\"reasoning\":\"理由\"}}。只输出JSON。", current_price, credibility_str, if recent_decisions.is_empty() { String::new() } else { format!("\n\n历史决策参考:\n{}", history_str) }),
        &format!("交易对: {}\n当前价: {:.6}\n买卖力量比: {:.4}\n{}\n\n分析师交叉辩论意见:\n{}\n\n部门报告:\n{}\n\n请做最终决策，参考历史决策表现。优先考虑辩论中一致性强的信号。",
            symbol, current_price, bid_ask_imbalance, oi_change_hint,
            serde_json::to_string_pretty(&agent_opinions).unwrap_or_default(),
            serde_json::to_string_pretty(&department_reports).unwrap_or_default(),
        ),
        "fund_manager",
    ).await {
        Ok(response) => {
            let parsed = parse_fund_manager_response(&response, current_price);
            json!({
                "action": parsed.action, "confidence": parsed.confidence,
                "stop_loss": parsed.stop_loss, "take_profit": parsed.take_profit,
                "leverage": parsed.leverage, "reasoning": parsed.reasoning,
            })
        }
        Err(_) => {
            let dept_consensuses: Vec<&str> = department_reports.iter()
                .filter_map(|r| r.get("consensus").and_then(|v| v.as_str())).collect();
            let bull = dept_consensuses.iter().filter(|&&c| c == "bullish").count();
            let bear = dept_consensuses.iter().filter(|&&c| c == "bearish").count();
            json!({
                "action": if bull > bear { "long" } else if bear > bull { "short" } else { "hold" },
                "confidence": 0.4, "stop_loss": current_price * 0.97,
                "take_profit": [current_price * 1.03, current_price * 1.05],
                "leverage": 2, "reasoning": "LLM不可用，基于部门多数意见决策",
            })
        }
    };

    // Extract decision values
    let action = fund_manager_decision.get("action").and_then(|v| v.as_str()).unwrap_or("hold").to_string();
    let confidence = fund_manager_decision.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.4);

    // Apply enhanced confidence calibration based on market context
    let historical_accuracy = trends_from_decisions(pool, user_id, symbol).await;
    let enhanced_confidence = calculate_enhanced_confidence(
        confidence,
        &agent_opinions,
        &ctx.volatility,
        multi_timeframe_data.alignment,
        historical_accuracy,
    );

    let stop_loss = fund_manager_decision.get("stop_loss").and_then(|v| v.as_f64()).unwrap_or(current_price * 0.97);
    let take_profit = fund_manager_decision.get("take_profit")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_else(|| vec![current_price * 1.03, current_price * 1.05]);
    let leverage = fund_manager_decision.get("leverage").and_then(|v| v.as_i64()).unwrap_or(2) as i32;
    let reasoning = fund_manager_decision.get("reasoning").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // Enhanced reasoning with market context
    let enhanced_reasoning = format!(
        "{} | 市场环境: 趋势{} 强度{:.1}%, 波动性{}, 成交量{}, RSI {:.1}, MACD {}, 多周期一致性{:.1}%({})",
        reasoning,
        ctx.trend, ctx.trend_strength * 100.0,
        ctx.volatility, ctx.volume_profile,
        ctx.rsi_14, ctx.macd_signal,
        multi_timeframe_data.alignment * 100.0,
        multi_timeframe_data.alignment_details,
    );

    // 7. Save debate session with enhanced data
    let enhanced_decision = json!({
        "action": action,
        "confidence": enhanced_confidence,
        "stop_loss": stop_loss,
        "take_profit": take_profit,
        "leverage": leverage,
        "reasoning": enhanced_reasoning,
        "market_context": {
            "trend": ctx.trend,
            "trend_strength": ctx.trend_strength,
            "volatility": ctx.volatility,
            "volume_profile": ctx.volume_profile,
            "key_levels": [ctx.key_levels.0, ctx.key_levels.1, ctx.key_levels.2],
            "ma5": ctx.ma5,
            "ma10": ctx.ma10,
            "ma20": ctx.ma20,
            "rsi_14": ctx.rsi_14,
            "macd_signal": ctx.macd_signal,
            "atr": ctx.atr,
            "price_change_1h": ctx.price_change_1h,
            "price_change_4h": ctx.price_change_4h,
        },
        "multi_timeframe": {
            "m5_trend": multi_timeframe_data.m5_trend,
            "m15_trend": multi_timeframe_data.m15_trend,
            "h1_trend": multi_timeframe_data.h1_trend,
            "h4_trend": multi_timeframe_data.h4_trend,
            "d1_trend": multi_timeframe_data.d1_trend,
            "alignment": multi_timeframe_data.alignment,
            "alignment_details": multi_timeframe_data.alignment_details,
        },
    });

    let _ = sqlx::query(
        r#"UPDATE debate_sessions SET status = 'completed', progress = 'completed',
            agent_opinions = $2, department_reports = $3, fund_manager_decision = $4, updated_at = NOW()
        WHERE id = $1"#
    )
    .bind(session_id)
    .bind(serde_json::to_value(&agent_opinions).unwrap_or(json!([])))
    .bind(serde_json::to_value(&department_reports).unwrap_or(json!([])))
    .bind(&enhanced_decision)
    .execute(pool)
    .await;

    Ok(DebateResult {
        session_id,
        action,
        confidence: enhanced_confidence,
        stop_loss,
        take_profit,
        leverage,
        reasoning: enhanced_reasoning,
        agent_opinions,
        department_reports,
    })
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
