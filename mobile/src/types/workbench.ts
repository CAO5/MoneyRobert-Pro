/**
 * 工作台聚合数据类型
 * 工作台首页一次性返回所有高频信息，避免首屏并发多个接口
 * 对应深度研究报告建议的 Mobile BFF 聚合接口
 */

/** 关键指标卡 */
export interface MetricCard {
  key: string;
  label: string;
  value: string | number;
  unit?: string;
  trend?: 'up' | 'down' | 'flat';
  change_percent?: number;
}

/** 风险提醒 */
export interface RiskAlert {
  id: string;
  level: 'info' | 'warning' | 'critical';
  title: string;
  description: string;
  symbol?: string;
  created_at: string;
}

/** 今日最值得处理的一条交易洞察 */
export interface DailyInsight {
  id: string;
  symbol: string;
  symbol_name: string;
  action: '关注' | '观察' | '减仓' | '止盈' | '规避';
  title: string;
  reason: string;
  confidence: number;
  expected_move: string;
  expires_at: string;
  route: string;
  params?: Record<string, string>;
}

/** 自选行情快照 */
export interface WatchlistQuote {
  symbol: string;
  display_name: string;
  price: string;
  change_percent: number;
  signal: string;
}

/** 产品已为用户创造的可感知价值 */
export interface ValueProof {
  risks_avoided: number;
  opportunities_found: number;
  estimated_value: string;
  analysis_count: number;
  free_analysis_limit: number;
}

/** 快捷入口 */
export interface QuickEntry {
  key: string;
  label: string;
  // 跳转路径或自定义动作
  route?: string;
  badge?: number; // 角标数字
}

/** 最近访问 */
export interface RecentItem {
  id: string;
  type: 'symbol' | 'decision' | 'backtest' | 'report';
  title: string;
  subtitle?: string;
  visited_at: string;
  // 跳转参数
  route: string;
  params?: Record<string, string>;
}

/** 工作台聚合响应 */
export interface WorkbenchData {
  greeting: string; // 问候语
  todo_count: number; // 待办总数
  risk_alert_count: number; // 风险告警数
  unread_message_count: number; // 未读消息数
  metrics: MetricCard[];
  risk_alerts: RiskAlert[];
  quick_entries: QuickEntry[];
  recent_items: RecentItem[];
  market_session: string;
  market_tone: 'bullish' | 'neutral' | 'bearish';
  market_summary: string;
  net_asset: string;
  today_pnl: string;
  today_pnl_percent: number;
  daily_insight: DailyInsight;
  watchlist: WatchlistQuote[];
  value_proof: ValueProof;
}
