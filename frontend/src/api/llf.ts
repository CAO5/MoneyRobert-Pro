import api from '@/api'

type Reading = number | null

interface MarketInput {
  status: 'fresh' | 'stale' | 'invalid' | 'missing'
  raw_id?: number
  exchange_ts?: string
  received_at?: string
  age_ms?: number
  source_quality?: string
  values?: Record<string, unknown>
}

interface LlfAsset {
  asset: string
  observed_at: string | null
  score: Reading
  percentile: Reading
  components: { oi_load: Reading; carry: Reading; flow: Reading; liquidity: Reading }
  quality: 'waiting' | 'stale' | 'ready' | 'incomplete'
  reason: string
  inputs: Record<string, Record<string, MarketInput>>
  oi_history: { first_at: string | null; last_at: string | null; sample_days: number }
}

interface PaperTrade {
  position_id: string
  asset: string
  opened_at: string
  closed_at: string | null
  quantity: Reading
  entry_price: Reading
  status: string
  net_pnl: Reading
  fees: Reading
  funding: Reading
  slippage_cost: Reading
  mae_usdt: Reading
  liquidation_hit: boolean | null
  liquidation_at: string | null
  post_liquidation_return_pct: Reading
  post_liquidation_drawdown_pct: Reading
  pending_funding_settlements: number
  fills: Record<string, unknown>[] | null
}

interface PaperEvent {
  signal_id: string
  asset: string
  observed_at: string
  percentile: Reading
  reason: string
  status: string
  entry_mark: Reading
  return_15m_pct: Reading
}

interface LlfPaper {
  account_id: string
  enabled: boolean
  initial_equity: Reading
  equity: Reading
  cash: Reading
  margin_used: Reading
  fee_bps: Reading
  order_usdt: Reading
  positions: { asset: string }[]
  metrics: {
    entries: number
    closed_trades: number
    win_rate: Reading
    avg_holding_seconds: Reading
    max_drawdown_pct: Reading
    max_adverse_usdt: Reading
    net_pnl: Reading
    total_fees: Reading
    slippage_cost: Reading
    funding: Reading
    liquidation_hits: Reading
  }
  trades: PaperTrade[]
  events: PaperEvent[]
}

export interface LlfStatus {
  factor_version: string
  collector_enabled: boolean
  formula_ready: boolean
  formula_status: string
  as_of: string
  assets: LlfAsset[]
  paper: LlfPaper
}

export async function getLlfStatus(signal: AbortSignal): Promise<LlfStatus> {
  const response = await api.post<LlfStatus>('/llf/status', {}, { signal })
  return response.data
}
