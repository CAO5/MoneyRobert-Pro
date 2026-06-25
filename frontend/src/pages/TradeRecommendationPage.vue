<template>
  <div class="trade-recommendation-page">
    <div class="page-header">
      <h1>交易建议卡片</h1>
      <p class="subtitle">基于 EV/CVaR/风控/信任等级的综合交易建议</p>
    </div>

    <!-- 输入表单 -->
    <div class="card input-card">
      <h2>信号参数</h2>
      <div class="form-grid">
        <div class="form-field">
          <label>交易对</label>
          <select v-model="form.symbol">
            <option v-for="s in symbols" :key="s" :value="s">{{ s }}</option>
          </select>
        </div>
        <div class="form-field">
          <label>方向</label>
          <select v-model="form.direction">
            <option value="long">做多</option>
            <option value="short">做空</option>
            <option value="hold">观望</option>
          </select>
        </div>
        <div class="form-field">
          <label>置信度 ({{ (form.confidence * 100).toFixed(0) }}%)</label>
          <input type="range" min="0" max="1" step="0.05" v-model.number="form.confidence" />
        </div>
        <div class="form-field">
          <label>信号强度 ({{ (form.signal_strength * 100).toFixed(0) }}%)</label>
          <input type="range" min="0" max="1" step="0.05" v-model.number="form.signal_strength" />
        </div>
        <div class="form-field">
          <label>当前价格</label>
          <input type="number" step="0.01" v-model.number="form.current_price" />
        </div>
        <div class="form-field">
          <label>预期收益 (bps)</label>
          <input type="number" step="1" v-model.number="form.expected_return_bps" />
        </div>
        <div class="form-field">
          <label>账户权益 (USDT)</label>
          <input type="number" step="1000" v-model.number="form.total_equity" />
        </div>
        <div class="form-field">
          <label>资产波动率 ({{ ((form.asset_volatility || 0.6) * 100).toFixed(0) }}%)</label>
          <input type="range" min="0.1" max="2.0" step="0.1" v-model.number="form.asset_volatility" />
        </div>
        <div class="form-field">
          <label>市场状态</label>
          <select v-model="form.market_regime">
            <option :value="undefined">自动</option>
            <option value="trending_bull">趋势上涨</option>
            <option value="trending_bear">趋势下跌</option>
            <option value="ranging">震荡</option>
            <option value="high_volatility">高波动</option>
            <option value="crisis">危机</option>
          </select>
        </div>
      </div>
      <button class="btn-primary" @click="generate" :disabled="loading">
        {{ loading ? '计算中...' : '生成建议' }}
      </button>
    </div>

    <!-- 建议结果 -->
    <div v-if="result" class="card result-card" :class="trustClass">
      <!-- 头部：动作 + 信任等级 -->
      <div class="result-header">
        <div class="action-section">
          <span class="action-badge" :class="actionClass">
            {{ actionLabel }}
          </span>
          <span v-if="result.executable" class="executable-tag">可执行</span>
          <span v-else class="blocked-tag">不可执行</span>
        </div>
        <div class="trust-badge" :class="trustClass">
          信任等级 {{ result.trust_level }}
        </div>
      </div>

      <!-- 核心指标 -->
      <div class="metrics-grid">
        <div class="metric">
          <span class="metric-label">置信度</span>
          <span class="metric-value">{{ (result.confidence * 100).toFixed(1) }}%</span>
        </div>
        <div class="metric">
          <span class="metric-label">期望价值 (EV)</span>
          <span class="metric-value" :class="{ positive: result.expected_value > 0, negative: result.expected_value < 0 }">
            {{ (result.expected_value * 100).toFixed(3) }}%
          </span>
        </div>
        <div class="metric">
          <span class="metric-label">CVaR (95%)</span>
          <span class="metric-value negative">{{ (result.cvar * 100).toFixed(2) }}%</span>
        </div>
        <div class="metric">
          <span class="metric-label">建议仓位</span>
          <span class="metric-value">{{ (result.position_pct * 100).toFixed(2) }}%</span>
        </div>
        <div class="metric">
          <span class="metric-label">建议名义</span>
          <span class="metric-value">{{ result.suggested_notional.toFixed(2) }} USDT</span>
        </div>
        <div class="metric">
          <span class="metric-label">止损价</span>
          <span class="metric-value">{{ result.stop_loss_price?.toFixed(2) || '-' }}</span>
        </div>
        <div class="metric">
          <span class="metric-label">止盈价</span>
          <span class="metric-value">{{ result.take_profit_price?.toFixed(2) || '-' }}</span>
        </div>
      </div>

      <!-- 阻断原因 -->
      <div v-if="result.blockers.length > 0" class="section blockers-section">
        <h3>不能交易的原因</h3>
        <ul class="blocker-list">
          <li v-for="(b, i) in result.blockers" :key="i">{{ b }}</li>
        </ul>
      </div>

      <!-- 主要理由 -->
      <div v-if="result.reasons.length > 0" class="section">
        <h3>主要理由</h3>
        <ul class="reason-list">
          <li v-for="(r, i) in result.reasons" :key="i">{{ r }}</li>
        </ul>
      </div>

      <!-- 主要风险 -->
      <div v-if="result.risks.length > 0" class="section">
        <h3>主要风险</h3>
        <ul class="risk-list">
          <li v-for="(r, i) in result.risks" :key="i">{{ r }}</li>
        </ul>
      </div>

      <!-- 决策流水线 -->
      <div class="section">
        <h3>决策流水线 (trace: {{ result.trace_id.substring(0, 8) }}...)</h3>
        <div class="pipeline">
          <div v-for="(step, i) in result.pipeline_steps" :key="i" class="pipeline-step">
            {{ step }}
          </div>
        </div>
      </div>
    </div>

    <div v-if="error" class="card error-card">
      <p>{{ error }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { SignalApi, type TradeRecommendationRequest, type TradeRecommendationResponse } from '@/api'

const symbols = [
  'BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP', 'BNB-USDT-SWAP',
  'XRP-USDT-SWAP', 'ADA-USDT-SWAP', 'AVAX-USDT-SWAP', 'LINK-USDT-SWAP',
  'ARB-USDT-SWAP', 'OP-USDT-SWAP', 'APT-USDT-SWAP', 'NEAR-USDT-SWAP',
]

const form = ref<TradeRecommendationRequest>({
  symbol: 'BTC-USDT-SWAP',
  direction: 'long',
  confidence: 0.6,
  signal_strength: 0.5,
  expected_return_bps: 100,
  current_price: 60000,
  asset_volatility: 0.6,
  total_equity: 100000,
  market_regime: undefined,
})

const result = ref<TradeRecommendationResponse | null>(null)
const loading = ref(false)
const error = ref('')

const generate = async () => {
  loading.value = true
  error.value = ''
  result.value = null
  try {
    result.value = await SignalApi.generateTradeRecommendation(form.value)
  } catch (e: any) {
    error.value = e?.response?.data?.detail || e?.message || '请求失败'
  } finally {
    loading.value = false
  }
}

const actionLabel = computed(() => {
  if (!result.value) return ''
  const map: Record<string, string> = {
    open_long: '做多',
    open_short: '做空',
    hold: '观望',
  }
  return map[result.value.action] || result.value.action
})

const actionClass = computed(() => {
  if (!result.value) return ''
  if (result.value.action === 'open_long') return 'action-long'
  if (result.value.action === 'open_short') return 'action-short'
  return 'action-hold'
})

const trustClass = computed(() => {
  if (!result.value) return ''
  return `trust-${result.value.trust_level.toLowerCase()}`
})
</script>

<style scoped>
.trade-recommendation-page {
  padding: 24px;
  max-width: 900px;
  margin: 0 auto;
}

.page-header {
  margin-bottom: 24px;
}

.page-header h1 {
  font-size: 1.8rem;
  margin: 0 0 4px 0;
}

.subtitle {
  color: var(--text-secondary, #888);
  font-size: 0.9rem;
}

.card {
  background: var(--bg-card, #1a1a2e);
  border-radius: 12px;
  padding: 20px;
  margin-bottom: 16px;
  border: 1px solid var(--border-color, #333);
}

.input-card h2 {
  margin: 0 0 16px 0;
  font-size: 1.1rem;
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 12px;
  margin-bottom: 16px;
}

.form-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.form-field label {
  font-size: 0.8rem;
  color: var(--text-secondary, #888);
}

.form-field input,
.form-field select {
  padding: 8px 10px;
  border-radius: 6px;
  border: 1px solid var(--border-color, #444);
  background: var(--bg-input, #0f0f1a);
  color: var(--text-primary, #eee);
  font-size: 0.9rem;
}

.btn-primary {
  padding: 10px 24px;
  border-radius: 8px;
  border: none;
  background: var(--accent, #4f46e5);
  color: white;
  font-size: 0.95rem;
  cursor: pointer;
  transition: opacity 0.2s;
}

.btn-primary:hover:not(:disabled) {
  opacity: 0.85;
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 结果卡片 */
.result-card {
  border-width: 2px;
}

.trust-a { border-color: #22c55e; }
.trust-b { border-color: #3b82f6; }
.trust-c { border-color: #eab308; }
.trust-d { border-color: #ef4444; }

.result-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.action-section {
  display: flex;
  align-items: center;
  gap: 8px;
}

.action-badge {
  padding: 6px 16px;
  border-radius: 6px;
  font-weight: 600;
  font-size: 1rem;
}

.action-long { background: #22c55e20; color: #22c55e; }
.action-short { background: #ef444420; color: #ef4444; }
.action-hold { background: #6b728020; color: #6b7280; }

.executable-tag {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 0.75rem;
  background: #22c55e20;
  color: #22c55e;
}

.blocked-tag {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 0.75rem;
  background: #ef444420;
  color: #ef4444;
}

.trust-badge {
  padding: 6px 14px;
  border-radius: 6px;
  font-weight: 600;
  font-size: 0.9rem;
}

.trust-badge.trust-a { background: #22c55e20; color: #22c55e; }
.trust-badge.trust-b { background: #3b82f620; color: #3b82f6; }
.trust-badge.trust-c { background: #eab30820; color: #eab308; }
.trust-badge.trust-d { background: #ef444420; color: #ef4444; }

/* 指标网格 */
.metrics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  gap: 12px;
  margin-bottom: 20px;
}

.metric {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px;
  background: var(--bg-metric, #0f0f1a);
  border-radius: 8px;
}

.metric-label {
  font-size: 0.75rem;
  color: var(--text-secondary, #888);
}

.metric-value {
  font-size: 1.05rem;
  font-weight: 600;
}

.positive { color: #22c55e; }
.negative { color: #ef4444; }

/* 区块 */
.section {
  margin-top: 16px;
}

.section h3 {
  font-size: 0.9rem;
  margin: 0 0 8px 0;
  color: var(--text-secondary, #aaa);
}

.section ul {
  list-style: none;
  padding: 0;
  margin: 0;
}

.section li {
  padding: 4px 0;
  font-size: 0.88rem;
  padding-left: 16px;
  position: relative;
}

.section li::before {
  content: '•';
  position: absolute;
  left: 0;
}

.blockers-section li::before { color: #ef4444; }
.reason-list li::before { color: #3b82f6; }
.risk-list li::before { color: #eab308; }

/* 流水线 */
.pipeline {
  background: var(--bg-metric, #0f0f1a);
  border-radius: 8px;
  padding: 12px;
  max-height: 200px;
  overflow-y: auto;
}

.pipeline-step {
  font-family: monospace;
  font-size: 0.78rem;
  padding: 2px 0;
  color: var(--text-secondary, #aaa);
  white-space: pre-wrap;
  word-break: break-all;
}

.error-card {
  border-color: #ef4444;
  color: #ef4444;
}
</style>
