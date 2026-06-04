use crate::agents::errors::{AgentError, AgentResult};
use crate::agents::models::AgentDepartment;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::env;
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: i32,
    pub temperature: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LlmProvider {
    OpenAI,
    DeepSeek,
    Anthropic,
    Custom,
}

impl Default for LlmConfig {
    fn default() -> Self {
        let provider = match env::var("LLM_PROVIDER")
            .unwrap_or_else(|_| "openai".to_string())
            .to_lowercase()
            .as_str()
        {
            "deepseek" => LlmProvider::DeepSeek,
            "anthropic" => LlmProvider::Anthropic,
            "custom" => LlmProvider::Custom,
            _ => LlmProvider::OpenAI,
        };

        let (default_url, default_model) = match &provider {
            LlmProvider::OpenAI => (
                "https://api.openai.com/v1".to_string(),
                "gpt-4o-mini".to_string(),
            ),
            LlmProvider::DeepSeek => (
                "https://api.deepseek.com/v1".to_string(),
                "deepseek-chat".to_string(),
            ),
            LlmProvider::Anthropic => (
                "https://api.anthropic.com/v1".to_string(),
                "claude-3-haiku-20240307".to_string(),
            ),
            LlmProvider::Custom => (
                env::var("LLM_BASE_URL").unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
                env::var("LLM_MODEL").unwrap_or_else(|_| "local-model".to_string()),
            ),
        };

        Self {
            provider,
            api_key: env::var("LLM_API_KEY").unwrap_or_default(),
            base_url: env::var("LLM_BASE_URL").unwrap_or(default_url),
            model: env::var("LLM_MODEL").unwrap_or(default_model),
            max_tokens: env::var("LLM_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2048),
            temperature: env::var("LLM_TEMPERATURE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.7),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: i32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

#[derive(Debug, Clone)]
pub struct LlmClient {
    config: LlmConfig,
    http: reqwest::Client,
    db: Option<PgPool>,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .no_proxy()
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            http,
            db: None,
        }
    }

    pub fn with_db(mut self, db: PgPool) -> Self {
        self.db = Some(db);
        self
    }

    pub fn from_env() -> AgentResult<Self> {
        let config = LlmConfig::default();
        if config.api_key.is_empty() && config.provider != LlmProvider::Custom {
            return Err(AgentError::ConfigurationError(
                "LLM_API_KEY is not set. Please configure it in environment variables or .env file".to_string(),
            ));
        }
        Ok(Self::new(config))
    }

    pub async fn chat(&self, messages: Vec<ChatMessage>) -> AgentResult<String> {
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: Some(self.config.max_tokens),
            temperature: Some(self.config.temperature),
        };

        debug!("LLM request: model={}, messages_count={}", self.config.model, request.messages.len());

        let url = format!("{}/chat/completions", self.config.base_url);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::ExternalApiError(format!("LLM API request failed: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| AgentError::ExternalApiError(format!("Failed to read LLM response: {}", e)))?;

        if !status.is_success() {
            warn!("LLM API error: status={}, body={}", status, &body[..body.len().min(500)]);
            return Err(AgentError::ExternalApiError(format!(
                "LLM API returned {}: {}",
                status,
                &body[..body.len().min(200)]
            )));
        }

        let completion: ChatCompletionResponse = serde_json::from_str(&body)
            .map_err(|e| AgentError::ExternalApiError(format!("Failed to parse LLM response: {}", e)))?;

        let content = completion
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        if let Some(usage) = &completion.usage {
            debug!(
                "LLM usage: prompt={}, completion={}, total={}",
                usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
            );
            self.record_usage(usage).await;
        }

        Ok(content)
    }

    pub async fn chat_with_system(&self, system_prompt: &str, user_message: &str) -> AgentResult<String> {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_message.to_string(),
            },
        ];
        self.chat(messages).await
    }

    pub async fn analyze_as_agent(
        &self,
        agent_name: &str,
        department: &AgentDepartment,
        role: &str,
        personality: &[&str],
        market_data: &str,
        debate_context: Option<&str>,
    ) -> AgentResult<AgentAnalysisResult> {
        let dept_name = match department {
            AgentDepartment::Technical => "技术分析",
            AgentDepartment::Capital => "资金分析",
            AgentDepartment::News => "新闻分析",
            AgentDepartment::FundManager => "基金经理",
        };

        let personality_str = personality.join("、");

        let system_prompt = format!(
            "你是{}的{}，名叫{}。你的性格特点是：{}。\n\
            你需要基于提供的市场数据，从你的专业角度进行分析。\n\
            你必须以JSON格式回复，格式如下：\n\
            {{\"sentiment\": \"bullish\"|\"bearish\"|\"neutral\"|\"cautious\", \"confidence\": 0.0-1.0, \"analysis\": \"你的详细分析\", \"key_factors\": [\"因素1\", \"因素2\"]}}\n\
            sentiment必须是bullish(看多)、bearish(看空)、neutral(中性)、cautious(谨慎)之一。\n\
            confidence必须是0到1之间的数字，表示你的信心程度。\n\
            只输出JSON，不要输出其他内容。",
            dept_name, role, agent_name, personality_str
        );

        let mut user_content = format!("当前市场数据：\n{}", market_data);

        if let Some(ctx) = debate_context {
            user_content.push_str(&format!("\n\n辩论上下文：\n{}", ctx));
        }

        let response = self.chat_with_system(&system_prompt, &user_content).await?;

        self.parse_agent_analysis(&response, agent_name, department)
    }

    fn parse_agent_analysis(
        &self,
        response: &str,
        agent_name: &str,
        department: &AgentDepartment,
    ) -> AgentResult<AgentAnalysisResult> {
        let cleaned = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let parsed: serde_json::Value = serde_json::from_str(cleaned)
            .map_err(|e| AgentError::ExternalApiError(format!(
                "Failed to parse agent analysis JSON: {}. Response: {}",
                e, &cleaned[..cleaned.len().min(200)]
            )))?;

        let sentiment_str = parsed.get("sentiment")
            .and_then(|v| v.as_str())
            .unwrap_or("neutral");

        let sentiment = match sentiment_str {
            "bullish" => AgentSentimentType::Bullish,
            "bearish" => AgentSentimentType::Bearish,
            "cautious" => AgentSentimentType::Cautious,
            _ => AgentSentimentType::Neutral,
        };

        let confidence = parsed.get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        let analysis = parsed.get("analysis")
            .and_then(|v| v.as_str())
            .unwrap_or("分析结果解析失败")
            .to_string();

        let key_factors = parsed.get("key_factors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(AgentAnalysisResult {
            agent_name: agent_name.to_string(),
            department: department.clone(),
            sentiment,
            confidence,
            analysis,
            key_factors,
            raw_response: response.to_string(),
        })
    }

    async fn record_usage(&self, usage: &Usage) {
        if let Some(db) = &self.db {
            let result = sqlx::query(
                r#"INSERT INTO llm_usage_logs (provider, model, prompt_tokens, completion_tokens, total_tokens, created_at)
                   VALUES ($1, $2, $3, $4, $5, NOW())"#,
            )
            .bind(format!("{:?}", self.config.provider))
            .bind(&self.config.model)
            .bind(usage.prompt_tokens)
            .bind(usage.completion_tokens)
            .bind(usage.total_tokens)
            .execute(db)
            .await;

            if let Err(e) = result {
                debug!("Failed to record LLM usage: {}", e);
            }
        }
    }

    pub async fn get_user_api_key(&self, db: &PgPool, user_id: i64, key_name: &str) -> AgentResult<Option<String>> {
        let row = sqlx::query_scalar::<_, String>(
            r#"SELECT secret FROM api_keys WHERE user_id = $1 AND name = $2 AND is_active = true"#
        )
        .bind(user_id)
        .bind(key_name)
        .fetch_optional(db)
        .await
        .map_err(|e| AgentError::DatabaseError(e.to_string()))?;

        Ok(row)
    }

    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    pub fn is_configured(&self) -> bool {
        !self.config.api_key.is_empty() || self.config.provider == LlmProvider::Custom
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentSentimentType {
    Bullish,
    Bearish,
    Neutral,
    Cautious,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAnalysisResult {
    pub agent_name: String,
    pub department: AgentDepartment,
    pub sentiment: AgentSentimentType,
    pub confidence: f64,
    pub analysis: String,
    pub key_factors: Vec<String>,
    pub raw_response: String,
}

impl AgentAnalysisResult {
    pub fn to_agent_sentiment(&self) -> crate::agents::models::AgentSentiment {
        match self.sentiment {
            AgentSentimentType::Bullish => crate::agents::models::AgentSentiment::Bullish,
            AgentSentimentType::Bearish => crate::agents::models::AgentSentiment::Bearish,
            AgentSentimentType::Neutral => crate::agents::models::AgentSentiment::Neutral,
            AgentSentimentType::Cautious => crate::agents::models::AgentSentiment::Cautious,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_config_default() {
        let config = LlmConfig::default();
        assert!(!config.base_url.is_empty());
        assert!(!config.model.is_empty());
        assert!(config.temperature > 0.0 && config.temperature <= 2.0);
        assert!(config.max_tokens > 0);
    }

    #[test]
    fn test_parse_agent_analysis_bullish() {
        let client = LlmClient::new(LlmConfig::default());
        let json = r#"{"sentiment": "bullish", "confidence": 0.85, "analysis": "看多", "key_factors": ["RSI超卖"]}"#;
        let result = client.parse_agent_analysis(json, "test", &AgentDepartment::Technical).unwrap();
        assert!(matches!(result.sentiment, AgentSentimentType::Bullish));
        assert!((result.confidence - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_parse_agent_analysis_bearish() {
        let client = LlmClient::new(LlmConfig::default());
        let json = r#"{"sentiment": "bearish", "confidence": 0.7, "analysis": "看空", "key_factors": []}"#;
        let result = client.parse_agent_analysis(json, "test", &AgentDepartment::Capital).unwrap();
        assert!(matches!(result.sentiment, AgentSentimentType::Bearish));
    }

    #[test]
    fn test_parse_agent_analysis_with_markdown() {
        let client = LlmClient::new(LlmConfig::default());
        let json = "```json\n{\"sentiment\": \"neutral\", \"confidence\": 0.5, \"analysis\": \"中性\"}\n```";
        let result = client.parse_agent_analysis(json, "test", &AgentDepartment::News).unwrap();
        assert!(matches!(result.sentiment, AgentSentimentType::Neutral));
    }

    #[test]
    fn test_chat_message_creation() {
        let msg = ChatMessage {
            role: "system".to_string(),
            content: "test".to_string(),
        };
        assert_eq!(msg.role, "system");
    }
}
