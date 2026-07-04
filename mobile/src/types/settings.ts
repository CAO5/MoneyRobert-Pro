/**
 * 设置相关类型
 * 对接后端 /system/* 与 /api-keys/* 接口
 */

/** API Key 类型 */
export type ApiKeyType = 'exchange' | 'ai_provider';

/** API Key 配置 */
export interface ApiKeyConfig {
  id: string;
  name: string;
  key_type: ApiKeyType;
  metadata: {
    // exchange 类型
    is_demo?: boolean;
    exchange?: string;
    secret?: string;
    // ai_provider 类型
    provider?: string;
    base_url?: string;
    model?: string;
    max_tokens?: number;
    temperature?: number;
    [key: string]: unknown;
  };
  is_active: boolean;
  created_at: string;
}

/** 创建 API Key 请求 */
export interface CreateApiKeyRequest {
  name: string;
  key_type: ApiKeyType;
  api_key: string;
  secret?: string;
  metadata?: Record<string, unknown>;
}

/** 代理配置 */
export interface ProxyConfig {
  enabled: boolean;
  url: string;
  proxy_type: string;
  test_url: string;
}

/** 交易参数配置 */
export interface TradingConfig {
  default_leverage: number;
  max_position_size: number;
  max_total_leverage: number;
  max_daily_loss_pct: number;
  default_stop_loss_pct: number;
  default_take_profit_pct: number;
  enable_auto_trading: boolean;
  enable_paper_trading: boolean;
}

/** 风控配置 */
export interface RiskConfig {
  max_portfolio_risk: number;
  max_single_position_risk: number;
  var_limit: number;
  cvar_limit: number;
  max_drawdown_limit: number;
  daily_loss_limit: number;
  margin_call_threshold: number;
  enable_risk_alerts: boolean;
}

/** 通知配置 */
export interface NotificationConfig {
  enable_push: boolean;
  enable_email: boolean;
  enable_trade_notifications: boolean;
  enable_risk_alerts: boolean;
  enable_news_notifications: boolean;
  enable_agent_notifications: boolean;
  enable_system_notifications: boolean;
  quiet_hours_start?: string;
  quiet_hours_end?: string;
}

/** 系统设置 */
export interface SystemSettings {
  proxy?: ProxyConfig;
  trading?: TradingConfig;
  risk?: RiskConfig;
  notification?: NotificationConfig;
  [key: string]: unknown;
}

/** 系统健康状态 */
export interface SystemHealth {
  status: string;
  version: string;
  environment: string;
  timestamp: string;
  redis: string;
  database: string;
}

/** 系统日志条目 */
export interface SystemLogEntry {
  log_id: string;
  level: 'info' | 'warning' | 'error' | 'debug';
  module: string;
  message: string;
  context?: Record<string, unknown>;
  timestamp: string;
}
