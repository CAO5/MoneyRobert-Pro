/**
 * 决策卡相关类型
 * 对接后端 /signals/decision-card、/signals/decision-cards 接口
 */

/** 决策卡建议动作 */
export type DecisionAction =
  | 'open_long'
  | 'open_short'
  | 'close'
  | 'hold'
  | 'reduce';

/** 决策卡响应 */
export interface DecisionCard {
  card_id: string;
  symbol: string;
  generated_at: string;
  suggested_action: DecisionAction;
  target_horizon_sec: number;
  // 概率分布
  p_up: number;
  p_down: number;
  p_flat: number;
  // 收益分位数
  q10?: number;
  q50?: number;
  q90?: number;
  // 净期望 EV
  expected_value: number;
  // 最坏情形 CVaR
  worst_case?: number;
  // 仓位建议（0-1）
  position_suggestion: number;
  // 已用风险预算
  risk_budget_used?: number;
  // 适用市场状态
  applicable_regime?: string;
  // 数据新鲜度（秒）
  data_freshness_sec?: number;
  // 失效条件
  invalidation_conditions?: Record<string, unknown>;
  // 模型版本
  model_version: string;
}

/** 决策卡列表项（列表场景裁剪字段，提升性能） */
export interface DecisionCardListItem {
  card_id: string;
  symbol: string;
  generated_at: string;
  suggested_action: DecisionAction;
  expected_value: number;
  p_up: number;
  p_down: number;
  p_flat: number;
  trust_level?: string; // A/B/C/D
}

/** 决策建议动作的中英文映射 */
export const DECISION_ACTION_LABELS: Record<DecisionAction, string> = {
  open_long: '做多',
  open_short: '做空',
  close: '平仓',
  hold: '观望',
  reduce: '减仓',
};
