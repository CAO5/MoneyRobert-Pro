//! Trading Simulation & Backtest Module
//! 交易模拟与历史回测模块

pub mod models;
pub mod matching_engine;
pub mod account_engine;
pub mod performance_engine;
pub mod replay_engine;
pub mod risk_engine;
pub mod runner;
pub mod trust_engine;
pub mod walk_forward;
pub mod purged_kfold;
pub mod benchmark;
pub mod position_sizing;
pub mod portfolio_risk;
pub mod execution;
pub mod attribution;
pub mod strategy_failure;
