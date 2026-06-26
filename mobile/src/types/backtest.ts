/**
 * 回测相关类型
 * 对接后端 /backtest/* 接口
 */

/** 回测任务摘要（列表项） */
export interface BacktestJobSummary {
  job_id: string;
  job_name: string;
  strategy_id?: string;
  status: string; // pending / running / completed / failed
  progress: number; // 0-100
  start_time: string;
  end_time: string;
  initial_equity: number;
  total_trades?: number;
  total_return_pct?: number;
  sharpe_ratio?: number;
  max_drawdown_pct?: number;
  created_at: string;
}

/** 回测任务详情 */
export interface BacktestJobDetail extends BacktestJobSummary {
  winning_trades?: number;
  fee_total?: number;
  slippage_total?: number;
  completed_at?: string;
  mode: string;
  data_frequency: string;
  fee_taker_bps: number;
  fee_maker_bps: number;
  slippage_bps: number;
  max_single_position_pct: number;
  max_total_leverage: number;
  max_daily_loss_pct: number;
  assets: string[];
}

/** 回测绩效报告 */
export interface BacktestReport {
  report_id: string;
  total_return?: number;
  annualized_return?: number;
  max_drawdown?: number;
  sharpe_ratio?: number;
  win_rate?: number;
  profit_factor?: number;
  total_trades?: number;
  winning_trades?: number;
  losing_trades?: number;
  average_win?: number;
  average_loss?: number;
  payoff_ratio?: number;
  total_fee?: number;
}

/** 回测可信等级 */
export interface TrustLevelResponse {
  assessment_id: string;
  job_id: string;
  /** 可信等级：display_only / comparable / promotion_eligible */
  trust_level: string;
  test_coverage_passed: boolean;
  capital_conservation_passed: boolean;
  slippage_accounted: boolean;
  data_quality_grade: string;
  sample_size_sufficient: boolean;
  walk_forward_validated: boolean;
  calibration_healthy: boolean;
  total_trades: number;
  test_pass_rate: number;
  data_coverage_ratio: number;
  issues: unknown;
  recommendations: unknown;
  promotion_eligible: boolean;
  promotion_blockers: unknown;
  assessed_at: string;
}

/** 任务状态中文映射 */
export const BACKTEST_STATUS_LABELS: Record<string, string> = {
  pending: '待启动',
  running: '运行中',
  completed: '已完成',
  failed: '失败',
  cancelled: '已取消',
};

/** 可信等级中文与颜色映射 */
export const TRUST_LEVEL_LABELS: Record<string, { label: string; color: string }> = {
  display_only: { label: 'D 不可信', color: '#f53f3f' },
  comparable: { label: 'C 可比较', color: '#ff7d00' },
  promotion_eligible: { label: 'A 高可信', color: '#00b42a' },
};
