use crate::agents::errors::{AgentError, AgentResult};
use crate::agents::models::NotificationLevel;
use crate::websocket::WebSocketManager;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Option<i64>,
    pub title: String,
    pub content: String,
    pub level: NotificationLevel,
    pub category: NotificationCategory,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationCategory {
    System,
    Trading,
    Risk,
    Agent,
    Market,
    Promotion,
}

#[async_trait::async_trait]
pub trait NotificationService: Send + Sync {
    async fn send_notification(
        &self,
        user_id: Option<i64>,
        title: String,
        content: String,
        level: NotificationLevel,
        category: NotificationCategory,
        metadata: Option<serde_json::Value>,
    ) -> AgentResult<Uuid>;

    async fn broadcast_notification(
        &self,
        title: String,
        content: String,
        level: NotificationLevel,
        category: NotificationCategory,
        metadata: Option<serde_json::Value>,
    ) -> AgentResult<Vec<Uuid>>;

    async fn get_user_notifications(
        &self,
        user_id: i64,
        limit: Option<i64>,
        offset: Option<i64>,
        unread_only: bool,
    ) -> AgentResult<Vec<Notification>>;

    async fn mark_as_read(&self, notification_id: Uuid, user_id: i64) -> AgentResult<()>;

    async fn mark_all_as_read(&self, user_id: i64) -> AgentResult<()>;

    async fn delete_notification(&self, notification_id: Uuid, user_id: i64) -> AgentResult<()>;

    fn get_unread_count(&self, user_id: i64) -> usize;
}

pub struct DatabaseNotificationService {
    db: PgPool,
    ws_manager: Arc<WebSocketManager>,
    unread_counts: Arc<DashMap<i64, usize>>,
}

impl DatabaseNotificationService {
    pub fn new(db: PgPool, ws_manager: Arc<WebSocketManager>) -> Self {
        Self {
            db,
            ws_manager,
            unread_counts: Arc::new(DashMap::new()),
        }
    }

    async fn save_notification_to_db(
        &self,
        user_id: Option<i64>,
        title: String,
        content: String,
        level: NotificationLevel,
        category: NotificationCategory,
        metadata: Option<serde_json::Value>,
    ) -> AgentResult<Uuid> {
        let notification_id = Uuid::new_v4();
        let level_str = format!("{:?}", level);
        let category_str = format!("{:?}", category);

        sqlx::query(
            r#"INSERT INTO notifications (id, user_id, title, content, notification_type, notification_level, category, is_read, created_at, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, false, NOW(), $8)"#,
        )
        .bind(notification_id)
        .bind(user_id)
        .bind(&title)
        .bind(&content)
        .bind(&category_str)
        .bind(&level_str)
        .bind(&category_str)
        .bind(metadata)
        .execute(&self.db)
        .await?;

        if let Some(uid) = user_id {
            self.unread_counts
                .entry(uid)
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }

        Ok(notification_id)
    }

    fn send_websocket_notification(
        &self,
        user_id: Option<i64>,
        notification: &Notification,
    ) {
        let message = serde_json::json!({
            "type": "notification",
            "data": notification,
            "timestamp": Utc::now().timestamp(),
        });

        let message_str = message.to_string();

        if let Some(uid) = user_id {
            self.ws_manager.broadcast_to_user(uid, &message_str);
        } else {
            self.ws_manager
                .broadcast_to_all(axum::extract::ws::Message::Text(
                    axum::extract::ws::Utf8Bytes::from(message_str),
                ));
        }
    }

    pub async fn load_unread_counts(&self) -> AgentResult<()> {
        let rows = sqlx::query(
            r#"SELECT user_id, COUNT(*) as count FROM notifications WHERE user_id IS NOT NULL AND is_read = false GROUP BY user_id"#,
        )
        .fetch_all(&self.db)
        .await?;

        for row in rows {
            let user_id: i64 = row.get("user_id");
            let count: i64 = row.get("count");
            self.unread_counts.insert(user_id, count as usize);
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl NotificationService for DatabaseNotificationService {
    async fn send_notification(
        &self,
        user_id: Option<i64>,
        title: String,
        content: String,
        level: NotificationLevel,
        category: NotificationCategory,
        metadata: Option<serde_json::Value>,
    ) -> AgentResult<Uuid> {
        let notification_id = self
            .save_notification_to_db(
                user_id,
                title.clone(),
                content.clone(),
                level.clone(),
                category.clone(),
                metadata.clone(),
            )
            .await?;

        let notification = Notification {
            id: notification_id,
            user_id,
            title,
            content,
            level,
            category,
            is_read: false,
            created_at: Utc::now(),
            metadata,
        };

        self.send_websocket_notification(user_id, &notification);

        tracing::info!(
            "Notification sent: {} (user: {:?}, level: {:?})",
            notification_id,
            user_id,
            notification.level
        );

        Ok(notification_id)
    }

    async fn broadcast_notification(
        &self,
        title: String,
        content: String,
        level: NotificationLevel,
        category: NotificationCategory,
        metadata: Option<serde_json::Value>,
    ) -> AgentResult<Vec<Uuid>> {
        let notification_id = self
            .save_notification_to_db(
                None,
                title.clone(),
                content.clone(),
                level.clone(),
                category.clone(),
                metadata.clone(),
            )
            .await?;

        let notification = Notification {
            id: notification_id,
            user_id: None,
            title,
            content,
            level,
            category,
            is_read: false,
            created_at: Utc::now(),
            metadata,
        };

        self.send_websocket_notification(None, &notification);

        tracing::info!(
            "Notification broadcast: {} (level: {:?})",
            notification_id,
            notification.level
        );

        Ok(vec![notification_id])
    }

    async fn get_user_notifications(
        &self,
        user_id: i64,
        limit: Option<i64>,
        offset: Option<i64>,
        unread_only: bool,
    ) -> AgentResult<Vec<Notification>> {
        let query = if unread_only {
            r#"SELECT id, user_id, title, content, notification_level, category, is_read, created_at, metadata
               FROM notifications
               WHERE user_id = $1 AND is_read = false
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#
        } else {
            r#"SELECT id, user_id, title, content, notification_level, category, is_read, created_at, metadata
               FROM notifications
               WHERE user_id = $1 OR user_id IS NULL
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#
        };

        let rows = sqlx::query(query)
            .bind(user_id)
            .bind(limit.unwrap_or(50))
            .bind(offset.unwrap_or(0))
            .fetch_all(&self.db)
            .await?;

        let mut notifications = Vec::new();
        for row in rows {
            let level_str: String = row.get("notification_level");
            let level = match level_str.as_str() {
                "Info" => NotificationLevel::Info,
                "Warning" => NotificationLevel::Warning,
                "Error" => NotificationLevel::Error,
                "Critical" => NotificationLevel::Critical,
                "Emergency" => NotificationLevel::Emergency,
                _ => NotificationLevel::Info,
            };

            let category_str: String = row.get("category");
            let category = match category_str.as_str() {
                "System" => NotificationCategory::System,
                "Trading" => NotificationCategory::Trading,
                "Risk" => NotificationCategory::Risk,
                "Agent" => NotificationCategory::Agent,
                "Market" => NotificationCategory::Market,
                "Promotion" => NotificationCategory::Promotion,
                _ => NotificationCategory::System,
            };

            notifications.push(Notification {
                id: row.get("id"),
                user_id: row.get("user_id"),
                title: row.get("title"),
                content: row.get("content"),
                level,
                category,
                is_read: row.get("is_read"),
                created_at: row.get("created_at"),
                metadata: row.get("metadata"),
            });
        }

        Ok(notifications)
    }

    async fn mark_as_read(&self, notification_id: Uuid, user_id: i64) -> AgentResult<()> {
        let result = sqlx::query(
            r#"UPDATE notifications SET is_read = true WHERE id = $1 AND user_id = $2"#,
        )
        .bind(notification_id)
        .bind(user_id)
        .execute(&self.db)
        .await?;

        if result.rows_affected() > 0 {
            self.unread_counts
                .entry(user_id)
                .and_modify(|count| *count = count.saturating_sub(1));
        }

        Ok(())
    }

    async fn mark_all_as_read(&self, user_id: i64) -> AgentResult<()> {
        sqlx::query(
            r#"UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false"#,
        )
        .bind(user_id)
        .execute(&self.db)
        .await?;

        self.unread_counts.insert(user_id, 0);
        Ok(())
    }

    async fn delete_notification(&self, notification_id: Uuid, user_id: i64) -> AgentResult<()> {
        let row = sqlx::query(
            r#"SELECT is_read FROM notifications WHERE id = $1 AND user_id = $2"#,
        )
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await?;

        let was_unread = row.map(|r| !r.get::<bool, _>("is_read")).unwrap_or(false);

        sqlx::query(r#"DELETE FROM notifications WHERE id = $1 AND user_id = $2"#)
            .bind(notification_id)
            .bind(user_id)
            .execute(&self.db)
            .await?;

        if was_unread {
            self.unread_counts
                .entry(user_id)
                .and_modify(|count| *count = count.saturating_sub(1));
        }

        Ok(())
    }

    fn get_unread_count(&self, user_id: i64) -> usize {
        self.unread_counts.get(&user_id).map(|c| *c).unwrap_or(0)
    }
}

pub struct NotificationBuilder {
    user_id: Option<i64>,
    title: Option<String>,
    content: Option<String>,
    level: NotificationLevel,
    category: NotificationCategory,
    metadata: Option<serde_json::Value>,
}

impl NotificationBuilder {
    pub fn new() -> Self {
        Self {
            user_id: None,
            title: None,
            content: None,
            level: NotificationLevel::Info,
            category: NotificationCategory::System,
            metadata: None,
        }
    }

    pub fn user_id(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn level(mut self, level: NotificationLevel) -> Self {
        self.level = level;
        self
    }

    pub fn category(mut self, category: NotificationCategory) -> Self {
        self.category = category;
        self
    }

    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub async fn send(self, service: &impl NotificationService) -> AgentResult<Uuid> {
        let title = self
            .title
            .ok_or_else(|| AgentError::ValidationError("Notification title is required".into()))?;
        let content = self
            .content
            .ok_or_else(|| AgentError::ValidationError("Notification content is required".into()))?;

        service
            .send_notification(
                self.user_id,
                title,
                content,
                self.level,
                self.category,
                self.metadata,
            )
            .await
    }
}

impl Default for NotificationBuilder {
    fn default() -> Self {
        Self::new()
    }
}
