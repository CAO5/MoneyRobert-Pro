//! System Orchestration Layer
//! 系统编排层 — 统一决策流水线和账本服务
//!
//! 依据《系统 2.0 待补全问题清单》P1-4：
//! - DecisionOrchestrator: 市场→特征→信号→校准→EV/CVaR→仓位→风险→建议
//! - LedgerService: 统一开仓/加仓/减仓/平仓/反手，对接回测/模拟盘/实盘

pub mod decision_orchestrator;
pub mod ledger_service;

pub use decision_orchestrator::{DecisionOrchestrator, OrchestratorConfig, OrchestratorResult};
pub use ledger_service::{LedgerService, LedgerEntry, LedgerError};
