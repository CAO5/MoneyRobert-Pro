import { http } from './request';
import type {
  Position,
  Order,
  Trade,
  AccountBalance,
  CreateOrderRequest,
  ProfitCurvePoint,
  AssetDistribution,
} from '@/types/trading';

/**
 * 交易服务
 * 对接后端 /trading/* 与 /dashboard/* 接口
 *
 * 重要：后端 /trading/* 直接透传 OKX 原始字段（camelCase + 缩写），
 *       本 service 在适配层做字段转换，对上层暴露 snake_case 业务命名。
 *
 * 后端实际路由：
 * - GET  /trading/balance           账户余额（OKX 原始字段）
 * - GET  /trading/positions          持仓列表（OKX 原始字段）
 * - GET  /trading/orders             订单列表（参数 page_size，无 total）
 * - POST /trading/orders             创建订单
 * - POST /trading/orders/{id}/cancel 撤销订单
 * - GET  /trading/trades             成交记录（参数 page_size）
 * - POST /trading/positions/sl-tp   单标的设置止损止盈（注意是 POST，需 side+size）
 * - GET  /dashboard/metrics          仪表盘指标
 * - GET  /dashboard/asset-distribution 资产分布
 * - GET  /dashboard/profit-trend     收益曲线
 * - GET  /dashboard/strategy-summary 策略汇总
 */

/** 安全转 number：兼容 OKX 返回的字符串数字 / null / undefined */
function num(v: unknown): number {
  if (v === null || v === undefined) return 0;
  if (typeof v === 'number') return Number.isFinite(v) ? v : 0;
  const n = typeof v === 'string' ? parseFloat(v) : NaN;
  return Number.isFinite(n) ? n : 0;
}

/** OKX balance 原始响应（部分字段） */
interface OkxBalanceRaw {
  totalEq?: string;
  eq?: string;
  imr?: string;
  mmr?: string;
  mgnRatio?: string;
  notionalUsd?: string;
  ordFroz?: string;
  details?: Array<{
    ccy?: string;
    availBal?: string;
    frozenBal?: string;
    eq?: string;
    cashBal?: string;
    upl?: string;
    bal?: string;
  }>;
}

/** OKX positions 响应项（来自后端 trading.rs 的 serde_json::json!） */
interface OkxPositionRaw {
  inst_id?: string;
  inst_type?: string;
  mgn_mode?: string;
  pos_side?: string;
  side?: string;
  size?: number | string;
  avg_px?: string;
  mark_px?: string;
  upl?: string;
  upl_ratio?: string;
  lever?: string;
  liq_px?: string;
  margin?: string;
  notional_usd?: string;
}

/** 后端 dashboard metrics 原始响应（success_response 解包后的 data） */
interface DashboardMetricsRaw {
  total_equity?: number;
  available_balance?: number;
  margin_used?: number;
  unrealized_pnl?: number;
  realized_pnl?: number;
  today_pnl?: number;
  today_pnl_percent?: number;
  active_strategies?: number;
  total_strategies?: number;
  open_positions?: number;
  avg_win_rate?: number;
}

/** 后端 asset-distribution 原始项 */
interface AssetDistributionRaw {
  currency?: string;
  balance?: number;
  available?: number;
  equity?: number;
  equity_usdt?: number;
  percent?: number;
  price_usdt?: number;
}

/** 后端 strategy-summary 原始项 */
interface StrategySummaryRaw {
  id?: number;
  name?: string;
  symbol?: string;
  status?: string;
  profit?: number;
  profit_percent?: number;
  win_rate?: number;
  total_trades?: number;
  max_drawdown?: number;
}

/** OKX position → 移动端 Position */
function mapPosition(p: OkxPositionRaw): Position {
  const entry = num(p.avg_px);
  const upl = num(p.upl);
  return {
    symbol: p.inst_id || '',
    side: (p.side as Position['side']) || 'net',
    size: num(p.size),
    entry_price: entry,
    mark_price: num(p.mark_px),
    unrealized_pnl: upl,
    unrealized_pnl_pct: num(p.upl_ratio) * 100,
    leverage: num(p.lever) || 1,
    margin: num(p.margin),
    liquidation_price: p.liq_px ? num(p.liq_px) : undefined,
    funding_rate: undefined,
    updated_at: undefined,
  };
}

export const tradingService = {
  /** 获取账户余额（OKX 原始字段 → AccountBalance） */
  async getBalance(): Promise<AccountBalance> {
    const raw = await http.get<OkxBalanceRaw | OkxBalanceRaw[]>('/trading/balance');
    const item = Array.isArray(raw) ? (raw[0] || {}) : raw;
    const details = item.details?.[0];
    return {
      total_equity: num(item.totalEq ?? item.eq),
      available_balance: num(details?.availBal ?? item.eq),
      margin_used: num(item.imr ?? item.ordFroz),
      margin_ratio: num(item.mgnRatio),
      unrealized_pnl: num(details?.upl),
    };
  },

  /** 获取持仓列表（OKX 字段 → Position） */
  async getPositions(): Promise<Position[]> {
    const res = await http.get<{ positions: OkxPositionRaw[] } | OkxPositionRaw[]>(
      '/trading/positions',
    );
    const list = Array.isArray(res) ? res : (res.positions || []);
    return list.map(mapPosition);
  },

  /** 获取订单列表（参数 limit→page_size，响应 total 兜底） */
  async getOrders(params?: {
    symbol?: string;
    status?: string;
    limit?: number;
    page?: number;
  }): Promise<{ orders: Order[]; total: number }> {
    const res = await http.get<{ orders: Order[]; total?: number }>('/trading/orders', {
      symbol: params?.symbol,
      status: params?.status,
      page: params?.page,
      page_size: params?.limit,
    });
    const orders = res?.orders || [];
    // 后端不返回 total，用当前返回数量兜底（避免前端 total=undefined 导致分页异常）
    return { orders, total: res?.total ?? orders.length };
  },

  /** 创建订单 */
  async createOrder(req: CreateOrderRequest): Promise<Order> {
    return http.post<Order>('/trading/orders', req);
  },

  /** 撤销订单 */
  async cancelOrder(orderId: string): Promise<void> {
    return http.post(`/trading/orders/${encodeURIComponent(orderId)}/cancel`);
  },

  /** 获取成交记录（参数 limit→page_size） */
  async getTrades(params?: { symbol?: string; limit?: number }): Promise<Trade[]> {
    const res = await http.get<{ trades: Trade[] } | Trade[]>('/trading/trades', {
      symbol: params?.symbol,
      page_size: params?.limit,
    });
    return Array.isArray(res) ? res : (res.trades || []);
  },

  /**
   * 批量设置止损止盈
   * 后端为 POST /trading/positions/sl-tp 单标的接口，需 side+size，
   * 这里串行调用每个 position 并聚合结果
   */
  async setStopLossTakeProfit(
    positions: Array<{ symbol: string; stop_loss?: number; take_profit?: number }>,
  ): Promise<{ updated: string[]; failed: string[] }> {
    // 批量接口缺 side/size，先查当前持仓补齐
    let currentPositions: Position[] = [];
    try {
      currentPositions = await this.getPositions();
    } catch {
      // 查不到也没关系，下方按 side=long 兜底
    }
    const updated: string[] = [];
    const failed: string[] = [];
    for (const p of positions) {
      const cur = currentPositions.find((c) => c.symbol === p.symbol);
      try {
        await http.post('/trading/positions/sl-tp', {
          symbol: p.symbol,
          side: cur?.side || 'long',
          pos_side: cur?.side === 'long' ? 'long' : cur?.side === 'short' ? 'short' : undefined,
          size: cur?.size || 0,
          stop_loss: p.stop_loss,
          take_profit: p.take_profit,
        });
        updated.push(p.symbol);
      } catch {
        failed.push(p.symbol);
      }
    }
    return { updated, failed };
  },

  /** 获取仪表盘指标（字段补齐：margin_ratio/leverage 后端不返回，兜底 0） */
  async getDashboardMetrics(): Promise<{
    total_equity: number;
    available_balance: number;
    unrealized_pnl: number;
    realized_pnl: number;
    margin_used: number;
    margin_ratio: number;
    leverage: number;
    open_positions: number;
    today_pnl: number;
    today_pnl_pct: number;
  }> {
    const raw = await http.get<DashboardMetricsRaw>('/dashboard/metrics');
    return {
      total_equity: num(raw?.total_equity),
      available_balance: num(raw?.available_balance),
      unrealized_pnl: num(raw?.unrealized_pnl),
      realized_pnl: num(raw?.realized_pnl),
      margin_used: num(raw?.margin_used),
      // 后端不返回，置 0（前端已 ?? 0 兜底）
      margin_ratio: num(raw?.margin_ratio),
      leverage: num(raw?.leverage) || 1,
      open_positions: num(raw?.open_positions),
      today_pnl: num(raw?.today_pnl),
      // 后端字段为 today_pnl_percent，映射到移动端 today_pnl_pct
      today_pnl_pct: num(raw?.today_pnl_percent),
    };
  },

  /** 获取资产分布（字段映射 currency→asset, balance→amount, equity_usdt→value_usdt, percent→percentage） */
  async getAssetDistribution(): Promise<AssetDistribution[]> {
    const raw = await http.get<AssetDistributionRaw[]>('/dashboard/asset-distribution');
    return (raw || []).map((r) => ({
      asset: r.currency || '',
      amount: num(r.balance),
      value_usdt: num(r.equity_usdt ?? r.equity),
      percentage: num(r.percent),
    }));
  },

  /** 获取收益曲线（后端字段 date → timestamp） */
  async getProfitTrend(period?: 'week' | 'month'): Promise<ProfitCurvePoint[]> {
    const raw = await http.get<Array<{ date?: string; pnl?: number; equity?: number; cumulative_pnl?: number }>>(
      '/dashboard/profit-trend',
      { period },
    );
    return (raw || []).map((r) => ({
      timestamp: r.date || '',
      equity: num(r.equity),
      pnl: num(r.pnl),
    }));
  },

  /** 获取策略汇总（字段映射 id→strategy_id, profit→pnl, profit_percent→pnl_pct, total_trades→positions） */
  async getStrategySummary(): Promise<Array<{
    strategy_id: string;
    name: string;
    status: string;
    pnl: number;
    pnl_pct: number;
    positions: number;
  }>> {
    const raw = await http.get<StrategySummaryRaw[]>('/dashboard/strategy-summary');
    return (raw || []).map((r) => ({
      strategy_id: String(r.id ?? ''),
      name: r.name || r.symbol || '',
      status: r.status || 'unknown',
      pnl: num(r.profit),
      pnl_pct: num(r.profit_percent),
      positions: num(r.total_trades),
    }));
  },
};

export default tradingService;
