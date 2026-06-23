//! Feature Store & Market Regime Detection Module
//! 特征存储与市场状态识别模块
//!
//! 依据《系统评估与演进规划》第二阶段"量化基础"：
//! - 建立长期历史数据和特征仓库
//! - 建立市场状态模型及概率校准
//! - 补齐数据血缘与数据质量字段（018 迁移）
//!
//! 模块结构：
//! - `regime`: 市场状态识别（5 类状态分类器）
//! - `store`: 特征存储（读写特征值到 PostgreSQL，含血缘与质量）
//! - `calculator`: 特征计算器（从 K 线计算并存储特征）

pub mod regime;
pub mod store;
pub mod calculator;

pub use regime::{MarketRegime, RegimeClassifier, RegimeConfig, RegimeSnapshot};
pub use store::{
    DataQualitySnapshot, FeatureDefinition, FeatureLineage, FeatureStore, FeatureValue, QualityGrade,
};
pub use calculator::{FeatureCalculator, FeatureSet};
