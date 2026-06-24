-- =============================================
-- Extended Technical Indicators (Phase 2 Enhancement)
-- 扩展技术指标定义
--
-- 依据：系统评估与演进规划 基金经理视角建议扩充
--   | 类别 | 指标 |
--   |------|------|
--   | 趋势 | ADX、Donchian、SuperTrend、时间序列动量 |
--   | 量价 | VWAP、Anchored VWAP、OBV、MFI、CVD |
--   | 波动率 | 实现波动率、Parkinson/Garman-Klass、GARCH |
--   | 微观结构 | spread、depth imbalance、Kyle Lambda、价格冲击 |
--
-- 本迁移新增 7 个特征定义：
--   1. donchian_width    - Donchian 通道宽度
--   2. supertrend        - SuperTrend 趋势指标
--   3. vwap              - VWAP 偏离度
--   4. obv               - 能量潮
--   5. mfi_14            - 资金流量指标
--   6. garch_volatility  - GARCH(1,1) 波动率
--   7. kyle_lambda       - 价格冲击系数
-- =============================================

INSERT INTO feature_definitions (name, description, category, version, parameters, unit) VALUES
    ('donchian_width', '20 周期 Donchian 通道宽度（最高价-最低价）/收盘', 'volatility', '1.0', '{"period": 20}'::jsonb, 'percent'),
    ('supertrend', 'SuperTrend 趋势跟踪指标（ATR 10, multiplier 3.0）', 'momentum', '1.0', '{"period": 10, "multiplier": 3.0}'::jsonb, 'price'),
    ('vwap', '20 周期 VWAP 偏离度', 'volume', '1.0', '{"period": 20}'::jsonb, 'percent'),
    ('obv', '能量潮（On-Balance Volume）变化率', 'volume', '1.0', '{"period": 20}'::jsonb, 'ratio'),
    ('mfi_14', '14 周期资金流量指标', 'momentum', '1.0', '{"period": 14}'::jsonb, 'ratio'),
    ('garch_volatility', 'GARCH(1,1) 年化波动率', 'volatility', '1.0', '{"period": 20, "omega": 0.00001, "alpha": 0.10, "beta": 0.88}'::jsonb, 'percent'),
    ('kyle_lambda', 'Kyle Lambda 价格冲击系数', 'microstructure', '1.0', '{"period": 20}'::jsonb, 'ratio')
ON CONFLICT (name) DO NOTHING;

COMMENT ON TABLE feature_definitions IS '特征定义表，包含技术指标、量价指标、波动率指标、微观结构指标';
