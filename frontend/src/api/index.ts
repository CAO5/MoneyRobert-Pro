import axios from 'axios'

const api = axios.create({
  baseURL: import.meta.env.VITE_API_URL || '/api/v1',
  timeout: 30000,
  headers: { 'Content-Type': 'application/json' },
})

api.interceptors.request.use((config) => {
  const token = localStorage.getItem('access_token')
  if (token) {
    config.headers.Authorization = `Bearer ${token}`
  }
  return config
})

api.interceptors.response.use(
  (response) => {
    const { data } = response
    if (data && typeof data === 'object') {
      if ('success' in data && data.success === true && 'data' in data) {
        response.data = data.data
        return response
      }
      if ('code' in data && (data.code === 200 || data.code === 0) && 'data' in data) {
        response.data = data.data
        return response
      }
      if ('error' in data && typeof data.error === 'string') {
        return Promise.reject(new Error(data.message || '请求失败'))
      }
    }
    return response
  },
  async (error) => {
    if (error.response?.status === 401) {
      const refreshToken = localStorage.getItem('refresh_token')
      if (refreshToken) {
        try {
          const { data } = await axios.post(
            `${import.meta.env.VITE_API_URL || '/api/v1'}/auth/refresh`,
            { refresh_token: refreshToken }
          )
          localStorage.setItem('access_token', data.access_token)
          localStorage.setItem('refresh_token', data.refresh_token)
          error.config.headers.Authorization = `Bearer ${data.access_token}`
          return axios(error.config)
        } catch {
          localStorage.removeItem('access_token')
          localStorage.removeItem('refresh_token')
          window.location.href = '/login'
        }
      } else {
        localStorage.removeItem('access_token')
        window.location.href = '/login'
      }
    }

    if (error.response) {
      const { data } = error.response
      const message = data?.message || data?.error?.message || data?.detail?.[0]?.msg || '请求失败'
      return Promise.reject(new Error(message))
    }

    return Promise.reject(error)
  }
)

export interface StartSimulationRequest {
  symbol: string
  initial_balance?: number
}

export interface StartSimulationResponse {
  config_id: string
  status: string
}

export interface SimulationStatusResponse {
  config: AiSimulationConfig
  status: string
}

export interface TradesResponse {
  trades: AiSimulationTrade[]
}

export interface StatsResponse {
  config: AiSimulationConfig
}

export interface LevelResponse {
  current_level: number
  next_level?: number
  eligibility?: PromotionEligibility
}

export interface StartDebateRequest {
  symbol: string
  config_id?: string
}

export interface StartDebateResponse {
  session_id: string
  status: string
}

export interface ApprovePromotionRequest {
  audit_id: string
  review_comment?: string
}

export interface ApprovePromotionResponse {
  config: AiSimulationConfig
  status: string
}

export interface SignRiskConfirmationRequest {
  config_id?: string
  version: string
  max_acceptable_loss: number
  accept_reason?: string
}

export interface SignRiskConfirmationResponse {
  confirmation_id: string
  signed: boolean
}

export interface AiSimulationConfig {
  id: string
  user_id: number
  symbol: string
  mode: string
  level: number
  status: string
  initial_balance: number
  current_balance: number
  max_position_size_percent: number
  max_leverage: number
  max_daily_trades: number
  max_daily_loss_percent: number
  max_weekly_loss_percent: number
  max_single_trade_loss_percent: number
  ai_confidence_threshold: number
  analysis_interval_minutes: number
  allowed_symbols: string[]
  autonomous_mode_enabled: boolean
  requires_manual_confirm: boolean
  total_trades: number
  winning_trades: number
  losing_trades: number
  win_rate: number
  avg_pnl_percent: number
  profit_loss_ratio: number
  max_drawdown_percent: number
  sharpe_ratio: number
  weekly_pnl: number
  weekly_loss_percent: number
  daily_pnl: number
  daily_loss_percent: number
  consecutive_stop_losses: number
  running_days: number
  last_trade_at?: string
  promotion_eligible: boolean
  risk_confirmation_signed: boolean
  risk_confirmation_signed_at?: string
  max_acceptable_loss_amount?: number
  created_at: string
  updated_at: string
}

export interface AiSimulationTrade {
  id: string
  config_id: string
  symbol: string
  mode: string
  direction: string
  entry_price: number
  exit_price?: number
  quantity: number
  leverage: number
  stop_loss?: number
  take_profit?: number
  ai_confidence?: number
  ai_reasoning?: any
  agent_session_id?: string
  pnl?: number
  pnl_percent?: number
  fee_percent: number
  net_pnl_percent?: number
  status: string
  close_reason?: string
  holding_duration_minutes?: number
  opened_at: string
  closed_at?: string
}

export interface MarketSnapshot {
  symbol: string
  current_price: number
  open_24h: number
  high_24h: number
  low_24h: number
  close_24h: number
  volume_24h: number
  price_change_percent_24h: number
  funding_rate?: number
  open_interest?: number
  long_short_ratio?: number
  rsi_14?: number
  macd_signal?: number
  timestamp: string
}

export interface DebateSession {
  id: string
  config_id?: string
  user_id?: number
  symbol: string
  status: string
  messages: DebateMessage[]
  final_decision?: FundManagerDecision
  created_at: string
  updated_at: string
}

export interface DebateMessage {
  id: string
  session_id: string
  agent_name: string
  agent_department: string
  role: string
  content: string
  analysis_data: any
  confidence: number
  sentiment?: string
  message_order: number
  created_at: string
}

export interface FundManagerDecision {
  session_id: string
  action: string
  symbol: string
  confidence: number
  position_size_percent: number
  leverage: number
  stop_loss_percent?: number
  take_profit_percent?: number
  reasoning: string
  agent_contributions: AgentContribution[]
  risk_assessment: RiskAssessment
  timestamp: string
}

export interface AgentContribution {
  agent_name: string
  department: string
  sentiment: string
  confidence: number
  contribution_weight: number
  credibility_score: number
}

export interface RiskAssessment {
  overall_risk_level: string
  max_position_risk: number
  margin_requirement: number
  risk_reward_ratio: number
  volatility_rating: string
  alerts: string[]
}

export interface RollingStats {
  total_trades: number
  winning_trades: number
  losing_trades: number
  win_rate: number
  avg_pnl_percent: number
  profit_loss_ratio: number
  max_drawdown_percent: number
  running_days: number
  daily_loss_percent: number
  consecutive_days_without_risk_trigger: number
  weekly_loss_percent: number
}

export interface PromotionEligibility {
  eligible: boolean
  current_level: number
  next_level?: number
  stats: RollingStats
  requirements_met: boolean
  missing_requirements: string[]
}

export class AgentApi {
  static async startSimulation(req: StartSimulationRequest) {
    const { data } = await api.post<StartSimulationResponse>('/agent/simulation/start', req)
    return data
  }

  static async stopSimulation(config_id: string) {
    const { data } = await api.post('/agent/simulation/stop', { config_id })
    return data
  }

  static async getSimulationStatus() {
    const { data } = await api.get<SimulationStatusResponse>('/agent/simulation/status')
    return data
  }

  static async getTrades() {
    const { data } = await api.get<TradesResponse>('/agent/simulation/trades')
    return data
  }

  static async getStats() {
    const { data } = await api.get<StatsResponse>('/agent/simulation/stats')
    return data
  }

  static async getLevel() {
    const { data } = await api.get<LevelResponse>('/agent/simulation/level')
    return data
  }

  static async startDebate(req: StartDebateRequest) {
    const { data } = await api.post<StartDebateResponse>('/agent/debate/start', req)
    return data
  }

  static async getDebateSession(id: string) {
    const { data } = await api.get<DebateSession>(`/agent/debate/${id}`)
    return data
  }

  static async approvePromotion(req: ApprovePromotionRequest) {
    const { data } = await api.post<ApprovePromotionResponse>('/agent/promotion/approve', req)
    return data
  }

  static async signRiskConfirmation(req: SignRiskConfirmationRequest) {
    const { data } = await api.post<SignRiskConfirmationResponse>('/agent/risk/confirmation/sign', req)
    return data
  }

  static async startAutonomous(config_id: string) {
    const { data } = await api.post('/agent/autonomous/start', { config_id })
    return data
  }

  static async stopAutonomous(config_id: string) {
    const { data } = await api.post('/agent/autonomous/stop', { config_id })
    return data
  }

  static async emergencyStop(config_id: string) {
    const { data } = await api.post('/agent/emergency/stop', { config_id })
    return data
  }
}

// =========================================================
// 概率信号与决策卡 API（/signals/*）
// =========================================================

/// 创建决策卡请求
export interface CreateDecisionCardRequest {
  symbol: string
  /// 预测周期（秒）
  target_horizon_sec: number
  /// 概率分布（p_up + p_down + p_flat = 1）
  p_up: number
  p_down: number
  p_flat: number
  /// 收益分位数
  q10?: number
  q50?: number
  q90?: number
  /// 预期波动率
  expected_volatility?: number
  /// 模型版本
  model_version: string
  /// 市场状态
  market_regime?: string
  /// 净期望 EV（扣除费用/滑点/资金费率后）
  expected_value: number
  /// 仓位建议（0-1）
  position_suggestion: number
  /// 最坏情形（CVaR 口径）
  worst_case?: number
  /// 已用风险预算
  risk_budget_used?: number
  /// 数据新鲜度（秒）
  data_freshness_sec?: number
  /// 支持证据
  supporting_evidence?: Record<string, unknown>
  /// 反对证据
  opposing_evidence?: Record<string, unknown>
  /// 样本表现
  sample_performance?: Record<string, unknown>
  /// 数据血缘
  data_lineage?: Record<string, unknown>
  /// 失效条件
  invalidation_conditions?: Record<string, unknown>
}

/// 决策卡响应
export interface DecisionCardResponse {
  card_id: string
  symbol: string
  generated_at: string
  /// 建议动作：open_long / open_short / close / hold / reduce
  suggested_action: string
  target_horizon_sec: number
  p_up: number
  p_down: number
  p_flat: number
  q10?: number
  q50?: number
  q90?: number
  /// 净期望 EV
  expected_value: number
  /// 最坏情形（CVaR）
  worst_case?: number
  /// 仓位建议（0-1）
  position_suggestion: number
  /// 已用风险预算
  risk_budget_used?: number
  /// 适用市场状态
  applicable_regime?: string
  /// 数据新鲜度（秒）
  data_freshness_sec?: number
  /// 失效条件
  invalidation_conditions?: Record<string, unknown>
  /// 模型版本
  model_version: string
}

/// 校准报告响应
export interface CalibrationResponse {
  report_id: string
  model_version: string
  symbol?: string
  market_regime?: string
  eval_start: string
  eval_end: string
  brier_score: number
  log_loss: number
  accuracy: number
  calibration_error?: number
  calibration_curve: unknown
  sample_count: number
  is_well_calibrated: boolean
  degradation_detected: boolean
}

export class SignalApi {
  /// 生成概率决策卡
  static async createDecisionCard(req: CreateDecisionCardRequest) {
    const { data } = await api.post<DecisionCardResponse>('/signals/decision-card', req)
    return data
  }

  /// 查询用户决策卡列表
  static async listDecisionCards(limit = 20) {
    const { data } = await api.get<DecisionCardResponse[]>('/signals/decision-cards', {
      params: { limit },
    })
    return data
  }

  /// 查询概率校准报告
  static async getCalibration(modelVersion: string) {
    const { data } = await api.get<CalibrationResponse>('/signals/calibration', {
      params: { model_version: modelVersion },
    })
    return data
  }

  /// 触发校准计算
  static async computeCalibration(req: {
    model_version: string
    symbol?: string
    start_time: string
    end_time: string
  }) {
    const { data } = await api.post<CalibrationResponse>('/signals/calibration/compute', req)
    return data
  }

  /// P2-2: 生成交易建议卡片
  static async generateTradeRecommendation(req: TradeRecommendationRequest) {
    const { data } = await api.post<TradeRecommendationResponse>('/signals/trade-recommendation', req)
    return data
  }
}

/// 交易建议请求
export interface TradeRecommendationRequest {
  symbol: string
  direction: string
  confidence: number
  signal_strength: number
  expected_return_bps?: number
  current_price: number
  asset_volatility?: number
  total_equity: number
  margin_used?: number
  existing_position_notional?: number
  market_regime?: string
}

/// 交易建议响应
export interface TradeRecommendationResponse {
  trace_id: string
  action: string
  executable: boolean
  confidence: number
  expected_value: number
  cvar: number
  trust_level: string
  position_pct: number
  suggested_notional: number
  stop_loss_price?: number
  take_profit_price?: number
  reasons: string[]
  risks: string[]
  blockers: string[]
  pipeline_steps: string[]
  generated_at: string
}

// =========================================================
// 回测与可信等级 API（/backtest/*）
// =========================================================

/// 回测任务摘要（列表项）
export interface BacktestJobSummary {
  job_id: string
  job_name: string
  strategy_id?: string
  status: string
  progress: number
  start_time: string
  end_time: string
  initial_equity: number
  total_trades?: number
  total_return_pct?: number
  sharpe_ratio?: number
  max_drawdown_pct?: number
  created_at: string
}

/// 回测任务详情
export interface BacktestJobDetail extends BacktestJobSummary {
  winning_trades?: number
  fee_total?: number
  slippage_total?: number
  completed_at?: string
  mode: string
  data_frequency: string
  fee_taker_bps: number
  fee_maker_bps: number
  slippage_bps: number
  max_single_position_pct: number
  max_total_leverage: number
  max_daily_loss_pct: number
  assets: string[]
}

/// 回测绩效报告
export interface BacktestReport {
  report_id: string
  total_return?: number
  annualized_return?: number
  max_drawdown?: number
  sharpe_ratio?: number
  win_rate?: number
  profit_factor?: number
  total_trades?: number
  winning_trades?: number
  losing_trades?: number
  average_win?: number
  average_loss?: number
  payoff_ratio?: number
  total_fee?: number
  by_agent?: Record<string, unknown>
  by_asset?: Record<string, unknown>
  report?: Record<string, unknown>
}

/// 回测可信等级评估
export interface TrustLevelResponse {
  assessment_id: string
  job_id: string
  /// 可信等级：display_only / comparable / promotion_eligible
  trust_level: string
  test_coverage_passed: boolean
  capital_conservation_passed: boolean
  slippage_accounted: boolean
  data_quality_grade: string
  sample_size_sufficient: boolean
  walk_forward_validated: boolean
  calibration_healthy: boolean
  total_trades: number
  test_pass_rate: number
  data_coverage_ratio: number
  issues: unknown
  recommendations: unknown
  promotion_eligible: boolean
  promotion_blockers: unknown
  assessed_at: string
}

/// 创建回测任务请求
export interface CreateBacktestJobRequest {
  job_name: string
  strategy_id?: string
  assets: string[]
  exchanges?: string[]
  start_time: string
  end_time: string
  initial_equity?: number
  data_frequency?: string
  fee_taker_bps?: number
  fee_maker_bps?: number
  slippage_bps?: number
  max_single_position_pct?: number
  max_total_leverage?: number
  max_daily_loss_pct?: number
  min_signal_confidence?: number
  min_signal_strength?: number
}

export class BacktestApi {
  /// 查询回测任务列表
  static async listJobs() {
    const { data } = await api.get<{ jobs: BacktestJobSummary[] }>('/backtest/jobs')
    return data.jobs || []
  }

  /// 查询回测任务详情
  static async getJob(jobId: string) {
    const { data } = await api.get<BacktestJobDetail>(`/backtest/jobs/${jobId}`)
    return data
  }

  /// 创建回测任务
  static async createJob(req: CreateBacktestJobRequest) {
    const { data } = await api.post<{ job_id: string; status: string; job_name: string }>(
      '/backtest/jobs',
      req,
    )
    return data
  }

  /// 启动回测任务
  static async startJob(jobId: string) {
    const { data } = await api.post(`/backtest/jobs/${jobId}/start`)
    return data
  }

  /// 查询回测绩效报告
  static async getReport(jobId: string) {
    const { data } = await api.get<BacktestReport>(`/backtest/jobs/${jobId}/report`)
    return data
  }

  /// 查询回测可信等级
  static async getTrustLevel(jobId: string) {
    const { data } = await api.get<TrustLevelResponse>(`/backtest/jobs/${jobId}/trust-level`)
    return data
  }

  /// 触发回测可信等级评估
  static async assessTrustLevel(
    jobId: string,
    params?: {
      test_pass_rate?: number
      data_coverage_ratio?: number
      data_quality_grade?: string
      walk_forward_validated?: boolean
      calibration_healthy?: boolean
    },
  ) {
    const { data } = await api.post<TrustLevelResponse>(`/backtest/jobs/${jobId}/trust-level`, params || {})
    return data
  }

  /// 查询回测归因分析列表
  static async listAttributions(jobId: string) {
    const { data } = await api.get(`/backtest/jobs/${jobId}/attributions`)
    return data
  }

  /// 查询归因汇总
  static async getAttributionSummary(jobId: string) {
    const { data } = await api.get('/backtest/attributions/summary', { params: { job_id: jobId } })
    return data
  }

  /// 检测策略失效
  static async detectStrategyFailures(req: {
    job_id?: string
    symbol?: string
    lookback_days?: number
  }) {
    const { data } = await api.post('/backtest/strategy-failure/detect', req)
    return data
  }

  /// 查询策略失效告警列表
  static async listFailureAlerts(params?: {
    severity?: string
    status?: string
    limit?: number
  }) {
    const { data } = await api.get('/backtest/strategy-failure/alerts', { params })
    return data
  }

  /// 确认告警
  static async acknowledgeAlert(alertId: string) {
    const { data } = await api.post(`/backtest/strategy-failure/alerts/${alertId}/acknowledge`)
    return data
  }

  /// 解决告警
  static async resolveAlert(alertId: string) {
    const { data } = await api.post(`/backtest/strategy-failure/alerts/${alertId}/resolve`)
    return data
  }

  /// 生成 Walk-forward 窗口
  static async generateWalkForwardWindows(req: {
    train_window_days?: number
    test_window_days?: number
    step_days?: number
    purge_days?: number
    embargo_days?: number
    start_time: string
    end_time: string
  }) {
    const { data } = await api.post('/backtest/walk-forward/windows', req)
    return data
  }

  /// 组合风险检查
  static async checkPortfolioRisk(req: {
    assets: Array<{ symbol: string; position_pct: number; volatility: number; avg_daily_volume: number }>
    correlations?: Array<[string, string, number]>
    max_portfolio_cvar?: number
    max_risk_concentration?: number
    max_volume_participation?: number
    high_correlation_threshold?: number
    max_correlated_exposure?: number
  }) {
    const { data } = await api.post('/backtest/portfolio-risk/check', req)
    return data
  }

  /// 仓位计算（Fractional Kelly）
  static async calculatePosition(req: {
    entry_price: number
    win_probability: number
    avg_win: number
    avg_loss: number
    asset_volatility: number
    stop_loss_pct?: number
    kelly_fraction?: number
    volatility_target?: number
    max_risk_per_trade?: number
    max_position_pct?: number
    max_leverage?: number
    min_position_pct?: number
  }) {
    const { data } = await api.post('/backtest/position-sizing/calculate', req)
    return data
  }

  /// 查询特征血缘
  static async queryFeatureLineage(params: {
    feature_id: string
    symbol: string
    start_time?: string
    end_time?: string
  }) {
    const { data } = await api.get('/features/lineage', { params })
    return data
  }
}

// =========================================================
// 微结构数据 API（/microstructure/*）
// =========================================================

export interface OrderbookSnapshot {
  symbol: string
  exchange: string
  best_bid: number
  best_ask: number
  spread: number
  spread_bps: number
  mid_price: number
  depth_imbalance_5: number
  timestamp: string
}

export interface CvdResponse {
  symbol: string
  start_time: string
  end_time: string
  tick_count: number
  cvd: number
  buy_volume: number
  sell_volume: number
  buy_notional: number
  sell_notional: number
}

export interface LiquidationSummary {
  start_time: string
  end_time: string
  count: number
  total_notional: number
  long_liquidation_notional: number
  short_liquidation_notional: number
  events: unknown[]
}

export class MicrostructureApi {
  /// 获取最新订单簿快照
  static async getLatestOrderbook(symbol: string) {
    const { data } = await api.get<OrderbookSnapshot>(`/microstructure/orderbook/${symbol}`)
    return data
  }

  /// 查询订单簿历史
  static async listOrderbookHistory(symbol: string, limit = 50) {
    const { data } = await api.get(`/microstructure/orderbook/${symbol}/history`, {
      params: { limit },
    })
    return data
  }

  /// 查询逐笔成交
  static async listTrades(symbol: string, limit = 100) {
    const { data } = await api.get(`/microstructure/trades/${symbol}`, {
      params: { limit },
    })
    return data
  }

  /// 计算 CVD
  static async computeCvd(symbol: string, params?: {
    start_time?: string
    end_time?: string
    limit?: number
  }) {
    const { data } = await api.get<CvdResponse>(`/microstructure/trades/${symbol}/cvd`, {
      params,
    })
    return data
  }

  /// 查询清算事件
  static async listLiquidations(params?: {
    symbol?: string
    limit?: number
  }) {
    const { data } = await api.get<LiquidationSummary>('/microstructure/liquidations', {
      params,
    })
    return data
  }

  /// 查询基差数据
  static async listBasis(symbol: string, limit = 100) {
    const { data } = await api.get(`/microstructure/basis/${symbol}`, {
      params: { limit },
    })
    return data
  }
}

// =========================================================
// 数据质量监控 API（/data-quality/*）
// =========================================================

export interface QualityOverviewItem {
  symbol: string
  data_source: string
  snapshot_time: string
  freshness_sec: number | null
  gap_ratio: number
  outlier_ratio: number
  coverage_ratio: number
  quality_grade: string
  expected_points: number
  actual_points: number
}

export interface QualityAlert {
  symbol: string
  data_source: string
  snapshot_time: string
  freshness_sec: number | null
  gap_ratio: number
  coverage_ratio: number
  quality_grade: string
  severity: string
}

export class DataQualityApi {
  /// 获取质量概览
  static async getOverview() {
    const { data } = await api.get<{ total_sources: number; healthy: number; warning: number; critical: number; overview: QualityOverviewItem[] }>('/data-quality/overview')
    return data
  }

  /// 获取告警列表
  static async getAlerts() {
    const { data } = await api.get<{ total_alerts: number; critical: number; warning: number; alerts: QualityAlert[] }>('/data-quality/alerts/list')
    return data
  }

  /// 手动触发扫描
  static async triggerScan(req?: { lookback_hours?: number; symbols?: string[] }) {
    const { data } = await api.post('/data-quality/scan', req || {})
    return data
  }

  /// 获取数据源定义
  static async listSources() {
    const { data } = await api.get('/data-quality/sources')
    return data
  }
}

// =========================================================
// 模型卡 API（/model-cards/*）
// =========================================================

export interface ModelCardSummary {
  card_id: string
  model_version: string
  model_type: string
  model_name: string
  status: string
  promotion_eligible: boolean
  brier_score?: number
  log_loss?: number
  accuracy?: number
  calibration_report_id?: string
  trust_assessment_id?: string
  shadow_period_start?: string
  shadow_period_end?: string
  previous_version?: string
  created_at: string
  updated_at: string
}

export interface ModelCardDetail extends ModelCardSummary {
  description?: string
  intended_use?: string
  out_of_scope?: string
  training_data_summary?: unknown
  feature_version?: string
  features_used?: unknown
  calibration_curve?: unknown
  invalidation_conditions?: unknown
  known_limitations?: unknown
  ethical_considerations?: string
  created_by?: number
  approved_by?: number
  approved_at?: string
}

export class ModelCardApi {
  /// 查询模型卡列表
  static async listCards(params?: { status?: string; limit?: number }) {
    const { data } = await api.get<{ cards: ModelCardSummary[]; count: number }>('/model-cards', { params })
    return data
  }

  /// 查询单个模型卡详情
  static async getCard(modelVersion: string) {
    const { data } = await api.get<ModelCardDetail>(`/model-cards/${modelVersion}`)
    return data
  }

  /// 创建/聚合生成模型卡
  static async createCard(req: {
    model_version: string
    model_type: string
    model_name: string
    description?: string
    intended_use?: string
    out_of_scope?: string
    training_data_summary?: unknown
    feature_version?: string
    features_used?: unknown
    invalidation_conditions?: unknown
    known_limitations?: unknown
    ethical_considerations?: string
  }) {
    const { data } = await api.post('/model-cards', req)
    return data
  }

  /// 发布门禁：变更状态
  static async promoteCard(modelVersion: string, newStatus: string) {
    const { data } = await api.post(`/model-cards/${modelVersion}/promote`, { new_status: newStatus })
    return data
  }

  /// 回滚到之前的版本
  static async rollbackCard(modelVersion: string) {
    const { data } = await api.post(`/model-cards/${modelVersion}/rollback`)
    return data
  }
}

// =========================================================
// 反事实解释 API（/counterfactuals/*）
// =========================================================

export interface CounterfactualExplanation {
  explanation_id: string
  attribution_id?: string
  decision_card_id?: string
  job_id?: string
  user_id?: number
  symbol: string
  scenario_type: string
  scenario_description?: string
  counterfactual_pnl?: number
  actual_pnl: number
  pnl_delta?: number
  counterfactual_return?: number
  explanation: string
  key_drivers: unknown
  what_if_inputs?: unknown
  confidence: number
  evidence: unknown
  created_at: string
}

export class CounterfactualApi {
  /// 生成反事实解释
  static async generate(req: {
    attribution_id?: string
    decision_card_id?: string
    job_id?: string
    symbol: string
    direction: string
    actual_pnl: number
    gross_pnl: number
    entry_time: string
    exit_time?: string
    holding_period_sec?: number
    fee_cost: number
    slippage_cost: number
    funding_cost: number
    impact_cost: number
    benchmark_return?: number
    market_regime?: string
    signal_confidence?: number
    save?: boolean
  }) {
    const { data } = await api.post<{ explanations: CounterfactualExplanation[]; count: number; symbol: string; actual_pnl: number }>(
      '/counterfactuals/generate',
      req,
    )
    return data
  }

  /// 查询某笔交易的反事实场景
  static async listByAttribution(attributionId: string) {
    const { data } = await api.get<{ explanations: CounterfactualExplanation[]; count: number; attribution_id: string }>(
      `/counterfactuals/attribution/${attributionId}`,
    )
    return data
  }

  /// 查询某 job 下所有反事实场景
  static async listByJob(jobId: string) {
    const { data } = await api.get<{ explanations: CounterfactualExplanation[]; count: number; job_id: string }>(
      `/counterfactuals/job/${jobId}`,
    )
    return data
  }
}

export default api
