use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use tracing::{debug, warn};

use crate::agents::config::AgentConfig;
use crate::agents::llm_client::{LlmClient, LlmConfig, LlmProvider};
use crate::agents::market::{DatabaseMarketDataProvider, MarketDataProvider};
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
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
