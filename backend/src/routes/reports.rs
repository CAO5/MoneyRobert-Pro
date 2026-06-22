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
            SELECT format as report_type, COUNT(*) as count
            FROM reports
            WHERE user_id = $1
            GROUP BY format
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
            SELECT id, title, format as report_type, status, created_at
            FROM reports
            WHERE user_id = $1
              AND ($2::text IS NULL OR format = $2)
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
            SELECT id, title, format as report_type, status, created_at
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
) -> Result<Json<serde_json::Value>> {
    // 按 user_id 隔离：仅返回属于当前用户的报告
    let report = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, format as report_type, status, created_at
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

    Ok(Json(serde_json::json!({"data": report})))
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
    // 创建时绑定 user_id
    let report = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (
            INSERT INTO reports (user_id, title, content, format, status)
            VALUES ($1, $2, $3, $4, 'generated')
            RETURNING id, title, format as report_type, status, created_at
        )
        SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(user.user_id)
    .bind(req.title)
    .bind(req.content)
    .bind(req.report_type.unwrap_or_else(|| "markdown".to_string()))
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
               format = COALESCE($5, format)
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
            SELECT id, title, content, format as report_type, created_at
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
            SELECT id, title, format as report_type, status, created_at
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
