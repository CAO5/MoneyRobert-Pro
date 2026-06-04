use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::agents::llm_client::{LlmClient, LlmConfig, LlmProvider};
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;
use crate::utils::encryption::{decrypt, encrypt};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_providers))
        .route("/", post(create_provider))
        .route("/{id}", put(update_provider))
        .route("/{id}", delete(delete_provider))
        .route("/{id}/test", post(test_provider))
}

fn mask_api_key(value: &str) -> String {
    if value.len() > 4 {
        format!("{}****", &value[..4])
    } else {
        "****".to_string()
    }
}

#[derive(Debug, Deserialize)]
struct CreateProviderRequest {
    provider: Option<String>,
    api_key: String,
    base_url: Option<String>,
    model: Option<String>,
    max_tokens: Option<i32>,
    temperature: Option<f64>,
    is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateProviderRequest {
    provider: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    max_tokens: Option<i32>,
    temperature: Option<f64>,
    is_active: Option<bool>,
    is_default: Option<bool>,
}

async fn list_providers(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        r#"SELECT id, provider, api_key_encrypted, base_url, model, max_tokens, temperature, is_active, is_default, created_at::text, updated_at::text
        FROM ai_provider_configs WHERE user_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            let encrypted_key = row.try_get::<String, _>("api_key_encrypted").unwrap_or_default();
            let decrypted_key = decrypt(&encrypted_key).unwrap_or_default();
            let masked_key = mask_api_key(&decrypted_key);

            json!({
                "id": row.get::<i32, _>("id"),
                "provider": row.try_get::<String, _>("provider").unwrap_or_else(|_| "openai".to_string()),
                "api_key": masked_key,
                "base_url": row.try_get::<Option<String>, _>("base_url").unwrap_or(None),
                "model": row.try_get::<Option<String>, _>("model").unwrap_or(None),
                "max_tokens": row.try_get::<Option<i32>, _>("max_tokens").unwrap_or(None),
                "temperature": row.try_get::<Option<f64>, _>("temperature").unwrap_or(None),
                "is_active": row.get::<bool, _>("is_active"),
                "is_default": row.get::<bool, _>("is_default"),
                "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
                "updated_at": row.try_get::<String, _>("updated_at").unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(json!({"items": items})))
}

async fn create_provider(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateProviderRequest>,
) -> Result<Json<Value>> {
    let provider_str = req.provider.unwrap_or_else(|| "openai".to_string());
    let encrypted_key = encrypt(&req.api_key)?;

    let is_default = req.is_default.unwrap_or(false);

    // If setting as default, unset other defaults
    if is_default {
        sqlx::query(r#"UPDATE ai_provider_configs SET is_default = false WHERE user_id = $1"#)
            .bind(user.user_id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| AppError::Database(e))?;
    }

    let row = sqlx::query(
        r#"INSERT INTO ai_provider_configs (user_id, provider, api_key_encrypted, base_url, model, max_tokens, temperature, is_active, is_default, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, NOW(), NOW())
        RETURNING id, provider, base_url, model, max_tokens, temperature, is_active, is_default, created_at::text, updated_at::text"#,
    )
    .bind(user.user_id)
    .bind(&provider_str)
    .bind(&encrypted_key)
    .bind(&req.base_url)
    .bind(&req.model)
    .bind(req.max_tokens)
    .bind(req.temperature)
    .bind(is_default)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let masked_key = mask_api_key(&req.api_key);

    Ok(Json(json!({
        "id": row.get::<i32, _>("id"),
        "provider": row.try_get::<String, _>("provider").unwrap_or_else(|_| "openai".to_string()),
        "api_key": masked_key,
        "base_url": row.try_get::<Option<String>, _>("base_url").unwrap_or(None),
        "model": row.try_get::<Option<String>, _>("model").unwrap_or(None),
        "max_tokens": row.try_get::<Option<i32>, _>("max_tokens").unwrap_or(None),
        "temperature": row.try_get::<Option<f64>, _>("temperature").unwrap_or(None),
        "is_active": row.get::<bool, _>("is_active"),
        "is_default": row.get::<bool, _>("is_default"),
        "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
        "updated_at": row.try_get::<String, _>("updated_at").unwrap_or_default(),
    })))
}

async fn update_provider(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<Value>> {
    let existing = sqlx::query(
        r#"SELECT provider, api_key_encrypted, base_url, model, max_tokens, temperature, is_active, is_default
        FROM ai_provider_configs WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let existing = existing.ok_or_else(|| AppError::NotFound("AI provider config not found".to_string()))?;

    let provider = req.provider.unwrap_or_else(|| {
        existing.try_get::<String, _>("provider").unwrap_or_else(|_| "openai".to_string())
    });

    let encrypted_key = match req.api_key {
        Some(ref k) => encrypt(k)?,
        None => existing.get::<String, _>("api_key_encrypted"),
    };

    let base_url = req.base_url.or_else(|| {
        existing.try_get::<Option<String>, _>("base_url").unwrap_or(None)
    });
    let model = req.model.or_else(|| {
        existing.try_get::<Option<String>, _>("model").unwrap_or(None)
    });
    let max_tokens = req.max_tokens.or_else(|| {
        existing.try_get::<Option<i32>, _>("max_tokens").unwrap_or(None)
    });
    let temperature = req.temperature.or_else(|| {
        existing.try_get::<Option<f64>, _>("temperature").unwrap_or(None)
    });
    let is_active = req.is_active.unwrap_or_else(|| existing.get::<bool, _>("is_active"));
    let is_default = req.is_default.unwrap_or_else(|| existing.get::<bool, _>("is_default"));

    // If setting as default, unset other defaults
    if is_default {
        sqlx::query(r#"UPDATE ai_provider_configs SET is_default = false WHERE user_id = $1"#)
            .bind(user.user_id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| AppError::Database(e))?;
    }

    let row = sqlx::query(
        r#"UPDATE ai_provider_configs SET provider = $3, api_key_encrypted = $4, base_url = $5, model = $6, max_tokens = $7, temperature = $8, is_active = $9, is_default = $10, updated_at = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, provider, api_key_encrypted, base_url, model, max_tokens, temperature, is_active, is_default, created_at::text, updated_at::text"#,
    )
    .bind(id)
    .bind(user.user_id)
    .bind(&provider)
    .bind(&encrypted_key)
    .bind(&base_url)
    .bind(&model)
    .bind(max_tokens)
    .bind(temperature)
    .bind(is_active)
    .bind(is_default)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("AI provider config not found".to_string()))?;

    let decrypted_key = decrypt(&row.get::<String, _>("api_key_encrypted")).unwrap_or_default();
    let masked_key = mask_api_key(&decrypted_key);

    Ok(Json(json!({
        "id": row.get::<i32, _>("id"),
        "provider": row.try_get::<String, _>("provider").unwrap_or_else(|_| "openai".to_string()),
        "api_key": masked_key,
        "base_url": row.try_get::<Option<String>, _>("base_url").unwrap_or(None),
        "model": row.try_get::<Option<String>, _>("model").unwrap_or(None),
        "max_tokens": row.try_get::<Option<i32>, _>("max_tokens").unwrap_or(None),
        "temperature": row.try_get::<Option<f64>, _>("temperature").unwrap_or(None),
        "is_active": row.get::<bool, _>("is_active"),
        "is_default": row.get::<bool, _>("is_default"),
        "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
        "updated_at": row.try_get::<String, _>("updated_at").unwrap_or_default(),
    })))
}

async fn delete_provider(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Value>> {
    let result = sqlx::query(r#"DELETE FROM ai_provider_configs WHERE id = $1 AND user_id = $2"#)
        .bind(id)
        .bind(user.user_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("AI provider config not found".to_string()));
    }

    Ok(Json(json!({
        "success": true,
        "message": "AI provider config deleted",
    })))
}

async fn test_provider(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Value>> {
    let row = sqlx::query(
        r#"SELECT provider, api_key_encrypted, base_url, model, max_tokens, temperature, is_active
        FROM ai_provider_configs WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let row = row.ok_or_else(|| AppError::NotFound("AI provider config not found".to_string()))?;

    let is_active = row.get::<bool, _>("is_active");
    if !is_active {
        return Err(AppError::Validation(
            "AI provider config is disabled, please enable it first".to_string(),
        ));
    }

    let provider_str = row.try_get::<String, _>("provider").unwrap_or_else(|_| "openai".to_string());
    let encrypted_key = row.try_get::<String, _>("api_key_encrypted").unwrap_or_default();
    let api_key = decrypt(&encrypted_key)?;

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

    let base_url = row
        .try_get::<Option<String>, _>("base_url")
        .unwrap_or(None)
        .unwrap_or(default_url);
    let model = row
        .try_get::<Option<String>, _>("model")
        .unwrap_or(None)
        .unwrap_or(default_model);
    let max_tokens = row
        .try_get::<Option<i32>, _>("max_tokens")
        .unwrap_or(None)
        .unwrap_or(2048);
    let temperature = row
        .try_get::<Option<f64>, _>("temperature")
        .unwrap_or(None)
        .unwrap_or(0.7);

    let config = LlmConfig {
        provider,
        api_key,
        base_url,
        model: model.clone(),
        max_tokens: 10,
        temperature: 0.1,
    };

    let client = LlmClient::new(config);

    match client.chat_with_system("test", "hello").await {
        Ok(response) => {
            let response_preview = if response.chars().count() > 100 {
                let preview: String = response.chars().take(100).collect();
                format!("{}...", preview)
            } else {
                response
            };
            Ok(Json(json!({
                "success": true,
                "message": "AI provider connection successful",
                "details": {
                    "provider": provider_str,
                    "model": model,
                    "max_tokens": max_tokens,
                    "temperature": temperature,
                    "response_preview": response_preview,
                }
            })))
        }
        Err(e) => Ok(Json(json!({
            "success": false,
            "message": format!("AI provider connection failed: {}", e),
        }))),
    }
}
