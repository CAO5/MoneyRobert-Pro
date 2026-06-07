use crate::agents::{
    errors::AgentResult,
    models::{AiSimulationConfig, AiSimulationTrade, ExecutionMode, MarketSnapshot},
};
use crate::exchanges::okx::OkxClient;
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
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

        let trade = sqlx::query_as::<_, AiSimulationTrade>(
            r#"
            INSERT INTO ai_simulation_trades (
                config_id, symbol, mode, direction, entry_price, quantity, leverage,
                stop_loss, take_profit, ai_confidence, ai_reasoning, agent_session_id,
                status, opened_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'open', NOW())
            RETURNING *
            "#
        )
        .bind(config.id)
        .bind(&config.symbol)
        .bind(&config.mode)
        .bind(direction)
        .bind(entry_price)
        .bind(quantity)
        .bind(leverage)
        .bind(stop_loss)
        .bind(take_profit)
        .bind(ai_confidence)
        .bind(ai_reasoning)
        .bind(agent_session_id)
        .fetch_one(&mut *tx)
        .await?;

        let updated_config = sqlx::query_as::<_, AiSimulationConfig>(
            r#"
            UPDATE ai_simulation_configs
            SET total_trades = total_trades + 1,
                last_trade_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#
        )
        .bind(config.id)
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

        let trade = sqlx::query_as::<_, AiSimulationTrade>(
            r#"
            INSERT INTO ai_simulation_trades (
                config_id, symbol, mode, direction, entry_price, quantity, leverage,
                stop_loss, take_profit, ai_confidence, ai_reasoning, agent_session_id,
                status, opened_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'open', NOW())
            RETURNING *
            "#
        )
        .bind(config.id)
        .bind(&config.symbol)
        .bind(mode_str)
        .bind(direction)
        .bind(entry_price)
        .bind(quantity)
        .bind(leverage)
        .bind(stop_loss)
        .bind(take_profit)
        .bind(ai_confidence)
        .bind(ai_reasoning)
        .bind(agent_session_id)
        .fetch_one(&mut *tx)
        .await?;

        let updated_config = sqlx::query_as::<_, AiSimulationConfig>(
            r#"
            UPDATE ai_simulation_configs
            SET total_trades = total_trades + 1,
                last_trade_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#
        )
        .bind(config.id)
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

        let trade = sqlx::query_as::<_, AiSimulationTrade>(
            "SELECT * FROM ai_simulation_trades WHERE id = $1"
        )
        .bind(trade_id)
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

        let updated_trade = sqlx::query_as::<_, AiSimulationTrade>(
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
            "#
        )
        .bind(exit_price)
        .bind(pnl)
        .bind(pnl_percent)
        .bind(net_pnl_percent)
        .bind(close_reason)
        .bind(holding_duration_minutes)
        .bind(trade_id)
        .fetch_one(&mut *tx)
        .await?;

        let config = sqlx::query_as::<_, AiSimulationConfig>(
            "SELECT * FROM ai_simulation_configs WHERE id = $1"
        )
        .bind(trade.config_id)
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

        let updated_config = sqlx::query_as::<_, AiSimulationConfig>(
            r#"
            UPDATE ai_simulation_configs
            SET current_balance = $1,
                winning_trades = $2,
                losing_trades = $3,
                win_rate = $4,
                updated_at = NOW()
            WHERE id = $5
            RETURNING *
            "#
        )
        .bind(new_balance)
        .bind(winning_trades)
        .bind(losing_trades)
        .bind(win_rate)
        .bind(config.id)
        .fetch_one(&mut *tx)
        .await?;

        self.update_rolling_stats(&mut *tx, config.id).await?;

        // === Learning Feedback: Update agent_performance and decision_memory ===
        if let Err(e) = Self::record_trade_outcome(
            &mut *tx,
            &updated_trade,
            &config,
            net_pnl > 0.0,
        ).await {
            warn!("Failed to record trade outcome for learning: {}", e);
        }

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
        let open_trades = sqlx::query_as::<_, AiSimulationTrade>(
            "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'open'"
        )
        .bind(config_id)
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
        let trades = sqlx::query_as::<_, AiSimulationTrade>(
            "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'open' ORDER BY opened_at DESC"
        )
        .bind(config_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(trades)
    }

    pub async fn get_trade_history(
        &self,
        config_id: Uuid,
        limit: i64,
    ) -> AgentResult<Vec<AiSimulationTrade>> {
        let trades = sqlx::query_as::<_, AiSimulationTrade>(
            "SELECT * FROM ai_simulation_trades WHERE config_id = $1 ORDER BY opened_at DESC LIMIT $2"
        )
        .bind(config_id)
        .bind(limit)
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
            .filter_map(|a| a.total_eq.as_deref().and_then(|v| v.parse::<f64>().ok()))
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
            let direction = if pos.pos.as_deref().unwrap_or("0").parse::<f64>().unwrap_or(0.0) > 0.0 {
                "long"
            } else {
                "short"
            };

            let quantity = pos.pos.as_deref().unwrap_or("0").parse::<f64>().unwrap_or(0.0).abs();
            let entry_price = pos.avg_px.as_deref().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
            let leverage = pos.lever.as_deref().unwrap_or("1").parse::<i32>().unwrap_or(1);

            let existing = sqlx::query(
                r#"
                SELECT id FROM ai_simulation_trades
                WHERE config_id = $1 AND symbol = $2 AND direction = $3 AND status = 'open'
                "#
            )
            .bind(config_id)
            .bind(pos.inst_id.as_deref().unwrap_or(""))
            .bind(direction)
            .fetch_optional(&self.pool)
            .await?;

            if existing.is_none() && quantity > 0.0 {
                let trade = sqlx::query_as::<_, AiSimulationTrade>(
                    r#"
                    INSERT INTO ai_simulation_trades (
                        config_id, symbol, mode, direction, entry_price, quantity, leverage,
                        status, opened_at
                    )
                    VALUES ($1, $2, 'live', $3, $4, $5, $6, 'open', NOW())
                    RETURNING *
                    "#
                )
                .bind(config_id)
                .bind(pos.inst_id.as_deref().unwrap_or(""))
                .bind(direction)
                .bind(entry_price)
                .bind(quantity)
                .bind(leverage)
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

    async fn update_rolling_stats(&self, tx: &mut sqlx::PgConnection, config_id: Uuid) -> AgentResult<()> {
        let trades = sqlx::query_as::<_, AiSimulationTrade>(
            r#"
            SELECT * FROM ai_simulation_trades
            WHERE config_id = $1 AND status = 'closed'
            ORDER BY opened_at DESC
            LIMIT 50
            "#
        )
        .bind(config_id)
        .fetch_all(&mut *tx)
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

        sqlx::query(
            r#"
            UPDATE ai_simulation_configs
            SET avg_pnl_percent = $1,
                profit_loss_ratio = $2,
                updated_at = NOW()
            WHERE id = $3
            "#
        )
        .bind(avg_pnl_percent)
        .bind(profit_loss_ratio)
        .bind(config_id)
        .execute(&mut *tx)
        .await?;

        Ok(())
    }

    /// Record trade outcome for agent learning: update agent_performance + decision_memory
    async fn record_trade_outcome(
        tx: &mut sqlx::PgConnection,
        trade: &AiSimulationTrade,
        config: &AiSimulationConfig,
        is_win: bool,
    ) -> AgentResult<()> {
        // 1. Update decision_memory with actual outcome and market context
        if let Some(reasoning_val) = &trade.ai_reasoning {
            let debate_session_id = reasoning_val.get("debate_session_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<Uuid>().ok());

            let agent_opinions = reasoning_val.get("agent_opinions")
                .cloned()
                .unwrap_or(json!([]));
            let department_reports = reasoning_val.get("department_reports")
                .cloned()
                .unwrap_or(json!([]));
            let reasoning = reasoning_val.get("reasoning")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Extract market context from reasoning
            let market_trend = reasoning_val.get("market_context")
                .and_then(|v| v.get("trend"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let volatility = reasoning_val.get("market_context")
                .and_then(|v| v.get("volatility"))
                .and_then(|v| v.as_str())
                .unwrap_or("medium");
            let volume_profile = reasoning_val.get("market_context")
                .and_then(|v| v.get("volume_profile"))
                .and_then(|v| v.as_str())
                .unwrap_or("stable");

            // Extract multi-timeframe data
            let mtf_alignment = reasoning_val.get("multi_timeframe")
                .and_then(|v| v.get("alignment"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5);

            // Calculate entry timing score based on proximity to key levels
            let entry_timing_score = if let Some(key_levels) = reasoning_val.get("market_context")
                .and_then(|v| v.get("key_levels"))
                .and_then(|v| v.as_array()) {
                let entry = trade.entry_price;
                let mut min_dist = f64::MAX;
                for level in key_levels {
                    if let Some(price) = level.as_f64() {
                        let dist = ((entry - price) / entry).abs();
                        if dist < min_dist { min_dist = dist; }
                    }
                }
                (1.0 - min_dist.min(0.05) * 20.0).max(0.0)
            } else { 0.5 };

            // Calculate leverage fit based on volatility
            let leverage_fit = match volatility {
                "low" => trade.leverage <= 5,
                "medium" => trade.leverage <= 3,
                "high" => trade.leverage <= 2,
                _ => trade.leverage <= 3,
            };

            // Calculate position quality score
            let risk_reward_ratio = if let (Some(sl), Some(tp)) = (trade.stop_loss, trade.take_profit) {
                let risk = ((trade.entry_price - sl) / trade.entry_price).abs();
                let reward = ((tp - trade.entry_price) / trade.entry_price).abs();
                if risk > 0.0 { reward / risk } else { 1.0 }
            } else { 1.0 };
            let rr_score = (risk_reward_ratio / 2.0).min(1.0);
            let direction_score = if is_win { 1.0 } else { 0.0 };
            let position_quality_score = direction_score * 0.40 +
                                        entry_timing_score * 0.15 +
                                        rr_score * 0.20 +
                                        1.0 * 0.10 + // duration fit
                                        if leverage_fit { 1.0 } else { 0.5 } * 0.05 +
                                        mtf_alignment * 0.10;

            sqlx::query(
                r#"
                INSERT INTO decision_memory (
                    user_id, config_id, trade_id, debate_session_id,
                    symbol, action, confidence, leverage, stop_loss, take_profit,
                    agent_opinions, department_reports, reasoning,
                    actual_outcome, actual_pnl, actual_pnl_percent, success,
                    holding_duration_minutes, close_reason, created_at, updated_at,
                    market_trend, volatility, volume_profile,
                    entry_timing_score, leverage_fit, multi_timeframe_alignment,
                    position_quality_score
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, NOW(), NOW(),
                        $20, $21, $22, $23, $24, $25, $26)
                ON CONFLICT (trade_id) DO UPDATE SET
                    actual_outcome = EXCLUDED.actual_outcome,
                    actual_pnl = EXCLUDED.actual_pnl,
                    actual_pnl_percent = EXCLUDED.actual_pnl_percent,
                    success = EXCLUDED.success,
                    holding_duration_minutes = EXCLUDED.holding_duration_minutes,
                    close_reason = EXCLUDED.close_reason,
                    market_trend = EXCLUDED.market_trend,
                    volatility = EXCLUDED.volatility,
                    volume_profile = EXCLUDED.volume_profile,
                    entry_timing_score = EXCLUDED.entry_timing_score,
                    leverage_fit = EXCLUDED.leverage_fit,
                    multi_timeframe_alignment = EXCLUDED.multi_timeframe_alignment,
                    position_quality_score = EXCLUDED.position_quality_score,
                    updated_at = NOW()
                "#
            )
            .bind(config.user_id)
            .bind(config.id)
            .bind(trade.id)
            .bind(debate_session_id)
            .bind(&trade.symbol)
            .bind(&trade.direction)
            .bind(trade.ai_confidence.unwrap_or(0.5))
            .bind(trade.leverage)
            .bind(trade.stop_loss)
            .bind(trade.take_profit)
            .bind(&agent_opinions)
            .bind(&department_reports)
            .bind(reasoning)
            .bind(if is_win { "profit" } else { "loss" })
            .bind(trade.pnl)
            .bind(trade.pnl_percent)
            .bind(is_win)
            .bind(trade.holding_duration_minutes)
            .bind(&trade.close_reason)
            .bind(market_trend)
            .bind(volatility)
            .bind(volume_profile)
            .bind(entry_timing_score)
            .bind(leverage_fit)
            .bind(mtf_alignment)
            .bind(position_quality_score)
            .execute(&mut *tx)
            .await?;
        }

        // 2. Update agent_performance for each agent that contributed to this trade
        if let Some(reasoning_val) = &trade.ai_reasoning {
            if let Some(opinions) = reasoning_val.get("agent_opinions").and_then(|v| v.as_array()) {
                let trade_direction = &trade.direction;

                for opinion in opinions {
                    let agent_name = opinion.get("agent_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let department = opinion.get("department")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let agent_sentiment = opinion.get("sentiment")
                        .and_then(|v| v.as_str())
                        .unwrap_or("neutral");

                    // Determine if this agent's prediction was correct
                    let predicted_correct = match (agent_sentiment, trade_direction.as_str()) {
                        ("bullish", "long") => true,
                        ("bearish", "short") => true,
                        ("bearish", "long") => false,
                        ("bullish", "short") => false,
                        _ => false, // neutral is neither correct nor incorrect
                    };

                    // Upsert agent_performance
                    let existing = sqlx::query_as::<_, (i32, i32, f64, f64, f64, f64, f64, f64, f64, f64)>(
                        r#"SELECT total_analyses, correct_predictions, accuracy, credibility_score, calibration_factor,
                                  trend_accuracy, volatility_accuracy, volume_accuracy, timing_accuracy, weighted_accuracy
                           FROM agent_performance WHERE agent_name = $1 AND agent_department = $2"#
                    )
                    .bind(agent_name)
                    .bind(department)
                    .fetch_optional(&mut *tx)
                    .await?;

                    match existing {
                        Some((total, correct, _accuracy, credibility, calibration,
                              trend_acc, vol_acc, vol_profile_acc, timing_acc, weighted_acc)) => {
                            let new_total = total + 1;
                            let new_correct = correct + if predicted_correct { 1 } else { 0 };
                            let new_accuracy = new_correct as f64 / new_total as f64;

                            // Extract market context from reasoning
                            let market_trend = reasoning_val.get("market_context")
                                .and_then(|v| v.get("trend"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let volatility = reasoning_val.get("market_context")
                                .and_then(|v| v.get("volatility"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("medium");
                            let volume_profile = reasoning_val.get("market_context")
                                .and_then(|v| v.get("volume_profile"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("stable");
                            let mtf_alignment = reasoning_val.get("multi_timeframe")
                                .and_then(|v| v.get("alignment"))
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.5);

                            // Calculate context-specific accuracy with decay weighting
                            let decay_rate: f64 = 0.95;
                            let weighted_correct: f64 = if predicted_correct { 1.0 } else { 0.0 };

                            // Update trend accuracy
                            let new_trend_acc = if market_trend != "unknown" {
                                let trend_weight = decay_rate.powi((total - 1) as i32);
                                (trend_acc * (1.0 - trend_weight) + if predicted_correct { 1.0 } else { 0.0 } * trend_weight)
                            } else {
                                trend_acc
                            };

                            // Update volatility accuracy
                            let new_vol_acc = if volatility != "medium" {
                                let vol_weight = decay_rate.powi((total - 1) as i32);
                                (vol_acc * (1.0 - vol_weight) + if predicted_correct { 1.0 } else { 0.0 } * vol_weight)
                            } else {
                                vol_acc
                            };

                            // Update volume accuracy
                            let new_vol_profile_acc = if volume_profile != "stable" {
                                let vol_profile_weight = decay_rate.powi((total - 1) as i32);
                                (vol_profile_acc * (1.0 - vol_profile_weight) + if predicted_correct { 1.0 } else { 0.0 } * vol_profile_weight)
                            } else {
                                vol_profile_acc
                            };

                            // Update timing accuracy based on multi-timeframe alignment
                            let new_timing_acc = if mtf_alignment > 0.7 {
                                let timing_weight = decay_rate.powi((total - 1) as i32);
                                (timing_acc * (1.0 - timing_weight) + if predicted_correct { 1.0 } else { 0.0 } * timing_weight)
                            } else {
                                timing_acc
                            };

                            // Calculate weighted accuracy with decay
                            let new_weighted_acc = if total > 0 {
                                (weighted_acc * (total - 1) as f64 * decay_rate + weighted_correct) / (total as f64)
                            } else {
                                weighted_correct
                            };

                            // Smoothed credibility with context awareness
                            // 70% performance + 15% trend accuracy + 15% weighted accuracy
                            let new_credibility = (new_accuracy * 0.7) + (new_trend_acc * 0.15) + (new_weighted_acc * 0.15);

                            // Calibration: how well confidence matches reality
                            let agent_confidence = opinion.get("confidence")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.5);
                            let new_calibration = if new_total > 5 {
                                (calibration * (new_total - 1) as f64 + (1.0 - (agent_confidence - new_accuracy).abs())) / new_total as f64
                            } else {
                                calibration
                            };

                            sqlx::query(
                                r#"UPDATE agent_performance SET
                                total_analyses = $1, correct_predictions = $2, accuracy = $3,
                                credibility_score = $4, calibration_factor = $5,
                                trend_accuracy = $8, volatility_accuracy = $9, volume_accuracy = $10,
                                timing_accuracy = $11, weighted_accuracy = $12,
                                last_analysis_at = NOW(), updated_at = NOW()
                                WHERE agent_name = $6 AND agent_department = $7"#
                            )
                            .bind(new_total)
                            .bind(new_correct)
                            .bind(new_accuracy)
                            .bind(new_credibility)
                            .bind(new_calibration)
                            .bind(agent_name)
                            .bind(department)
                            .bind(new_trend_acc)
                            .bind(new_vol_acc)
                            .bind(new_vol_profile_acc)
                            .bind(new_timing_acc)
                            .bind(new_weighted_acc)
                            .execute(&mut *tx)
                            .await?;
                        }
                        None => {
                            let credibility = if predicted_correct { 0.6 } else { 0.4 };
                            let trend_acc = if predicted_correct { 0.6 } else { 0.4 };
                            let vol_acc = if predicted_correct { 0.6 } else { 0.4 };
                            let vol_profile_acc = if predicted_correct { 0.6 } else { 0.4 };
                            let timing_acc = if predicted_correct { 0.6 } else { 0.4 };
                            let weighted_acc = if predicted_correct { 1.0 } else { 0.0 };

                            sqlx::query(
                                r#"INSERT INTO agent_performance (
                                    agent_name, agent_department, total_analyses, correct_predictions,
                                    accuracy, credibility_score, calibration_factor, last_analysis_at, created_at, updated_at,
                                    trend_accuracy, volatility_accuracy, volume_accuracy, timing_accuracy, weighted_accuracy,
                                    total_predictions, prediction_decay_rate
                                ) VALUES ($1, $2, 1, $3, $4, $5, 1.0, NOW(), NOW(), NOW(),
                                          $6, $7, $8, $9, $10, 1, 0.95)"#
                            )
                            .bind(agent_name)
                            .bind(department)
                            .bind(if predicted_correct { 1 } else { 0 })
                            .bind(if predicted_correct { 1.0 } else { 0.0 })
                            .bind(credibility)
                            .bind(trend_acc)
                            .bind(vol_acc)
                            .bind(vol_profile_acc)
                            .bind(timing_acc)
                            .bind(weighted_acc)
                            .execute(&mut *tx)
                            .await?;
                        }
                    }
                }
            }
        }

        info!(
            "Learning feedback recorded for trade {}: win={}, direction={}",
            trade.id, is_win, trade.direction
        );

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
