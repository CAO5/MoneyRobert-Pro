/**
 * 行情数据相关类型
 * 对接后端 /market/* 接口
 */

/** 市场行情快照 */
export interface MarketSnapshot {
  symbol: string;
  current_price: number;
  open_24h: number;
  high_24h: number;
  low_24h: number;
  close_24h: number;
  volume_24h: number;
  price_change_percent_24h: number;
  funding_rate?: number;
  open_interest?: number;
  long_short_ratio?: number;
  rsi_14?: number;
  macd_signal?: number;
  timestamp: string;
}

/** 收藏标的 */
export interface FavoriteSymbol {
  symbol: string;
  name?: string;
  added_at: string;
}

/** 市场状态 */
export interface MarketRegime {
  symbol: string;
  regime: string; // trending_up / trending_down / ranging / volatile
  confidence: number;
  detected_at: string;
}
