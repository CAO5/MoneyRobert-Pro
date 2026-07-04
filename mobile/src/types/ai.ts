/**
 * AI 分析相关类型
 * 对接后端 /ai/* 与 /chat/* 接口
 */

/** 分析类型 */
export type AnalysisType = 'technical' | 'funding' | 'sentiment' | 'comprehensive';

/** 分析请求 */
export interface AnalysisRequest {
  symbol: string;
  interval: string;
  strategy_id?: string;
  language?: string;
}

/** 分析结果 */
export interface AnalysisResult {
  analysis_id: string;
  symbol: string;
  interval: string;
  analysis_type: AnalysisType;
  result: string;
  confidence?: number;
  summary?: string;
  key_points?: string[];
  recommendations?: string[];
  created_at: string;
}

/** 分析历史条目 */
export interface AnalysisHistoryItem {
  analysis_id: string;
  symbol: string;
  interval: string;
  analysis_type: AnalysisType;
  summary: string;
  confidence: number;
  created_at: string;
}

/** AI 对话会话 */
export interface ChatSession {
  session_id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

/** 对话消息 */
export interface ChatMessage {
  message_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  created_at: string;
}

/** 辩论事件 */
export interface DebateEvent {
  agent: string;
  message: string;
  stance?: 'bullish' | 'bearish' | 'neutral';
  confidence?: number;
  timestamp?: string;
}

/** 辩论会话 */
export interface DebateSession {
  debate_id: string;
  symbol: string;
  status: string;
  events: DebateEvent[];
  final_recommendation?: string;
  created_at: string;
  completed_at?: string;
}

/** AI 供应商 */
export interface AiProvider {
  id: string;
  provider: string;
  api_key_masked: string;
  base_url: string;
  model: string;
  max_tokens?: number;
  temperature?: number;
  is_default: boolean;
}

/** AI 预测 */
export interface AiPrediction {
  prediction_id: string;
  symbol: string;
  target_horizon_sec: number;
  p_up: number;
  p_down: number;
  p_flat: number;
  status: string;
  model_version: string;
  ai_provider?: string;
  model_name?: string;
  created_at: string;
}

/** AI 用量统计 */
export interface AiUsage {
  total_requests: number;
  requests_today: number;
  tokens_used: number;
  tokens_limit: number;
  remaining: number;
}
