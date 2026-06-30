import { http, MOCK_ENABLED } from './request';
import type { MarketSnapshot, MarketRegime } from '@/types/market';
import { mockMarketSnapshots, mockMarketRegimes } from '@/data/market';

/**
 * 行情服务
 * 对接后端 /market/* 与 /features/* 接口
 *
 * 后端实际路由（见 backend/src/routes）：
 * - GET /market/snapshots          暂未实现 → mock 兜底
 * - GET /market/snapshots/{symbol} 暂未实现 → mock 兜底
 * - GET /features/regimes/history  查询市场状态历史（symbol 可选）
 * - GET /features/regimes/latest/{symbol}  查询某标的最新状态（必须带 symbol）
 */
export const marketService = {
  /** 获取行情快照列表 */
  async getSnapshots(symbols?: string[]): Promise<MarketSnapshot[]> {
    void symbols;
    if (MOCK_ENABLED) {
      return mockMarketSnapshots();
    }
    // 后端 /market/snapshots 暂未实现，mock 兜底避免阻断页面
    return mockMarketSnapshots();
  },

  /** 获取单标的行情 */
  async getSnapshot(symbol: string): Promise<MarketSnapshot> {
    if (MOCK_ENABLED) {
      const list = await mockMarketSnapshots();
      return list.find((s) => s.symbol === symbol) || list[0];
    }
    // 后端 /market/snapshots/{symbol} 暂未实现，mock 兜底
    const list = await mockMarketSnapshots();
    return list.find((s) => s.symbol === symbol) || list[0];
  },

  /** 获取市场状态（regime）列表 */
  async getRegimes(): Promise<MarketRegime[]> {
    if (MOCK_ENABLED) {
      return mockMarketRegimes();
    }
    // 后端 /features/regimes/latest 需要 {symbol} 路径参数（无"全部最新"接口）
    // 改用 /features/regimes/history（支持 symbol 可选），取最近一批作为各标的最新状态
    return http.get<MarketRegime[]>('/features/regimes/history', { limit: 50 });
  },

  /** 获取单标的最新市场状态 */
  async getLatestRegime(symbol: string): Promise<MarketRegime> {
    if (MOCK_ENABLED) {
      const list = await mockMarketRegimes();
      return list.find((r) => r.symbol === symbol) || list[0];
    }
    // 后端 /features/regimes/latest/{symbol} 必须带 symbol
    return http.get<MarketRegime>(`/features/regimes/latest/${encodeURIComponent(symbol)}`);
  },
};

export default marketService;
