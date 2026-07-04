import { http } from './request';
import type {
  AnalysisRequest,
  AnalysisResult,
  AnalysisHistoryItem,
  ChatSession,
  ChatMessage,
  DebateSession,
  AiProvider,
  AiPrediction,
  AiUsage,
} from '@/types/ai';

/**
 * AI 分析服务
 * 对接后端 /ai/* 与 /chat/* 接口
 *
 * 后端实际路由：
 * - GET  /ai/symbols                   可分析品种
 * - GET  /ai/timeframes                时间周期
 * - POST /ai/analyze/{type}             发起分析（technical/funding/sentiment/comprehensive）
 * - GET  /ai/{type}                    查询分析结果
 * - POST /ai/debate                     启动辩论（SSE 流式）
 * - GET  /ai/debate/{session_id}       辩论详情
 * - GET  /ai/debates                    辩论列表
 * - GET  /ai/usage                     用量统计
 * - POST /ai/generate-report            生成报告
 * - POST /chat/sessions                创建对话会话
 * - GET  /chat/sessions                会话列表
 * - GET  /chat/sessions/{id}/messages  消息列表
 * - POST /chat/sessions/{id}/messages  发送消息
 */
export const aiService = {
  /** 获取可分析品种 */
  async getSymbols(): Promise<string[]> {
    const res = await http.get<{ symbols: string[] }>('/ai/symbols');
    return res.symbols || [];
  },

  /** 获取时间周期 */
  async getTimeframes(): Promise<string[]> {
    const res = await http.get<{ timeframes: string[] }>('/ai/timeframes');
    return res.timeframes || [];
  },

  /** 发起分析 */
  async analyze(type: string, req: AnalysisRequest): Promise<AnalysisResult> {
    return http.post<AnalysisResult>(`/ai/analyze/${type}`, req);
  },

  /** 查询分析历史 */
  async getAnalysisHistory(
    type: string,
    params?: { symbol?: string; limit?: number },
  ): Promise<AnalysisHistoryItem[]> {
    const res = await http.get<{ data: AnalysisHistoryItem[] } | AnalysisHistoryItem[]>(
      `/ai/${type}`,
      params,
    );
    return Array.isArray(res) ? res : (res.data || []);
  },

  /** 启动 AI 辩论（返回 debate_id，事件流需单独拉取） */
  async startDebate(req: { symbol: string; interval?: string }): Promise<{ debate_id: string; status: string }> {
    return http.post('/ai/debate', req);
  },

  /** 获取辩论详情 */
  async getDebate(debateId: string): Promise<DebateSession> {
    return http.get(`/ai/debate/${encodeURIComponent(debateId)}`);
  },

  /** 获取辩论列表 */
  async listDebates(params?: { symbol?: string; limit?: number }): Promise<DebateSession[]> {
    const res = await http.get<{ debates: DebateSession[] } | DebateSession[]>(
      '/ai/debates',
      params,
    );
    return Array.isArray(res) ? res : (res.debates || []);
  },

  /** 获取 AI 用量 */
  async getUsage(): Promise<AiUsage> {
    return http.get<AiUsage>('/ai/usage');
  },

  // ============ AI 对话 ============

  /** 创建对话会话 */
  async createChatSession(title?: string): Promise<ChatSession> {
    return http.post<ChatSession>('/chat/sessions', { title });
  },

  /** 获取对话会话列表 */
  async listChatSessions(limit?: number): Promise<ChatSession[]> {
    const res = await http.get<{ sessions: ChatSession[] } | ChatSession[]>(
      '/chat/sessions',
      { limit },
    );
    return Array.isArray(res) ? res : (res.sessions || []);
  },

  /** 获取会话消息 */
  async getChatMessages(
    sessionId: string,
    params?: { limit?: number; offset?: number },
  ): Promise<ChatMessage[]> {
    const res = await http.get<{ messages: ChatMessage[] } | ChatMessage[]>(
      `/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
      params,
    );
    return Array.isArray(res) ? res : (res.messages || []);
  },

  /** 发送对话消息 */
  async sendChatMessage(sessionId: string, content: string): Promise<ChatMessage> {
    return http.post<ChatMessage>(
      `/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
      { content },
    );
  },

  /** 删除对话会话 */
  async deleteChatSession(sessionId: string): Promise<void> {
    return http.delete(`/chat/sessions/${encodeURIComponent(sessionId)}`);
  },

  // ============ AI 预测 ============

  /** 获取预测列表（后端 /ai/prediction 返回 {items:[...], page, page_size}） */
  async getPredictions(params?: {
    symbol?: string;
    status?: string;
    limit?: number;
  }): Promise<AiPrediction[]> {
    const res = await http.get<{ items: AiPrediction[] } | { predictions: AiPrediction[] } | AiPrediction[]>(
      '/ai/prediction',
      params,
    );
    if (Array.isArray(res)) return res;
    return res.items || res.predictions || [];
  },

  /** 获取预测统计摘要 */
  async getPredictionSummary(): Promise<Record<string, unknown>> {
    return http.get('/ai/prediction/statistics/summary');
  },

  // ============ AI 供应商 ============

  /** 获取 AI 供应商列表 */
  async getProviders(): Promise<AiProvider[]> {
    const res = await http.get<{ providers: AiProvider[] } | AiProvider[]>('/ai/providers');
    return Array.isArray(res) ? res : (res.providers || []);
  },
};

export default aiService;
