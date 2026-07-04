import { http } from './request';
import type {
  ApiKeyConfig,
  CreateApiKeyRequest,
  ProxyConfig,
  TradingConfig,
  RiskConfig,
  NotificationConfig,
  SystemSettings,
  SystemHealth,
  SystemLogEntry,
} from '@/types/settings';

/**
 * 设置服务
 * 对接后端 /system/* 与 /api-keys/* 与 /notifications/settings 接口
 *
 * 后端实际路由：
 * - GET  /api-keys              API Key 列表
 * - POST /api-keys              创建 API Key
 * - PUT  /api-keys/{id}         更新 API Key
 * - DELETE /api-keys/{id}       删除 API Key
 * - POST /api-keys/{id}/test    测试 API Key
 * - POST /api-keys/{id}/toggle  切换启用
 * - GET  /system/proxy          代理配置
 * - PUT  /system/proxy          更新代理
 * - PUT  /system/proxy/test     测试代理
 * - GET  /system/all            所有系统设置
 * - GET  /system/{key}          按 key 查询
 * - PUT  /system/{key}          按 key 更新
 * - GET  /notifications/settings 通知设置
 * - PUT  /notifications/settings 更新通知设置
 */
export const settingsService = {
  // ============ API Key 配置 ============

  /** 获取 API Key 列表 */
  async listApiKeys(keyType?: 'exchange' | 'ai_provider'): Promise<ApiKeyConfig[]> {
    const res = await http.get<{ keys: ApiKeyConfig[] } | ApiKeyConfig[]>(
      '/api-keys',
      keyType ? { key_type: keyType } : undefined,
    );
    return Array.isArray(res) ? res : (res.keys || []);
  },

  /** 创建 API Key */
  async createApiKey(req: CreateApiKeyRequest): Promise<ApiKeyConfig> {
    return http.post<ApiKeyConfig>('/api-keys', req);
  },

  /** 更新 API Key */
  async updateApiKey(id: string, data: Partial<CreateApiKeyRequest>): Promise<ApiKeyConfig> {
    return http.put<ApiKeyConfig>(`/api-keys/${encodeURIComponent(id)}`, data);
  },

  /** 删除 API Key */
  async deleteApiKey(id: string): Promise<void> {
    return http.delete(`/api-keys/${encodeURIComponent(id)}`);
  },

  /** 测试 API Key 连通性 */
  async testApiKey(id: string): Promise<{ success: boolean; latency_ms?: number; error?: string }> {
    return http.post(`/api-keys/${encodeURIComponent(id)}/test`);
  },

  /** 切换 API Key 启用状态 */
  async toggleApiKey(id: string): Promise<{ id: string; is_active: boolean }> {
    return http.post(`/api-keys/${encodeURIComponent(id)}/toggle`);
  },

  // ============ 系统设置 ============

  /** 获取代理配置 */
  async getProxyConfig(): Promise<ProxyConfig> {
    return http.get<ProxyConfig>('/system/proxy');
  },

  /** 更新代理配置 */
  async updateProxyConfig(config: ProxyConfig): Promise<ProxyConfig> {
    return http.put<ProxyConfig>('/system/proxy', config);
  },

  /** 测试代理 */
  async testProxy(testUrl: string): Promise<{ success: boolean; latency_ms?: number; error?: string }> {
    return http.put('/system/proxy/test', { test_url: testUrl });
  },

  /** 获取所有系统设置 */
  async getAllSettings(): Promise<SystemSettings> {
    return http.get<SystemSettings>('/system/all');
  },

  /** 按 key 查询设置 */
  async getSetting(key: string): Promise<unknown> {
    return http.get(`/system/${encodeURIComponent(key)}`);
  },

  /** 按 key 更新设置 */
  async updateSetting(key: string, value: unknown): Promise<void> {
    await http.put(`/system/${encodeURIComponent(key)}`, { value });
  },

  // ============ 通知设置 ============

  /** 获取通知设置 */
  async getNotificationConfig(): Promise<NotificationConfig> {
    return http.get<NotificationConfig>('/notifications/settings');
  },

  /** 更新通知设置 */
  async updateNotificationConfig(config: Partial<NotificationConfig>): Promise<NotificationConfig> {
    return http.put<NotificationConfig>('/notifications/settings', config);
  },

  // ============ 交易/风控参数（通过 system/{key} 读写） ============

  /** 获取交易参数配置 */
  async getTradingConfig(): Promise<TradingConfig> {
    return this.getSetting('trading_config') as Promise<TradingConfig>;
  },

  /** 更新交易参数配置 */
  async updateTradingConfig(config: Partial<TradingConfig>): Promise<void> {
    await this.updateSetting('trading_config', config);
  },

  /** 获取风控配置 */
  async getRiskConfig(): Promise<RiskConfig> {
    return this.getSetting('risk_config') as Promise<RiskConfig>;
  },

  /** 更新风控配置 */
  async updateRiskConfig(config: Partial<RiskConfig>): Promise<void> {
    await this.updateSetting('risk_config', config);
  },

  // ============ 系统健康 ============

  /** 获取系统健康状态 */
  async getHealth(): Promise<SystemHealth> {
    return http.get<SystemHealth>('/health');
  },

  /** 获取系统日志（后端未直接提供，暂返回空） */
  async getSystemLogs(_limit?: number): Promise<SystemLogEntry[]> {
    return [];
  },
};

export default settingsService;
