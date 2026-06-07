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
) -> Result<String> {
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
                    return Err(AppError::Validation(
                        "未配置 AI 模型，请在系统设置中配置 API Key（支持 OpenAI/DeepSeek/Anthropic）".to_string()
                    ));
                }
            }
        }
        Err(e) => {
            warn!("Failed to get LLM client from DB: {}, falling back to env", e);
            match get_llm_client_from_env() {
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

/// Analyze market context from candles
fn analyze_market_context(candles: &[OkxCandle], current_price: f64) -> (String, f64, String, String, (f64, f64, f64)) {
    if candles.len() < 10 {
        return ("unknown".to_string(), 0.5, "medium".to_string(), "stable".to_string(),
                (current_price * 0.95, current_price, current_price * 1.05));
    }

    // Calculate trend using moving averages
    let recent_closes: Vec<f64> = candles.iter().rev().take(20)
        .filter_map(|c| c.c.as_ref().and_then(|s| s.parse::<f64>().ok()))
        .collect();
    let ma5: f64 = recent_closes.iter().take(5).sum::<f64>() / 5.0;
    let ma20: f64 = recent_closes.iter().sum::<f64>() / recent_closes.len() as f64;

    let trend = if ma5 > ma20 * 1.02 {
        "bull".to_string()
    } else if ma5 < ma20 * 0.98 {
        "bear".to_string()
    } else {
        "range".to_string()
    };

    // Trend strength based on distance from MAs
    let trend_strength = ((ma5 - ma20) / ma20).abs().min(0.1) * 10.0;

    // Calculate volatility (ATR-based)
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
    let avg_vol = recent_vol.iter().sum::<f64>() / recent_vol.len() as f64;
    let volume_profile = if recent_vol.is_empty() || avg_vol == 0.0 {
        "stable".to_string()
    } else if recent_vol[0] > avg_vol * 1.3 {
        "increasing".to_string()
    } else if recent_vol[0] < avg_vol * 0.7 {
        "decreasing".to_string()
    } else {
        "stable".to_string()
    };

    // Key levels (support and resistance)
    let high = candles.iter()
        .filter_map(|c| c.h.as_ref().and_then(|s| s.parse::<f64>().ok()))
        .fold(f64::MIN, f64::max);
    let low = candles.iter()
        .filter_map(|c| c.l.as_ref().and_then(|s| s.parse::<f64>().ok()))
        .fold(f64::MAX, f64::min);
    let key_levels = (
        low,
        (high + low) / 2.0,
        high,
    );

    (trend, trend_strength, volatility, volume_profile, key_levels)
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

/// Calculate enhanced confidence with market context calibration
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
    // 50% base + 25% agreement + 15% alignment + 10% historical
    let final_confidence = base_confidence * 0.5 +
                          agreement * 0.25 +
                          alignment_factor * 0.15 +
                          historical_factor * 0.1;

    // Apply volatility dampening
    let calibrated_confidence = final_confidence * volatility_factor;

    // Clamp to reasonable range
    calibrated_confidence.max(0.2).min(0.98)
}

/// Extract trend patterns from historical decisions (simplified version)
fn trends_from_decisions(_decisions_len: usize) -> f64 {
    // This is a simplified version - in production you'd query decision_memory directly
    // For now, return neutral accuracy
    0.5
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
    let okx_client = crate::exchanges::okx::OkxClient::new(api_key, secret, passphrase, is_demo);

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
    let long_short_ratio = long_short_data.get("data").and_then(|d| d.as_array())
        .and_then(|arr| arr.first()).and_then(|item| item.as_array())
        .and_then(|pair| pair.get(1)).and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1.0);

    let recent_candles: Vec<Value> = candles.iter().take(20).map(|c| json!({
        "open": c.o, "high": c.h, "low": c.l, "close": c.c, "volume": c.vol,
    })).collect();

    // Analyze market context: trend, volatility, volume profile
    let (trend, trend_strength, volatility, volume_profile, key_levels) = analyze_market_context(&candles, current_price);

    // Fetch multi-timeframe data
    let multi_timeframe_data = fetch_multi_timeframe_data(&okx_client, symbol, current_price).await;

    let market_data_str = format!(
        "## 实时市场数据
交易对: {}
当前价格: {:.2}

### 市场环境分析
趋势: {} (强度: {:.0}%)
波动性: {}
成交量: {}
关键支撑/阻力位: {:.2}, {:.2}, {:.2}

### 多时间框架分析
5分钟趋势: {}
15分钟趋势: {}
1小时趋势: {}
4小时趋势: {}
日线趋势: {}
周期一致性: {:.0}% ({})
近期K线: {}

24h涨跌: {:.2}%
24h最高/最低: {:.2}/{:.2}
24h成交量: {:.2}
资金费率: {:.6}
多空比: {:.4}",
        symbol, current_price,
        trend, trend_strength * 100.0,
        volatility, volume_profile,
        key_levels.0, key_levels.1, key_levels.2,
        multi_timeframe_data.m5_trend,
        multi_timeframe_data.m15_trend,
        multi_timeframe_data.h1_trend,
        multi_timeframe_data.h4_trend,
        multi_timeframe_data.d1_trend,
        multi_timeframe_data.alignment * 100.0,
        multi_timeframe_data.alignment_details,
        serde_json::to_string_pretty(&recent_candles).unwrap_or_default(),
        if open_24h > 0.0 { (current_price - open_24h) / open_24h * 100.0 } else { 0.0 },
        high_24h, low_24h, vol_24h, funding_rate, long_short_ratio,
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

    // 4. Create debate session
    let session_row = sqlx::query(
        r#"INSERT INTO debate_sessions (user_id, symbol, status, progress, agent_opinions, department_reports, fund_manager_decision, created_at, updated_at)
        VALUES ($1, $2, 'in_progress', 'auto_simulation', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb, NOW(), NOW())
        RETURNING id"#
    )
    .bind(user_id)
    .bind(symbol)
    .fetch_one(pool)
    .await?;

    let session_id: Uuid = session_row.get::<Uuid, _>("id");

    // 5. Run 6 agents (with credibility weights injected)
    let mut agent_opinions: Vec<Value> = Vec::new();
    for agent_def in AGENTS {
        let agent_credibility = credibility_map.get(agent_def.name).copied().unwrap_or(0.5);
        let system_prompt = format!(
            "你是{}的{}，名叫{}。性格：{}。\n\
            你的历史可信度评分: {:.0}%（基于历史预测准确率）\n\
            基于OKX实时市场数据分析。必须JSON回复：\n\
            {{\"sentiment\": \"bullish\"|\"bearish\", \"confidence\": 0.5-1.0, \"analysis\": \"分析\", \"key_factors\": [\"因素\"]}}\n\
            模拟交易验证，必须给出明确方向。只输出JSON。",
            match agent_def.department {
                "technical" => "技术分析部", "capital" => "资金分析部", "news" => "新闻分析部", _ => "分析部",
            },
            agent_def.role, agent_def.name, agent_def.personality,
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

    // 5. Department reports
    let mut department_reports: Vec<Value> = Vec::new();
    for dept in &["technical", "capital", "news"] {
        let dept_name = match *dept { "technical" => "技术分析部", "capital" => "资金分析部", "news" => "新闻分析部", _ => "分析部" };
        let dept_opinions: Vec<&Value> = agent_opinions.iter()
            .filter(|o| o.get("department").and_then(|v| v.as_str()) == Some(*dept)).collect();

        let report = match analyze_with_llm(
            pool, user_id,
            &format!("你是{}汇总分析师。综合部门意见给出JSON：{{\"consensus\":\"bullish\"|\"bearish\", \"bull_summary\":\"理由\", \"bear_summary\":\"理由\"}}。只输出JSON。", dept_name),
            &format!("{}意见:\n{}", dept_name, serde_json::to_string_pretty(&dept_opinions).unwrap_or_default()),
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

    // 7. Fund manager decision (with historical reflection)
    let fund_manager_decision = match analyze_with_llm(
        pool, user_id,
        &format!("你是基金经理。综合各部门报告做决策。当前价: {:.2}。{}{}\n必须JSON：{{\"action\":\"long\"|\"short\"|\"hold\",\"confidence\":0.0-1.0,\"stop_loss\":价格,\"take_profit\":[价格],\"leverage\":1-5,\"reasoning\":\"理由\"}}。模拟交易需积极验证。只输出JSON。", current_price, credibility_str, if recent_decisions.is_empty() { String::new() } else { format!("\n\n历史决策参考:\n{}", history_str) }),
        &format!("交易对: {}\n\n分析师意见:\n{}\n\n部门报告:\n{}\n\n请做最终决策，参考历史决策表现。",
            symbol,
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
    let enhanced_confidence = calculate_enhanced_confidence(
        confidence,
        &agent_opinions,
        &volatility,
        multi_timeframe_data.alignment,
        trends_from_decisions(recent_decisions.len()),
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
        "{} | 市场环境: 趋势{} 强度{:.0}%, 波动性{}, 成交量{}, 多周期一致性{:.0}%({})",
        reasoning,
        trend, trend_strength * 100.0,
        volatility, volume_profile,
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
            "trend": trend,
            "trend_strength": trend_strength,
            "volatility": volatility,
            "volume_profile": volume_profile,
            "key_levels": [key_levels.0, key_levels.1, key_levels.2],
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
