use crate::agents::{
    errors::AgentResult,
    models::{AiSimulationConfig, AiSimulationTrade, ExecutionMode, MarketSnapshot},
};
use crate::exchanges::okx::OkxClient;
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TradeExecutionResult {
    pub trade: AiSimulationTrade,
    pub new_balance: f64,
    pub pnl: f64,
    pub pnl_percent: f64,
}

#[derive(Debug, Clone)]
pub struct StopLossTrigger {
    pub trade_id: Uuid,
    pub trigger_price: f64,
    pub current_price: f64,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct TakeProfitTrigger {
    pub trade_id: Uuid,
    pub trigger_price: f64,
    pub current_price: f64,
    pub reason: String,
}

pub struct SimulationEngine {
    pool: PgPool,
    okx_client: Option<Arc<OkxClient>>,
}

impl SimulationEngine {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            okx_client: None,
        }
    }

    pub fn with_okx_client(pool: PgPool, okx_client: Arc<OkxClient>) -> Self {
        Self {
            pool,
            okx_client: Some(okx_client),
        }
    }

    pub fn set_okx_client(&mut self, client: Arc<OkxClient>) {
        self.okx_client = Some(client);
    }

    fn parse_execution_mode(mode: &str) -> ExecutionMode {
        match mode.to_lowercase().as_str() {
            "demo" => ExecutionMode::Demo,
            "live" => ExecutionMode::Live,
            _ => ExecutionMode::Paper,
        }
    }

    fn execution_mode_to_string(mode: &ExecutionMode) -> &'static str {
        match mode {
            ExecutionMode::Paper => "paper",
            ExecutionMode::Demo => "demo",
            ExecutionMode::Live => "live",
        }
    }

    pub async fn execute_trade(
        &self,
        config: &AiSimulationConfig,
        direction: &str,
        entry_price: f64,
        quantity: f64,
        leverage: i32,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        ai_confidence: Option<f64>,
        ai_reasoning: Option<serde_json::Value>,
        agent_session_id: Option<Uuid>,
    ) -> AgentResult<TradeExecutionResult> {
        let mode = Self::parse_execution_mode(&config.mode);

        match mode {
            ExecutionMode::Paper => {
                self.execute_paper_trade(
                    config,
                    direction,
                    entry_price,
                    quantity,
                    leverage,
                    stop_loss,
                    take_profit,
                    ai_confidence,
                    ai_reasoning,
                    agent_session_id,
                )
                .await
            }
            ExecutionMode::Demo | ExecutionMode::Live => {
                self.execute_okx_trade(
                    config,
                    direction,
                    entry_price,
                    quantity,
                    leverage,
                    stop_loss,
                    take_profit,
                    ai_confidence,
                    ai_reasoning,
                    agent_session_id,
                    &mode,
                )
                .await
            }
        }
    }

    async fn execute_paper_trade(
        &self,
        config: &AiSimulationConfig,
        direction: &str,
        entry_price: f64,
        quantity: f64,
        leverage: i32,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        ai_confidence: Option<f64>,
        ai_reasoning: Option<serde_json::Value>,
        agent_session_id: Option<Uuid>,
    ) -> AgentResult<TradeExecutionResult> {
        let mut tx = self.pool.begin().await?;

        let position_value = quantity * entry_price;
        let margin_required = position_value / leverage as f64;

        if config.current_balance < margin_required {
            return Err(crate::agents::errors::AgentError::SimulationError(
                "Insufficient margin".to_string(),
            ));
        }

        let trade = sqlx::query_as!(
            AiSimulationTrade,
            r#"
            INSERT INTO ai_simulation_trades (
                config_id, symbol, mode, direction, entry_price, quantity, leverage,
                stop_loss, take_profit, ai_confidence, ai_reasoning, agent_session_id,
                status, opened_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'open', NOW())
            RETURNING *
            "#,
            config.id,
            config.symbol,
            config.mode,
            direction,
            entry_price,
            quantity,
            leverage,
            stop_loss,
            take_profit,
            ai_confidence,
            ai_reasoning,
            agent_session_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let updated_config = sqlx::query_as!(
            AiSimulationConfig,
            r#"
            UPDATE ai_simulation_configs
            SET total_trades = total_trades + 1,
                last_trade_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
            config.id
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        info!(
            "Paper trade executed: {} {} {} at {} with {}x leverage",
            direction, quantity, config.symbol, entry_price, leverage
        );

        Ok(TradeExecutionResult {
            trade,
            new_balance: updated_config.current_balance,
            pnl: 0.0,
            pnl_percent: 0.0,
        })
    }

    async fn execute_okx_trade(
        &self,
        config: &AiSimulationConfig,
        direction: &str,
        entry_price: f64,
        quantity: f64,
        leverage: i32,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        ai_confidence: Option<f64>,
        ai_reasoning: Option<serde_json::Value>,
        agent_session_id: Option<Uuid>,
        mode: &ExecutionMode,
    ) -> AgentResult<TradeExecutionResult> {
        let okx_client = self.okx_client.as_ref().ok_or_else(|| {
            crate::agents::errors::AgentError::ExecutionError(
                "OKX client not configured for live/demo trading".to_string(),
            )
        })?;

        okx_client
            .set_leverage(&config.symbol, &leverage.to_string(), "isolated")
            .await
            .map_err(|e| {
                crate::agents::errors::AgentError::ExecutionError(format!(
                    "Failed to set OKX leverage: {}",
                    e
                ))
            })?;

        let side = match direction {
            "long" => "buy",
            "short" => "sell",
            _ => "buy",
        };

        let order_request = crate::exchanges::okx::OkxOrderRequest {
            inst_id: config.symbol.clone(),
            td_mode: "isolated".to_string(),
            side: side.to_string(),
            ord_type: "market".to_string(),
            sz: quantity.to_string(),
            px: None,
            sl_trigger_px: stop_loss.map(|v| v.to_string()),
            sl_ord_px: stop_loss.map(|v| v.to_string()),
            tp_trigger_px: take_profit.map(|v| v.to_string()),
            tp_ord_px: take_profit.map(|v| v.to_string()),
        };

        let order_response = okx_client.place_order(&order_request).await.map_err(|e| {
            crate::agents::errors::AgentError::ExecutionError(format!(
                "OKX order failed: {}",
                e
            ))
        })?;

        if order_response.s_code != "0" {
            return Err(crate::agents::errors::AgentError::ExecutionError(format!(
                "OKX order rejected: {}",
                order_response.s_msg
            )));
        }

        let mut tx = self.pool.begin().await?;

        let position_value = quantity * entry_price;
        let margin_required = position_value / leverage as f64;

        if config.current_balance < margin_required {
            return Err(crate::agents::errors::AgentError::SimulationError(
                "Insufficient margin".to_string(),
            ));
        }

        let mode_str = Self::execution_mode_to_string(mode);

        let trade = sqlx::query_as!(
            AiSimulationTrade,
            r#"
            INSERT INTO ai_simulation_trades (
                config_id, symbol, mode, direction, entry_price, quantity, leverage,
                stop_loss, take_profit, ai_confidence, ai_reasoning, agent_session_id,
                status, opened_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'open', NOW())
            RETURNING *
            "#,
            config.id,
            config.symbol,
            mode_str,
            direction,
            entry_price,
            quantity,
            leverage,
            stop_loss,
            take_profit,
            ai_confidence,
            ai_reasoning,
            agent_session_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let updated_config = sqlx::query_as!(
            AiSimulationConfig,
            r#"
            UPDATE ai_simulation_configs
            SET total_trades = total_trades + 1,
                last_trade_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
            config.id
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        info!(
            "OKX trade executed: {} {} {} at {} with {}x leverage (order: {})",
            direction, quantity, config.symbol, entry_price, leverage, order_response.ord_id
        );

        Ok(TradeExecutionResult {
            trade,
            new_balance: updated_config.current_balance,
            pnl: 0.0,
            pnl_percent: 0.0,
        })
    }

    pub async fn close_trade(
        &self,
        trade_id: Uuid,
        exit_price: f64,
        close_reason: &str,
    ) -> AgentResult<TradeExecutionResult> {
        let mut tx = self.pool.begin().await?;

        let trade = sqlx::query_as!(
            AiSimulationTrade,
            "SELECT * FROM ai_simulation_trades WHERE id = $1",
            trade_id
        )
        .fetch_one(&mut *tx)
        .await?;

        if trade.status != "open" {
            return Err(crate::agents::errors::AgentError::SimulationError(
                "Trade is not open".to_string(),
            ));
        }

        let mode = Self::parse_execution_mode(&trade.mode);
        if matches!(mode, ExecutionMode::Demo | ExecutionMode::Live) {
            if let Some(okx_client) = &self.okx_client {
                let side = if trade.direction == "long" {
                    "sell"
                } else {
                    "buy"
                };

                let close_request = crate::exchanges::okx::OkxOrderRequest {
                    inst_id: trade.symbol.clone(),
                    td_mode: "isolated".to_string(),
                    side: side.to_string(),
                    ord_type: "market".to_string(),
                    sz: trade.quantity.to_string(),
                    px: None,
                    sl_trigger_px: None,
                    sl_ord_px: None,
                    tp_trigger_px: None,
                    tp_ord_px: None,
                };

                match okx_client.place_order(&close_request).await {
                    Ok(resp) => {
                        if resp.s_code != "0" {
                            warn!("OKX close order rejected: {}", resp.s_msg);
                        } else {
                            info!(
                                "OKX close order placed for trade {}: order {}",
                                trade_id, resp.ord_id
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Failed to place OKX close order: {}", e);
                    }
                }
            }
        }

        let (pnl, pnl_percent) = Self::calculate_pnl(
            &trade.direction,
            trade.entry_price,
            exit_price,
            trade.quantity,
            trade.leverage,
        );

        let fee = (trade.entry_price * trade.quantity + exit_price * trade.quantity)
            * (trade.fee_percent / 100.0);
        let net_pnl = pnl - fee;
        let net_pnl_percent = (net_pnl / (trade.entry_price * trade.quantity)) * 100.0;

        let holding_duration = Utc::now().signed_duration_since(trade.opened_at);
        let holding_duration_minutes = holding_duration.num_minutes() as i32;

        let updated_trade = sqlx::query_as!(
            AiSimulationTrade,
            r#"
            UPDATE ai_simulation_trades
            SET exit_price = $1,
                pnl = $2,
                pnl_percent = $3,
                net_pnl_percent = $4,
                status = 'closed',
                close_reason = $5,
                holding_duration_minutes = $6,
                closed_at = NOW()
            WHERE id = $7
            RETURNING *
            "#,
            exit_price,
            pnl,
            pnl_percent,
            net_pnl_percent,
            close_reason,
            holding_duration_minutes,
            trade_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let config = sqlx::query_as!(
            AiSimulationConfig,
            "SELECT * FROM ai_simulation_configs WHERE id = $1",
            trade.config_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let new_balance = config.current_balance + net_pnl;
        let (winning_trades, losing_trades) = if pnl > 0.0 {
            (config.winning_trades + 1, config.losing_trades)
        } else {
            (config.winning_trades, config.losing_trades + 1)
        };

        let total_trades = config.total_trades + 1;
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        let updated_config = sqlx::query_as!(
            AiSimulationConfig,
            r#"
            UPDATE ai_simulation_configs
            SET current_balance = $1,
                winning_trades = $2,
                losing_trades = $3,
                win_rate = $4,
                updated_at = NOW()
            WHERE id = $5
            RETURNING *
            "#,
            new_balance,
            winning_trades,
            losing_trades,
            win_rate,
            config.id
        )
        .fetch_one(&mut *tx)
        .await?;

        self.update_rolling_stats(&mut *tx, config.id).await?;

        tx.commit().await?;

        info!(
            "Trade closed: {} with PnL {:.2} ({:.2}%)",
            trade_id, pnl, pnl_percent
        );

        Ok(TradeExecutionResult {
            trade: updated_trade,
            new_balance,
            pnl: net_pnl,
            pnl_percent: net_pnl_percent,
        })
    }

    pub async fn check_stop_loss_take_profit(
        &self,
        config_id: Uuid,
        market_snapshot: &MarketSnapshot,
    ) -> AgentResult<(Vec<StopLossTrigger>, Vec<TakeProfitTrigger>)> {
        let open_trades = sqlx::query_as!(
            AiSimulationTrade,
            "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'open'",
            config_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut stop_loss_triggers = Vec::new();
        let mut take_profit_triggers = Vec::new();

        for trade in open_trades {
            if let Some(sl) = trade.stop_loss {
                if Self::should_trigger_stop_loss(&trade.direction, sl, market_snapshot.current_price)
                {
                    stop_loss_triggers.push(StopLossTrigger {
                        trade_id: trade.id,
                        trigger_price: sl,
                        current_price: market_snapshot.current_price,
                        reason: "Stop loss triggered".to_string(),
                    });
                }
            }

            if let Some(tp) = trade.take_profit {
                if Self::should_trigger_take_profit(
                    &trade.direction,
                    tp,
                    market_snapshot.current_price,
                ) {
                    take_profit_triggers.push(TakeProfitTrigger {
                        trade_id: trade.id,
                        trigger_price: tp,
                        current_price: market_snapshot.current_price,
                        reason: "Take profit triggered".to_string(),
                    });
                }
            }
        }

        Ok((stop_loss_triggers, take_profit_triggers))
    }

    pub async fn process_market_update(
        &self,
        config_id: Uuid,
        market_snapshot: &MarketSnapshot,
    ) -> AgentResult<Vec<TradeExecutionResult>> {
        let (sl_triggers, tp_triggers) = self
            .check_stop_loss_take_profit(config_id, market_snapshot)
            .await?;

        let mut results = Vec::new();

        for trigger in sl_triggers {
            let result = self
                .close_trade(trigger.trade_id, trigger.current_price, "stop_loss")
                .await?;
            results.push(result);
        }

        for trigger in tp_triggers {
            let result = self
                .close_trade(trigger.trade_id, trigger.current_price, "take_profit")
                .await?;
            results.push(result);
        }

        Ok(results)
    }

    pub async fn get_open_trades(&self, config_id: Uuid) -> AgentResult<Vec<AiSimulationTrade>> {
        let trades = sqlx::query_as!(
            AiSimulationTrade,
            "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'open' ORDER BY opened_at DESC",
            config_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(trades)
    }

    pub async fn get_trade_history(
        &self,
        config_id: Uuid,
        limit: i64,
    ) -> AgentResult<Vec<AiSimulationTrade>> {
        let trades = sqlx::query_as!(
            AiSimulationTrade,
            "SELECT * FROM ai_simulation_trades WHERE config_id = $1 ORDER BY opened_at DESC LIMIT $2",
            config_id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(trades)
    }

    pub async fn calculate_position_size(
        &self,
        config: &AiSimulationConfig,
        entry_price: f64,
        stop_loss: f64,
    ) -> AgentResult<f64> {
        let risk_amount =
            config.current_balance * (config.max_single_trade_loss_percent / 100.0);
        let price_risk = (entry_price - stop_loss).abs();

        if price_risk == 0.0 {
            return Ok(0.0);
        }

        let quantity = risk_amount / price_risk;
        let position_value = quantity * entry_price;
        let max_position_value =
            config.current_balance * (config.max_position_size_percent / 100.0);

        Ok(if position_value > max_position_value {
            max_position_value / entry_price
        } else {
            quantity
        })
    }

    pub async fn get_okx_account_balance(&self) -> AgentResult<f64> {
        let okx_client = self.okx_client.as_ref().ok_or_else(|| {
            crate::agents::errors::AgentError::ExecutionError(
                "OKX client not configured".to_string(),
            )
        })?;

        let accounts = okx_client.get_account_balance().await.map_err(|e| {
            crate::agents::errors::AgentError::ExternalApiError(format!(
                "Failed to get OKX balance: {}",
                e
            ))
        })?;

        let total_eq: f64 = accounts
            .iter()
            .filter_map(|a| a.total_eq.parse::<f64>().ok())
            .sum();

        Ok(total_eq)
    }

    pub async fn get_okx_positions(&self, inst_type: Option<&str>) -> AgentResult<Vec<crate::exchanges::okx::OkxPosition>> {
        let okx_client = self.okx_client.as_ref().ok_or_else(|| {
            crate::agents::errors::AgentError::ExecutionError(
                "OKX client not configured".to_string(),
            )
        })?;

        okx_client.get_positions(inst_type).await.map_err(|e| {
            crate::agents::errors::AgentError::ExternalApiError(format!(
                "Failed to get OKX positions: {}",
                e
            ))
        })
    }

    pub async fn get_okx_ticker(&self, inst_id: &str) -> AgentResult<crate::exchanges::okx::OkxTicker> {
        let okx_client = self.okx_client.as_ref().ok_or_else(|| {
            crate::agents::errors::AgentError::ExecutionError(
                "OKX client not configured".to_string(),
            )
        })?;

        okx_client.get_ticker(inst_id).await.map_err(|e| {
            crate::agents::errors::AgentError::ExternalApiError(format!(
                "Failed to get OKX ticker: {}",
                e
            ))
        })
    }

    pub async fn sync_okx_positions_to_db(&self, config_id: Uuid) -> AgentResult<Vec<AiSimulationTrade>> {
        let okx_client = self.okx_client.as_ref().ok_or_else(|| {
            crate::agents::errors::AgentError::ExecutionError(
                "OKX client not configured".to_string(),
            )
        })?;

        let positions = okx_client.get_positions(None).await.map_err(|e| {
            crate::agents::errors::AgentError::ExternalApiError(format!(
                "Failed to get OKX positions: {}",
                e
            ))
        })?;

        let mut synced_trades = Vec::new();

        for pos in &positions {
            let direction = if pos.pos.parse::<f64>().unwrap_or(0.0) > 0.0 {
                "long"
            } else {
                "short"
            };

            let quantity = pos.pos.parse::<f64>().unwrap_or(0.0).abs();
            let entry_price = pos.avg_px.parse::<f64>().unwrap_or(0.0);
            let leverage = pos.lever.parse::<i32>().unwrap_or(1);

            let existing = sqlx::query!(
                r#"
                SELECT id FROM ai_simulation_trades
                WHERE config_id = $1 AND symbol = $2 AND direction = $3 AND status = 'open'
                "#,
                config_id,
                pos.inst_id,
                direction
            )
            .fetch_optional(&self.pool)
            .await?;

            if existing.is_none() && quantity > 0.0 {
                let trade = sqlx::query_as!(
                    AiSimulationTrade,
                    r#"
                    INSERT INTO ai_simulation_trades (
                        config_id, symbol, mode, direction, entry_price, quantity, leverage,
                        status, opened_at
                    )
                    VALUES ($1, $2, 'live', $3, $4, $5, $6, 'open', NOW())
                    RETURNING *
                    "#,
                    config_id,
                    pos.inst_id,
                    direction,
                    entry_price,
                    quantity,
                    leverage
                )
                .fetch_one(&self.pool)
                .await?;

                synced_trades.push(trade);
            }
        }

        if !synced_trades.is_empty() {
            info!("Synced {} OKX positions to database", synced_trades.len());
        }

        Ok(synced_trades)
    }

    fn calculate_pnl(
        direction: &str,
        entry_price: f64,
        exit_price: f64,
        quantity: f64,
        leverage: i32,
    ) -> (f64, f64) {
        let price_change = if direction == "long" {
            exit_price - entry_price
        } else {
            entry_price - exit_price
        };

        let pnl = price_change * quantity * leverage as f64;
        let pnl_percent = (price_change / entry_price) * 100.0 * leverage as f64;

        (pnl, pnl_percent)
    }

    fn should_trigger_stop_loss(direction: &str, stop_loss: f64, current_price: f64) -> bool {
        if direction == "long" {
            current_price <= stop_loss
        } else {
            current_price >= stop_loss
        }
    }

    fn should_trigger_take_profit(direction: &str, take_profit: f64, current_price: f64) -> bool {
        if direction == "long" {
            current_price >= take_profit
        } else {
            current_price <= take_profit
        }
    }

    async fn update_rolling_stats(&self, tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, config_id: Uuid) -> AgentResult<()> {
        let trades = sqlx::query_as!(
            AiSimulationTrade,
            r#"
            SELECT * FROM ai_simulation_trades
            WHERE config_id = $1 AND status = 'closed'
            ORDER BY opened_at DESC
            LIMIT 50
            "#,
            config_id
        )
        .fetch_all(&mut **tx)
        .await?;

        if trades.is_empty() {
            return Ok(());
        }

        let _total_pnl: f64 = trades.iter().filter_map(|t| t.pnl).sum();
        let winning_trades: Vec<_> = trades.iter().filter(|t| t.pnl.unwrap_or(0.0) > 0.0).collect();
        let losing_trades: Vec<_> = trades.iter().filter(|t| t.pnl.unwrap_or(0.0) < 0.0).collect();

        let avg_pnl_percent = trades.iter().filter_map(|t| t.pnl_percent).sum::<f64>() / trades.len() as f64;
        let win_rate = winning_trades.len() as f64 / trades.len() as f64;

        let avg_win = if !winning_trades.is_empty() {
            winning_trades.iter().filter_map(|t| t.pnl).sum::<f64>() / winning_trades.len() as f64
        } else {
            0.0
        };

        let avg_loss = if !losing_trades.is_empty() {
            losing_trades.iter().filter_map(|t| t.pnl).map(|p| p.abs()).sum::<f64>() / losing_trades.len() as f64
        } else {
            0.0
        };

        let profit_loss_ratio = if avg_loss != 0.0 {
            avg_win / avg_loss
        } else {
            0.0
        };

        sqlx::query!(
            r#"
            UPDATE ai_simulation_configs
            SET avg_pnl_percent = $1,
                profit_loss_ratio = $2,
                updated_at = NOW()
            WHERE id = $3
            "#,
            avg_pnl_percent,
            profit_loss_ratio,
            config_id
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_pnl_long() {
        let (pnl, pnl_percent) = SimulationEngine::calculate_pnl("long", 100.0, 110.0, 1.0, 1);
        assert_eq!(pnl, 10.0);
        assert_eq!(pnl_percent, 10.0);
    }

    #[test]
    fn test_calculate_pnl_short() {
        let (pnl, pnl_percent) = SimulationEngine::calculate_pnl("short", 100.0, 90.0, 1.0, 1);
        assert_eq!(pnl, 10.0);
        assert_eq!(pnl_percent, 10.0);
    }

    #[test]
    fn test_should_trigger_stop_loss_long() {
        assert!(SimulationEngine::should_trigger_stop_loss("long", 95.0, 94.0));
        assert!(!SimulationEngine::should_trigger_stop_loss("long", 95.0, 96.0));
    }

    #[test]
    fn test_should_trigger_stop_loss_short() {
        assert!(SimulationEngine::should_trigger_stop_loss("short", 105.0, 106.0));
        assert!(!SimulationEngine::should_trigger_stop_loss("short", 105.0, 104.0));
    }

    #[test]
    fn test_should_trigger_take_profit_long() {
        assert!(SimulationEngine::should_trigger_take_profit("long", 110.0, 111.0));
        assert!(!SimulationEngine::should_trigger_take_profit("long", 110.0, 109.0));
    }

    #[test]
    fn test_should_trigger_take_profit_short() {
        assert!(SimulationEngine::should_trigger_take_profit("short", 90.0, 89.0));
        assert!(!SimulationEngine::should_trigger_take_profit("short", 90.0, 91.0));
    }

    #[test]
    fn test_parse_execution_mode() {
        assert_eq!(SimulationEngine::parse_execution_mode("paper"), ExecutionMode::Paper);
        assert_eq!(SimulationEngine::parse_execution_mode("demo"), ExecutionMode::Demo);
        assert_eq!(SimulationEngine::parse_execution_mode("live"), ExecutionMode::Live);
        assert_eq!(SimulationEngine::parse_execution_mode("Paper"), ExecutionMode::Paper);
        assert_eq!(SimulationEngine::parse_execution_mode("unknown"), ExecutionMode::Paper);
    }

    #[test]
    fn test_execution_mode_to_string() {
        assert_eq!(SimulationEngine::execution_mode_to_string(&ExecutionMode::Paper), "paper");
        assert_eq!(SimulationEngine::execution_mode_to_string(&ExecutionMode::Demo), "demo");
        assert_eq!(SimulationEngine::execution_mode_to_string(&ExecutionMode::Live), "live");
    }
}
