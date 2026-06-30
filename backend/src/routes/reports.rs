use axum::{
    extract::{State, Query, Path},
    routing::{get, post, put, delete},
    Router,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/statistics", get(get_report_statistics))
        .route("/", get(list_reports))
        .route("/search", get(search_reports))
        .route("/{report_id}", get(get_report))
        .route("/", post(create_report))
        .route("/{report_id}", put(update_report))
        .route("/{report_id}", delete(delete_report))
        .route("/{report_id}/export", post(export_report))
        .route("/compare", post(compare_reports))
        .route("/recent", get(get_recent_reports))
}

#[derive(Debug, Deserialize)]
struct ReportQuery {
    report_type: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ReportLocaleQuery {
    locale: Option<String>,
}

fn normalize_report_locale(locale: Option<&str>) -> &'static str {
    let value = locale.unwrap_or("en-US").replace('_', "-").to_lowercase();
    if value == "zh-tw" || value == "zh-hk" || value.contains("hant") {
        "zh-TW"
    } else if value.starts_with("zh") {
        "zh-CN"
    } else {
        "en-US"
    }
}

fn infer_legacy_report_locale(report: &serde_json::Value) -> &'static str {
    let text = format!(
        "{} {}",
        report.get("title").and_then(|value| value.as_str()).unwrap_or_default(),
        report.get("content").map(|value| value.to_string()).unwrap_or_default()
    );
    if text.chars().any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character)) {
        "zh-CN"
    } else {
        "en-US"
    }
}

/// Selects exactly the requested report translation. A mismatched legacy body is
/// never silently returned because that would produce a mixed-language report.
fn localize_report(mut report: serde_json::Value, requested_locale: &str) -> serde_json::Value {
    let translations = report
        .get("content")
        .and_then(|content| content.get("translations"))
        .and_then(|translations| translations.as_object());

    let available_locales: Vec<String> = translations
        .map(|items| items.keys().cloned().collect())
        .unwrap_or_else(|| vec![infer_legacy_report_locale(&report).to_string()]);

    let localized = translations.and_then(|items| items.get(requested_locale)).cloned();
    let has_translation = localized.is_some();
    let source_locale = report
        .get("content")
        .and_then(|content| content.get("locale"))
        .and_then(|locale| locale.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| infer_legacy_report_locale(&report).to_string());
    let language_match = has_translation || source_locale == requested_locale;
    let response_locale = if has_translation {
        requested_locale.to_string()
    } else {
        source_locale.clone()
    };

    if let Some(object) = report.as_object_mut() {
        if let Some(translation) = localized {
            if let Some(title) = translation.get("title").and_then(|value| value.as_str()) {
                object.insert("title".to_string(), serde_json::Value::String(title.to_string()));
            }
            object.insert(
                "content".to_string(),
                translation
                    .get("content")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            );
        } else if !language_match {
            object.insert("content".to_string(), serde_json::Value::Null);
        }
        object.insert(
            "locale".to_string(),
            serde_json::Value::String(response_locale),
        );
        object.insert(
            "requested_locale".to_string(),
            serde_json::Value::String(requested_locale.to_string()),
        );
        object.insert(
            "language_match".to_string(),
            serde_json::Value::Bool(language_match),
        );
        object.insert(
            "available_locales".to_string(),
            serde_json::json!(available_locales),
        );
    }

    report
}

async fn get_report_statistics(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    // 仅统计当前用户的报告
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) as count FROM reports WHERE user_id = $1"#,
    )
    .bind(user.user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let by_type = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT report_type, COUNT(*) as count
            FROM reports
            WHERE user_id = $1
            GROUP BY report_type
        ) AS sq"#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"total": total, "by_report_type": by_type})))
}

async fn list_reports(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    // 按 user_id 隔离
    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, report_type, status, created_at, updated_at
            FROM reports
            WHERE user_id = $1
              AND ($2::text IS NULL OR report_type = $2)
              AND ($3::text IS NULL OR status = $3)
            ORDER BY created_at DESC LIMIT $4 OFFSET $5
        ) AS sq"#,
    )
    .bind(user.user_id)
    .bind(query.report_type)
    .bind(query.status)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": reports, "page": page, "page_size": page_size})))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn search_reports(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;
    let pattern = format!("%{}%", query.q);

    // 按 user_id 隔离
    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, report_type, status, created_at, updated_at
            FROM reports
            WHERE user_id = $1 AND title ILIKE $2
            ORDER BY created_at DESC LIMIT $3 OFFSET $4
        ) AS sq"#,
    )
    .bind(user.user_id)
    .bind(pattern)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": reports, "page": page, "page_size": page_size})))
}

async fn get_report(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(report_id): Path<Uuid>,
    Query(query): Query<ReportLocaleQuery>,
) -> Result<Json<serde_json::Value>> {
    // 按 user_id 隔离：仅返回属于当前用户的报告
    let report = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, report_type, status, created_at, updated_at
            FROM reports
            WHERE id = $1 AND user_id = $2
        ) AS sq"#,
    )
    .bind(report_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Report not found".to_string()))?;

    let requested_locale = normalize_report_locale(query.locale.as_deref());
    Ok(Json(serde_json::json!({
        "data": localize_report(report, requested_locale)
    })))
}

#[derive(Debug, Deserialize)]
struct CreateReportRequest {
    title: String,
    content: serde_json::Value,
    report_type: Option<String>,
}

async fn create_report(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateReportRequest>,
) -> Result<Json<serde_json::Value>> {
    // 创建时绑定 user_id，content 直接作为 JSONB 写入
    let report = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (
            INSERT INTO reports (user_id, title, content, report_type, format, status)
            VALUES ($1, $2, $3, $4, $4, 'generated')
            RETURNING id, title, report_type, status, created_at
        )
        SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(user.user_id)
    .bind(req.title)
    .bind(req.content)
    .bind(req.report_type.unwrap_or_else(|| "general".to_string()))
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"data": report})))
}

#[derive(Debug, Deserialize)]
struct UpdateReportRequest {
    title: Option<String>,
    content: Option<serde_json::Value>,
    report_type: Option<String>,
}

async fn update_report(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(report_id): Path<Uuid>,
    Json(req): Json<UpdateReportRequest>,
) -> Result<Json<serde_json::Value>> {
    // 按 user_id 隔离：仅更新属于当前用户的报告
    let updated = sqlx::query_scalar::<_, i64>(
        r#"UPDATE reports
           SET title = COALESCE($3, title),
               content = COALESCE($4, content),
               report_type = COALESCE($5, report_type),
               format = COALESCE($5, format),
               updated_at = NOW()
           WHERE id = $1 AND user_id = $2
           RETURNING id"#,
    )
    .bind(report_id)
    .bind(user.user_id)
    .bind(req.title)
    .bind(req.content)
    .bind(req.report_type)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    if updated.is_none() {
        return Err(AppError::NotFound("Report not found or not owned by user".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Report updated"})))
}

async fn delete_report(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(report_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    // 按 user_id 隔离：仅删除属于当前用户的报告
    let result = sqlx::query(
        r#"DELETE FROM reports WHERE id = $1 AND user_id = $2"#,
    )
    .bind(report_id)
    .bind(user.user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Report not found or not owned by user".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Report deleted"})))
}

#[derive(Debug, Deserialize)]
struct ExportRequest {
    format: String,
}

async fn export_report(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(report_id): Path<Uuid>,
    Json(req): Json<ExportRequest>,
) -> Result<Json<serde_json::Value>> {
    // 按 user_id 隔离：仅允许导出属于当前用户的报告
    let _ = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM reports WHERE id = $1 AND user_id = $2"#,
    )
    .bind(report_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Report not found or not owned by user".to_string()))?;

    Ok(Json(serde_json::json!({
        "download_url": format!("/api/v1/reports/download/{}_{}.{}", report_id, req.format, req.format.to_lowercase()),
        "format": req.format,
        "status": "ready",
    })))
}

#[derive(Debug, Deserialize)]
struct CompareRequest {
    report_ids: Vec<Uuid>,
}

async fn compare_reports(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CompareRequest>,
) -> Result<Json<serde_json::Value>> {
    // 按 user_id 隔离：仅返回属于当前用户的报告
    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, report_type, created_at, updated_at
            FROM reports
            WHERE id = ANY($1) AND user_id = $2
        ) AS sq"#,
    )
    .bind(&req.report_ids)
    .bind(user.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"reports": reports})))
}

async fn get_recent_reports(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    // 按 user_id 隔离：仅返回当前用户最近的报告
    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, report_type, status, created_at, updated_at
            FROM reports
            WHERE user_id = $1
            ORDER BY created_at DESC LIMIT 5
        ) AS sq"#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"reports": reports})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_requested_report_translation() {
        let report = serde_json::json!({
            "title": "BTC 报告",
            "content": {
                "locale": "zh-CN",
                "translations": {
                    "zh-CN": {"title": "BTC 报告", "content": "中文正文"},
                    "en-US": {"title": "BTC Report", "content": "English body"}
                }
            }
        });

        let localized = localize_report(report, "en-US");
        assert_eq!(localized["title"], "BTC Report");
        assert_eq!(localized["content"], "English body");
        assert_eq!(localized["requested_locale"], "en-US");
        assert_eq!(localized["language_match"], true);
    }

    #[test]
    fn never_returns_mismatched_legacy_content() {
        let report = serde_json::json!({
            "title": "BTC 分析报告",
            "content": {"raw": "中文正文"}
        });

        let localized = localize_report(report, "en-US");
        assert_eq!(localized["content"], serde_json::Value::Null);
        assert_eq!(localized["language_match"], false);
    }
}
