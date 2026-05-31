use axum::{
    extract::{State, Query, Path},
    routing::{get, post, put, delete},
    Router,
    Json,
};
use serde::Deserialize;

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
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) as count FROM reports"#,
    )
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let by_type = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT report_type::text as report_type, COUNT(*) as count FROM reports GROUP BY report_type
        ) AS sq"#,
    )
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

    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, report_type::text as report_type, status::text as status, created_at FROM reports
            WHERE ($1::text IS NULL OR report_type::text = $1) AND ($2::text IS NULL OR status::text = $2)
            ORDER BY created_at DESC LIMIT $3 OFFSET $4
        ) AS sq"#,
    )
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

    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, report_type::text as report_type, status::text as status, created_at FROM reports
            WHERE title ILIKE $1
            ORDER BY created_at DESC LIMIT $2 OFFSET $3
        ) AS sq"#,
    )
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
    Path(report_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let report = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, report_type::text as report_type, status::text as status, created_at FROM reports WHERE id = $1
        ) AS sq"#,
    )
    .bind(report_id)
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
    let report = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (INSERT INTO reports (title, content, report_type, status) VALUES ($1, $2, $3::report_type_enum, 'DRAFT') RETURNING id, title, report_type::text as report_type, status::text as status, created_at)
        SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(req.title)
    .bind(req.content)
    .bind(req.report_type.unwrap_or_else(|| "DAILY".to_string()))
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
    Path(report_id): Path<i32>,
    Json(req): Json<UpdateReportRequest>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE reports SET title = COALESCE($2, title), content = COALESCE($3, content), report_type = COALESCE($4::report_type_enum, report_type), updated_at = NOW() WHERE id = $1 RETURNING id"#,
    )
    .bind(report_id)
    .bind(req.title)
    .bind(req.content)
    .bind(req.report_type)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Report not found".to_string()))?;

    Ok(Json(serde_json::json!({"message": "Report updated"})))
}

async fn delete_report(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(report_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(
        r#"DELETE FROM reports WHERE id = $1"#,
    )
    .bind(report_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Report not found".to_string()));
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
    Path(report_id): Path<i32>,
    Json(req): Json<ExportRequest>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id FROM reports WHERE id = $1
        ) AS sq"#,
    )
    .bind(report_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Report not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "download_url": format!("/api/v1/reports/download/{}_{}.{}", report_id, req.format, req.format.to_lowercase()),
        "format": req.format,
        "status": "ready",
    })))
}

#[derive(Debug, Deserialize)]
struct CompareRequest {
    report_ids: Vec<i32>,
}

async fn compare_reports(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CompareRequest>,
) -> Result<Json<serde_json::Value>> {
    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, report_type::text as report_type, created_at FROM reports WHERE id = ANY($1)
        ) AS sq"#,
    )
    .bind(&req.report_ids)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"reports": reports})))
}

async fn get_recent_reports(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, report_type::text as report_type, status::text as status, created_at FROM reports ORDER BY created_at DESC LIMIT 5
        ) AS sq"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"reports": reports})))
}
