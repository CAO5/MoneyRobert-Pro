import { http, MOCK_ENABLED } from './request';
import type { DecisionCard, DecisionCardListItem } from '@/types/signal';
import { mockDecisionCards, mockDecisionCardDetail } from '@/data/decision-card';

/**
 * 决策卡服务
 * 对接后端 /signals/decision-cards 接口
 *
 * 后端实际路由（见 backend/src/routes/signals_api.rs）：
 * - GET  /signals/decision-cards          查询用户决策卡列表 ✅ 已联通
 * - GET  /signals/decision-cards/{cardId} 决策卡详情 ✅ 已联通
 * - POST /signals/decision-card           创建决策卡
 */
export const signalService = {
  /** 查询决策卡列表 */
  async listCards(limit = 20): Promise<DecisionCardListItem[]> {
    if (MOCK_ENABLED) {
      return mockDecisionCards().slice(0, limit);
    }
    return http.get<DecisionCardListItem[]>('/signals/decision-cards', { limit });
  },

  /** 查询决策卡详情 */
  async getCard(cardId: string): Promise<DecisionCard> {
    if (MOCK_ENABLED) {
      return mockDecisionCardDetail(cardId);
    }
    // 后端返回 DecisionCardResponse（扁平结构，字段 snake_case），unwrapResponse 走 fallback 直接返回
    return http.get<DecisionCard>(
      `/signals/decision-cards/${encodeURIComponent(cardId)}`
    );
  },
};

export default signalService;
