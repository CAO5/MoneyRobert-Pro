use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::agents::llm_client::{LlmClient, LlmConfig, LlmProvider};
use crate::error::{AppError, Result};
use crate::exchanges::okx::OkxClient;
use crate::extractors::CurrentUser;
use crate::state::AppState;
use crate::utils::encryption::{decrypt, encrypt};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_api_keys))
        .route("/", post(create_api_key))
        .route("/{key_id}", put(update_api_key))
        .route("/{key_id}", delete(delete_api_key))
        .route("/{key_id}/test", post(test_api_key))
        .route("/{key_id}/toggle", post(toggle_api_key))
}

fn mask_secret(value: &str) -> String {
    if value.len() > 4 {
        format!("{}****", &value[..4])
    } else {
        "****".to_string()
    }
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    key_type: String,
    api_key: String,
    api_secret: String,
    passphrase: Option<String>,
    is_demo: Option<bool>,
    provider: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateApiKeyRequest {
    name: Option<String>,
    api_key: Option<String>,
    api_secret: Option<String>,
    passphrase: Option<String>,
    is_demo: Option<bool>,
    provider: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
}

fn validate_key_type(key_type: &str) -> Result<()> {
    match key_type {
        "exchange" | "ai_provider" => Ok(()),
        _ => Err(AppError::Validation(
            "key_type must be 'exchange' or 'ai_provider'".to_string(),
        )),
    }
}

fn build_metadata(
    key_type: &str,
    is_demo: Option<bool>,
    provider: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
) -> serde_json::Value {
    match key_type {
        "exchange" => serde_json::json!({
            "is_demo": is_demo.unwrap_or(false),
        }),
        "ai_provider" => serde_json::json!({
            "provider": provider.unwrap_or_else(|| "openai".to_string()),
            "base_url": base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            "model": model.unwrap_or_else(|| "gpt-4o-mini".to_string()),
        }),
        _ => serde_json::json!({}),
    }
}

async fn list_api_keys(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query(
        r#"SELECT id, name, key_type, key, is_active, metadata, created_at::text as created_at, updated_at::text as updated_at
        FROM api_keys WHERE user_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(user.user_id as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let encrypted_key = row.try_get::<String, _>("key").unwrap_or_default();
            let decrypted_key = decrypt(&encrypted_key).unwrap_or_default();
            let masked_key = mask_secret(&decrypted_key);

            let key_type = row.try_get::<String, _>("key_type").unwrap_or_else(|_| "exchange".to_string());
            let metadata: serde_json::Value = row
                .try_get::<serde_json::Value, _>("metadata")
                .unwrap_or(serde_json::json!({}));

            let mut response = serde_json::json!({
                "id": row.get::<Uuid, _>("id").to_string(),
                "name": row.get::<String, _>("name"),
                "key_type": key_type,
                "api_key": masked_key,
                "is_active": row.get::<bool, _>("is_active"),
                "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
                "updated_at": row.try_get::<String, _>("updated_at").unwrap_or_default(),
            });

            if key_type == "exchange" {
                if let Some(is_demo) = metadata.get("is_demo").and_then(|v| v.as_bool()) {
                    response["is_demo"] = serde_json::json!(is_demo);
                }
            } else if key_type == "ai_provider" {
                if let Some(provider) = metadata.get("provider").and_then(|v| v.as_str()) {
                    response["provider"] = serde_json::json!(provider);
                }
                if let Some(base_url) = metadata.get("base_url").and_then(|v| v.as_str()) {
                    response["base_url"] = serde_json::json!(base_url);
                }
                if let Some(model) = metadata.get("model").and_then(|v| v.as_str()) {
                    response["model"] = serde_json::json!(model);
                }
            }

            response
        })
        .collect();

    Ok(Json(serde_json::json!({
        "items": items,
        "keys": items,
    })))
}

async fn create_api_key(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<serde_json::Value>> {
    validate_key_type(&req.key_type)?;

    let encrypted_key = encrypt(&req.api_key)?;
    let encrypted_secret = encrypt(&req.api_secret)?;
    let passphrase = req.passphrase.unwrap_or_default();
    let encrypted_passphrase = encrypt(&passphrase)?;

    let metadata = build_metadata(
        &req.key_type,
        req.is_demo,
        req.provider,
        req.base_url,
        req.model,
    );

    let row = sqlx::query(
        r#"INSERT INTO api_keys (user_id, name, key_type, key, secret, passphrase, is_active, metadata, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, true, $7, NOW(), NOW())
        RETURNING id, name, key_type, is_active, metadata, created_at::text as created_at"#,
    )
    .bind(user.user_id as i64)
    .bind(&req.name)
    .bind(&req.key_type)
    .bind(&encrypted_key)
    .bind(&encrypted_secret)
    .bind(&encrypted_passphrase)
    .bind(&metadata)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let masked_key = mask_secret(&req.api_key);
    let key_type = row.try_get::<String, _>("key_type").unwrap_or_else(|_| "exchange".to_string());
    let returned_metadata: serde_json::Value = row
        .try_get::<serde_json::Value, _>("metadata")
        .unwrap_or(serde_json::json!({}));

    let mut response = serde_json::json!({
        "id": row.get::<Uuid, _>("id").to_string(),
        "name": row.get::<String, _>("name"),
        "key_type": key_type,
        "api_key": masked_key,
        "is_active": row.get::<bool, _>("is_active"),
        "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
    });

    if key_type == "exchange" {
        if let Some(is_demo) = returned_metadata.get("is_demo").and_then(|v| v.as_bool()) {
            response["is_demo"] = serde_json::json!(is_demo);
        }
    } else if key_type == "ai_provider" {
        if let Some(provider) = returned_metadata.get("provider").and_then(|v| v.as_str()) {
            response["provider"] = serde_json::json!(provider);
        }
        if let Some(base_url) = returned_metadata.get("base_url").and_then(|v| v.as_str()) {
            response["base_url"] = serde_json::json!(base_url);
        }
        if let Some(model) = returned_metadata.get("model").and_then(|v| v.as_str()) {
            response["model"] = serde_json::json!(model);
        }
    }

    Ok(Json(response))
}

async fn update_api_key(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key_id): Path<Uuid>,
    Json(req): Json<UpdateApiKeyRequest>,
) -> Result<Json<serde_json::Value>> {
    let row = sqlx::query(
        r#"SELECT key_type, metadata, name, key, secret, passphrase FROM api_keys WHERE id = $1 AND user_id = $2"#,
    )
    .bind(key_id)
    .bind(user.user_id as i64)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let existing = row.ok_or_else(|| AppError::NotFound("API key not found".to_string()))?;

    let key_type = existing
        .try_get::<String, _>("key_type")
        .unwrap_or_else(|_| "exchange".to_string());
    let mut metadata: serde_json::Value = existing
        .try_get::<serde_json::Value, _>("metadata")
        .unwrap_or(serde_json::json!({}));

    if key_type == "exchange" {
        if let Some(is_demo) = req.is_demo {
            metadata["is_demo"] = serde_json::json!(is_demo);
        }
    } else if key_type == "ai_provider" {
        if let Some(ref provider) = req.provider {
            metadata["provider"] = serde_json::json!(provider);
        }
        if let Some(ref base_url) = req.base_url {
            metadata["base_url"] = serde_json::json!(base_url);
        }
        if let Some(ref model) = req.model {
            metadata["model"] = serde_json::json!(model);
        }
    }

    let new_name = req.name.unwrap_or_else(|| {
        existing.try_get::<String, _>("name").unwrap_or_default()
    });

    let new_encrypted_key = match req.api_key {
        Some(ref k) => encrypt(k)?,
        None => existing.try_get::<String, _>("key").unwrap_or_default(),
    };

    let new_encrypted_secret = match req.api_secret {
        Some(ref s) => encrypt(s)?,
        None => existing.try_get::<String, _>("secret").unwrap_or_default(),
    };

    let new_encrypted_passphrase = match req.passphrase {
        Some(ref p) => encrypt(p)?,
        None => existing.try_get::<String, _>("passphrase").unwrap_or_default(),
    };

    let row = sqlx::query(
        r#"UPDATE api_keys SET name = $3, key = $4, secret = $5, passphrase = $6, metadata = $7, updated_at = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, name, key_type, key, is_active, metadata, created_at::text as created_at, updated_at::text as updated_at"#,
    )
    .bind(key_id)
    .bind(user.user_id as i64)
    .bind(&new_name)
    .bind(&new_encrypted_key)
    .bind(&new_encrypted_secret)
    .bind(&new_encrypted_passphrase)
    .bind(&metadata)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("API key not found".to_string()))?;

    let encrypted_key = row.try_get::<String, _>("key").unwrap_or_default();
    let decrypted_key = decrypt(&encrypted_key).unwrap_or_default();
    let masked_key = mask_secret(&decrypted_key);

    let updated_key_type = row
        .try_get::<String, _>("key_type")
        .unwrap_or_else(|_| "exchange".to_string());
    let updated_metadata: serde_json::Value = row
        .try_get::<serde_json::Value, _>("metadata")
        .unwrap_or(serde_json::json!({}));

    let mut response = serde_json::json!({
        "id": row.get::<Uuid, _>("id").to_string(),
        "name": row.get::<String, _>("name"),
        "key_type": updated_key_type,
        "api_key": masked_key,
        "is_active": row.get::<bool, _>("is_active"),
        "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
        "updated_at": row.try_get::<String, _>("updated_at").unwrap_or_default(),
    });

    if updated_key_type == "exchange" {
        if let Some(is_demo) = updated_metadata.get("is_demo").and_then(|v| v.as_bool()) {
            response["is_demo"] = serde_json::json!(is_demo);
        }
    } else if updated_key_type == "ai_provider" {
        if let Some(provider) = updated_metadata.get("provider").and_then(|v| v.as_str()) {
            response["provider"] = serde_json::json!(provider);
        }
        if let Some(base_url) = updated_metadata.get("base_url").and_then(|v| v.as_str()) {
            response["base_url"] = serde_json::json!(base_url);
        }
        if let Some(model) = updated_metadata.get("model").and_then(|v| v.as_str()) {
            response["model"] = serde_json::json!(model);
        }
    }

    Ok(Json(response))
}

async fn delete_api_key(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(r#"DELETE FROM api_keys WHERE id = $1 AND user_id = $2"#)
        .bind(key_id)
        .bind(user.user_id as i64)
        .execute(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("API key not found".to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "API key deleted",
    })))
}

async fn toggle_api_key(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let row = sqlx::query(
        r#"UPDATE api_keys SET is_active = NOT is_active, updated_at = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, name, key_type, is_active, updated_at::text as updated_at"#,
    )
    .bind(key_id)
    .bind(user.user_id as i64)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let row = row.ok_or_else(|| AppError::NotFound("API key not found".to_string()))?;

    let is_active = row.get::<bool, _>("is_active");

    Ok(Json(serde_json::json!({
        "id": row.get::<Uuid, _>("id").to_string(),
        "name": row.get::<String, _>("name"),
        "key_type": row.try_get::<String, _>("key_type").unwrap_or_else(|_| "exchange".to_string()),
        "is_active": is_active,
        "updated_at": row.try_get::<String, _>("updated_at").unwrap_or_default(),
        "message": if is_active { "API key enabled" } else { "API key disabled" },
    })))
}

async fn test_api_key(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let row = sqlx::query(
        r#"SELECT key_type, key, secret, passphrase, is_active, metadata FROM api_keys WHERE id = $1 AND user_id = $2"#,
    )
    .bind(key_id)
    .bind(user.user_id as i64)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let row = row.ok_or_else(|| AppError::NotFound("API key not found".to_string()))?;

    let is_active = row.get::<bool, _>("is_active");
    if !is_active {
        return Err(AppError::Validation(
            "API key is disabled, please enable it first".to_string(),
        ));
    }

    let key_type = row
        .try_get::<String, _>("key_type")
        .unwrap_or_else(|_| "exchange".to_string());
    let encrypted_key = row.try_get::<String, _>("key").unwrap_or_default();
    let encrypted_secret = row.try_get::<String, _>("secret").unwrap_or_default();
    let encrypted_passphrase = row
        .try_get::<String, _>("passphrase")
        .unwrap_or_default();

    let api_key = decrypt(&encrypted_key)?;
    let api_secret = decrypt(&encrypted_secret)?;
    let passphrase = decrypt(&encrypted_passphrase).unwrap_or_default();

    let metadata: serde_json::Value = row
        .try_get::<serde_json::Value, _>("metadata")
        .unwrap_or(serde_json::json!({}));

    match key_type.as_str() {
        "exchange" => test_exchange_key(&api_key, &api_secret, &passphrase, &metadata).await,
        "ai_provider" => test_ai_provider_key(&api_key, &metadata).await,
        _ => Err(AppError::Validation(format!(
            "Unknown key_type: {}",
            key_type
        ))),
    }
}

async fn test_exchange_key(
    api_key: &str,
    api_secret: &str,
    passphrase: &str,
    metadata: &serde_json::Value,
) -> Result<Json<serde_json::Value>> {
    let is_demo = metadata
        .get("is_demo")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let client = OkxClient::new(
        api_key.to_string(),
        api_secret.to_string(),
        passphrase.to_string(),
        is_demo,
    );

    match client.get_account_balance().await {
        Ok(accounts) => {
            let total_eq = accounts
                .first()
                .map(|a| a.total_eq.clone())
                .unwrap_or_else(|| "0".to_string());

            Ok(Json(serde_json::json!({
                "success": true,
                "message": "Exchange API key connection successful",
                "key_type": "exchange",
                "details": {
                    "total_equity_usd": total_eq,
                    "account_count": accounts.len(),
                    "is_demo": is_demo,
                }
            })))
        }
        Err(e) => Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("Exchange API key connection failed: {}", e),
            "key_type": "exchange",
        }))),
    }
}

async fn test_ai_provider_key(
    api_key: &str,
    metadata: &serde_json::Value,
) -> Result<Json<serde_json::Value>> {
    let provider_str = metadata
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("openai");
    let base_url = metadata
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://api.openai.com/v1");
    let model = metadata
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-4o-mini");

    let provider = match provider_str {
        "deepseek" => LlmProvider::DeepSeek,
        "anthropic" => LlmProvider::Anthropic,
        "custom" => LlmProvider::Custom,
        _ => LlmProvider::OpenAI,
    };

    let config = LlmConfig {
        provider,
        api_key: api_key.to_string(),
        base_url: base_url.to_string(),
        model: model.to_string(),
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
            Ok(Json(serde_json::json!({
                "success": true,
                "message": "AI provider API key connection successful",
                "key_type": "ai_provider",
                "details": {
                    "provider": provider_str,
                    "model": model,
                    "response_preview": response_preview,
                }
            })))
        }
        Err(e) => Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("AI provider API key connection failed: {}", e),
            "key_type": "ai_provider",
        }))),
    }
}
