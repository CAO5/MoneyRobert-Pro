/**
 * Agent 相关类型
 * 对接后端 /agent/* 接口
 */

/** Agent 状态 */
export type AgentStatus = 'idle' | 'training' | 'running' | 'paused' | 'stopped' | 'error';

/** Agent 信息 */
export interface AgentInfo {
  agent_id: string;
  name: string;
  status: AgentStatus;
  version?: string;
  accuracy?: number;
  total_trades?: number;
  winning_trades?: number;
  losing_trades?: number;
  created_at: string;
  updated_at?: string;
}

/** Agent 训练状态 */
export interface AgentTrainingStatus {
  agent_id: string;
  status: AgentStatus;
  progress: number;
  current_epoch?: number;
  total_epochs?: number;
  loss?: number;
  accuracy?: number;
  started_at: string;
  estimated_completion?: string;
}

/** Agent 准确率统计 */
export interface AgentAccuracy {
  agent_id: string;
  overall_accuracy: number;
  recent_accuracy: number;
  total_predictions: number;
  correct_predictions: number;
  by_symbol?: Array<{
    symbol: string;
    accuracy: number;
    count: number;
  }>;
  by_horizon?: Array<{
    horizon: string;
    accuracy: number;
    count: number;
  }>;
}

/** Agent 权限配置 */
export interface AgentPermission {
  agent_id: string;
  can_trade: boolean;
  can_modify_strategy: boolean;
  max_position_size: number;
  max_leverage: number;
  allowed_symbols: string[];
  restricted_actions: string[];
}

/** Agent 日志条目 */
export interface AgentLogEntry {
  log_id: string;
  agent_id: string;
  level: 'info' | 'warning' | 'error' | 'debug';
  message: string;
  context?: Record<string, unknown>;
  timestamp: string;
}

/** Agent 仪表盘汇总 */
export interface AgentDashboard {
  total_agents: number;
  active_agents: number;
  training_agents: number;
  total_accuracy: number;
  total_trades: number;
  agents: AgentInfo[];
}

/** Agent 性能数据 */
export interface AgentPerformance {
  agent_id: string;
  total_return: number;
  sharpe_ratio: number;
  max_drawdown: number;
  win_rate: number;
  profit_factor: number;
  total_trades: number;
  avg_trade_duration?: number;
}

/** 模拟状态 */
export interface SimulationStatus {
  simulation_id: string;
  status: string;
  started_at: string;
  trades_count: number;
  current_equity: number;
  initial_equity: number;
  return_pct: number;
}
