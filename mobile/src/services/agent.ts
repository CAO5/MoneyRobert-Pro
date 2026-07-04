import { http } from './request';
import type {
  AgentInfo,
  AgentTrainingStatus,
  AgentAccuracy,
  AgentPermission,
  AgentLogEntry,
  AgentDashboard,
  AgentPerformance,
  SimulationStatus,
} from '@/types/agent';

/**
 * Agent 管理服务
 * 对接后端 /agent/* 接口
 *
 * 后端实际路由：
 * - GET  /agent/dashboard              Agent 仪表盘汇总
 * - GET  /agent/analyze/agents          Agent 列表
 * - GET  /agent/analyze/performance     Agent 性能数据
 * - POST /agent/simulation/start        启动模拟
 * - POST /agent/simulation/stop         停止模拟
 * - GET  /agent/simulation/status       模拟状态
 * - GET  /agent/simulation/trades       模拟成交
 * - GET  /agent/simulation/stats       模拟统计
 * - POST /agent/autonomous/start        启动自主模式
 * - POST /agent/autonomous/stop         停止自主模式
 * - POST /agent/emergency/stop          紧急停止
 * - GET  /agent/performance             性能数据
 */
export const agentService = {
  /** 获取 Agent 仪表盘汇总 */
  async getDashboard(): Promise<AgentDashboard> {
    return http.get<AgentDashboard>('/agent/dashboard');
  },

  /** 获取 Agent 列表 */
  async getAgents(): Promise<AgentInfo[]> {
    const res = await http.get<{ agents: AgentInfo[] } | AgentInfo[]>(
      '/agent/analyze/agents',
    );
    return Array.isArray(res) ? res : (res.agents || []);
  },

  /** 获取 Agent 性能 */
  async getPerformance(agentId?: string): Promise<AgentPerformance | AgentPerformance[]> {
    const params = agentId ? { agent_id: agentId } : undefined;
    return http.get('/agent/analyze/performance', params);
  },

  /** 启动模拟 */
  async startSimulation(configId: string): Promise<{ simulation_id: string; status: string }> {
    return http.post('/agent/simulation/start', { config_id: configId });
  },

  /** 停止模拟 */
  async stopSimulation(): Promise<void> {
    return http.post('/agent/simulation/stop');
  },

  /** 获取模拟状态 */
  async getSimulationStatus(): Promise<SimulationStatus> {
    return http.get<SimulationStatus>('/agent/simulation/status');
  },

  /** 获取模拟成交 */
  async getSimulationTrades(limit?: number): Promise<unknown[]> {
    const res = await http.get<{ trades: unknown[] } | unknown[]>(
      '/agent/simulation/trades',
      { limit },
    );
    return Array.isArray(res) ? res : (res.trades || []);
  },

  /** 获取模拟统计 */
  async getSimulationStats(): Promise<Record<string, unknown>> {
    return http.get('/agent/simulation/stats');
  },

  /** 启动自主模式 */
  async startAutonomous(): Promise<void> {
    return http.post('/agent/autonomous/start');
  },

  /** 停止自主模式 */
  async stopAutonomous(): Promise<void> {
    return http.post('/agent/autonomous/stop');
  },

  /** 紧急停止 */
  async emergencyStop(): Promise<void> {
    return http.post('/agent/emergency/stop');
  },

  /** 获取 Agent 训练状态（基于 dashboard 数据推断） */
  async getTrainingStatus(agentId: string): Promise<AgentTrainingStatus> {
    // 后端未直接提供训练状态接口，基于 Agent 列表推断
    const agents = await this.getAgents();
    const agent = agents.find((a) => a.agent_id === agentId);
    return {
      agent_id: agentId,
      status: agent?.status || 'idle',
      progress: agent?.status === 'training' ? 50 : 0,
      started_at: agent?.updated_at || new Date().toISOString(),
    };
  },

  /** 获取 Agent 准确率（基于 performance 推断） */
  async getAccuracy(agentId: string): Promise<AgentAccuracy> {
    const perf = await this.getPerformance(agentId) as AgentPerformance;
    return {
      agent_id: agentId,
      overall_accuracy: perf?.win_rate || 0,
      recent_accuracy: perf?.win_rate || 0,
      total_predictions: perf?.total_trades || 0,
      correct_predictions: Math.round((perf?.total_trades || 0) * (perf?.win_rate || 0) / 100),
    };
  },

  /** 获取 Agent 权限（暂走本地默认，后端未实现） */
  async getPermissions(agentId: string): Promise<AgentPermission> {
    return {
      agent_id: agentId,
      can_trade: false,
      can_modify_strategy: false,
      max_position_size: 0,
      max_leverage: 1,
      allowed_symbols: [],
      restricted_actions: [],
    };
  },

  /** 获取 Agent 日志（后端未直接提供，暂返回空） */
  async getLogs(agentId: string, limit?: number): Promise<AgentLogEntry[]> {
    void agentId;
    void limit;
    return [];
  },
};

export default agentService;
