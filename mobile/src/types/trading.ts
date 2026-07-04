/**
 * 交易/持仓相关类型
 * 对接后端 /trading/* 与 /dashboard/positions 接口
 */

/** 持仓方向 */
export type PositionSide = 'long' | 'short' | 'net';

/** 订单状态 */
export type OrderStatus = 'pending' | 'open' | 'partial' | 'filled' | 'cancelled' | 'rejected';

/** 订单类型 */
export type OrderType = 'market' | 'limit' | 'stop' | 'stop_limit';

/** 订单方向 */
export type OrderSide = 'buy' | 'sell';

/** 持仓信息 */
export interface Position {
  symbol: string;
  side: PositionSide;
  size: number;
  entry_price: number;
  mark_price: number;
  unrealized_pnl: number;
  unrealized_pnl_pct: number;
  leverage: number;
  margin: number;
  liquidation_price?: number;
  funding_rate?: number;
  updated_at?: string;
}

/** 订单信息 */
export interface Order {
  order_id: string;
  symbol: string;
  side: OrderSide;
  type: OrderType;
  quantity: number;
  price?: number;
  leverage?: number;
  stop_loss?: number;
  take_profit?: number;
  reduce_only?: boolean;
  status: OrderStatus;
  filled_quantity?: number;
  avg_price?: number;
  created_at: string;
  updated_at?: string;
}

/** 成交记录 */
export interface Trade {
  trade_id: string;
  symbol: string;
  side: OrderSide;
  size: number;
  price: number;
  fee: number;
  timestamp: string;
}

/** 账户余额 */
export interface AccountBalance {
  total_equity: number;
  available_balance: number;
  margin_used: number;
  margin_ratio: number;
  unrealized_pnl: number;
}

/** 创建订单请求 */
export interface CreateOrderRequest {
  symbol: string;
  side: OrderSide;
  type: OrderType;
  quantity: number;
  price?: number;
  leverage?: number;
  stop_loss?: number;
  take_profit?: number;
  reduce_only?: boolean;
}

/** 收益曲线点 */
export interface ProfitCurvePoint {
  timestamp: string;
  equity: number;
  pnl: number;
}

/** 资产分布 */
export interface AssetDistribution {
  asset: string;
  amount: number;
  value_usdt: number;
  percentage: number;
}
