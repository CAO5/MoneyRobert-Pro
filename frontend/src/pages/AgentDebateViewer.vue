<script setup lang="ts">
import { ref, reactive, onMounted, onUnmounted, computed } from 'vue'
import {
  Play, RotateCcw, TrendingUp, TrendingDown, DollarSign,
  BarChart3, Activity, Target, Shield, Zap, Newspaper,
  Loader2, ChevronDown, History, CheckCircle2, AlertCircle,
  ChevronRight, Eye, Clock
} from 'lucide-vue-next'
import { useAuthStore } from '@/stores/auth'
import api from '@/api'

// === Agent Definitions ===
const AGENTS = [
  { id: 'tech_bull', name: '技术分析师A', department: 'technical', role: '看多', emoji: '📈', color: '#10B981' },
  { id: 'tech_bear', name: '技术分析师B', department: 'technical', role: '看空', emoji: '📉', color: '#EF4444' },
  { id: 'capital_bull', name: '资金分析师A', department: 'capital', role: '看多', emoji: '💰', color: '#10B981' },
  { id: 'capital_bear', name: '资金分析师B', department: 'capital', role: '看空', emoji: '💸', color: '#EF4444' },
  { id: 'news_bull', name: '新闻分析师A', department: 'news', role: '看多', emoji: '📰', color: '#10B981' },
  { id: 'news_bear', name: '新闻分析师B', department: 'news', role: '看空', emoji: '⚠️', color: '#EF4444' },
]

// === Types ===
interface AgentState {
  status: 'idle' | 'thinking' | 'speaking' | 'error'
  opinion: {
    agent_id: string
    agent_name: string
    department: string
    sentiment: string
    confidence: number
    analysis: string
    key_factors: string[]
  } | null
}

interface MarketData {
  price: number
  funding_rate: number
  long_short_ratio: number
  change_24h: number
}

interface DeptReport {
  department: string
  consensus: string
  bull_summary: string
  bear_summary: string
}

interface FundManagerDecision {
  action: string
  confidence: number
  entry_range: { low: number; high: number }
  stop_loss: number
  take_profit: number[]
  leverage: number
  reasoning: string
}

interface DebateHistoryItem {
  session_id: string
  symbol: string
  status: string
  created_at: string
}

// === State ===
const authStore = useAuthStore()
const popularSymbols = ['BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP', 'DOGE-USDT-SWAP', 'XRP-USDT-SWAP', 'ADA-USDT-SWAP', 'AVAX-USDT-SWAP', 'DOT-USDT-SWAP', 'LINK-USDT-SWAP', 'LTC-USDT-SWAP']
const intervals = ['1m', '5m', '15m', '30m', '1H', '4H', '1D']

const symbol = ref('BTC-USDT-SWAP')
const interval = ref('1H')
const sessionId = ref('')
const debateStatus = ref<'idle' | 'running' | 'completed' | 'error'>('idle')
const error = ref('')
const marketData = ref<MarketData | null>(null)
const departmentReports = ref<DeptReport[]>([])
const fundManagerDecision = ref<FundManagerDecision | null>(null)
const debateHistory = ref<DebateHistoryItem[]>([])
const historyLoading = ref(false)
const showHistory = ref(false)

// Agent states keyed by agent_id
const agentStates = reactive<Record<string, AgentState>>(
  Object.fromEntries(AGENTS.map(a => [a.id, { status: 'idle', opinion: null }]))
)

// === Computed ===
const isRunning = computed(() => debateStatus.value === 'running')
const isCompleted = computed(() => debateStatus.value === 'completed')

const technicalAgents = computed(() => AGENTS.filter(a => a.department === 'technical'))
const capitalAgents = computed(() => AGENTS.filter(a => a.department === 'capital'))
const newsAgents = computed(() => AGENTS.filter(a => a.department === 'news'))

const techReport = computed(() => departmentReports.value.find(r => r.department === 'technical'))
const capitalReport = computed(() => departmentReports.value.find(r => r.department === 'capital'))
const newsReport = computed(() => departmentReports.value.find(r => r.department === 'news'))

// === Helpers ===
function formatSymbol(s: string): string {
  return s.replace('-USDT-SWAP', '/USDT').replace('-USDT', '/USDT')
}

function getSentimentColor(s: string): string {
  if (s === 'bullish') return 'var(--profit)'
  if (s === 'bearish') return 'var(--loss)'
  if (s === 'cautious') return 'var(--warning)'
  return 'var(--text-muted)'
}

function getSentimentBg(s: string): string {
  if (s === 'bullish') return 'var(--profit-light)'
  if (s === 'bearish') return 'var(--loss-light)'
  if (s === 'cautious') return 'var(--warning-light)'
  return 'var(--surface-tertiary)'
}

function getSentimentText(s: string): string {
  if (s === 'bullish') return '看多'
  if (s === 'bearish') return '看空'
  if (s === 'cautious') return '谨慎'
  if (s === 'neutral') return '中性'
  return s
}

function getActionText(a: string): string {
  if (a === 'long') return '做多'
  if (a === 'short') return '做空'
  if (a === 'hold') return '观望'
  return a
}

function getActionColor(a: string): string {
  if (a === 'long') return 'var(--profit)'
  if (a === 'short') return 'var(--loss)'
  return 'var(--warning)'
}

function getActionBg(a: string): string {
  if (a === 'long') return 'var(--profit-light)'
  if (a === 'short') return 'var(--loss-light)'
  return 'var(--warning-light)'
}

function getConsensusIcon(c: string): string {
  if (c === 'bullish') return '📈'
  if (c === 'bearish') return '📉'
  return '➖'
}

function getDeptName(d: string): string {
  if (d === 'technical') return '技术分析部'
  if (d === 'capital') return '资金分析部'
  if (d === 'news') return '新闻分析部'
  return d
}

function getDeptIcon(d: string) {
  if (d === 'technical') return BarChart3
  if (d === 'capital') return DollarSign
  if (d === 'news') return Newspaper
  return Activity
}

function getDeptColor(d: string): string {
  if (d === 'technical') return 'var(--primary)'
  if (d === 'capital') return 'var(--warning)'
  if (d === 'news') return 'var(--info)'
  return 'var(--text-muted)'
}

function formatDateTime(s: string): string {
  return s ? new Date(s).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : ''
}

function getAvatarRingClass(agentId: string): string {
  const state = agentStates[agentId]
  if (!state) return 'ring-idle'
  if (state.status === 'thinking') return 'ring-thinking'
  if (state.status === 'speaking') return 'ring-speaking'
  if (state.status === 'error') return 'ring-error'
  return 'ring-idle'
}

function getAvatarGlowClass(agentId: string): string {
  const state = agentStates[agentId]
  if (!state || state.status !== 'speaking' || !state.opinion) return ''
  const sentiment = state.opinion.sentiment
  if (sentiment === 'bullish') return 'glow-bullish'
  if (sentiment === 'bearish') return 'glow-bearish'
  return ''
}

// === SSE Logic ===
function updateAgentStatus(agentId: string, status: AgentState['status']) {
  if (agentStates[agentId]) {
    agentStates[agentId].status = status
  }
}

function updateAgentOpinion(data: any) {
  const id = data.agent_id
  if (agentStates[id]) {
    agentStates[id].status = 'speaking'
    agentStates[id].opinion = data
  }
}

function handleSSEEvent(data: any) {
  switch (data.type) {
    case 'session_created':
      sessionId.value = data.session_id
      break
    case 'market_data':
      marketData.value = data
      break
    case 'agent_thinking':
      updateAgentStatus(data.agent_id, 'thinking')
      break
    case 'agent_opinion':
      updateAgentOpinion(data)
      break
    case 'dept_report':
      departmentReports.value.push(data)
      break
    case 'fund_manager':
      fundManagerDecision.value = data
      break
    case 'debate_complete':
      debateStatus.value = 'completed'
      loadHistory()
      break
  }
}

function resetState() {
  AGENTS.forEach(a => {
    agentStates[a.id].status = 'idle'
    agentStates[a.id].opinion = null
  })
  departmentReports.value = []
  fundManagerDecision.value = null
  marketData.value = null
  sessionId.value = ''
  error.value = ''
}

async function startDebate() {
  resetState()
  debateStatus.value = 'running'

  try {
    const response = await fetch('/api/v1/ai/debate', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${authStore.token}`,
      },
      body: JSON.stringify({ symbol: symbol.value, interval: interval.value }),
    })

    if (!response.ok) {
      let errMsg = '启动辩论失败'
      try {
        const err = await response.json()
        errMsg = err.message || errMsg
      } catch {}
      error.value = errMsg
      debateStatus.value = 'error'
      return
    }

    if (!response.body) {
      error.value = '浏览器不支持 SSE 流式传输'
      debateStatus.value = 'error'
      return
    }

    const reader = response.body.getReader()
    const decoder = new TextDecoder()
    let buffer = ''

    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      for (const line of lines) {
        if (line.startsWith('data:')) {
          try {
            const data = JSON.parse(line.slice(5).trim())
            handleSSEEvent(data)
          } catch (e) {
            console.warn('Failed to parse SSE data:', line, e)
          }
        }
      }
    }
  } catch (e: any) {
    error.value = e.message || '连接失败'
    debateStatus.value = 'error'
  }
}

// === History ===
async function loadHistory() {
  historyLoading.value = true
  try {
    const { data } = await api.get('/ai/debates')
    debateHistory.value = data.items || data.debates || data || []
  } catch (e) {
    console.error('Failed to load debate history', e)
  } finally {
    historyLoading.value = false
  }
}

async function loadDebateSession(sid: string) {
  try {
    const { data } = await api.get(`/ai/debate/${sid}`)
    // Populate state from historical data
    resetState()
    sessionId.value = data.session_id || sid
    symbol.value = data.symbol || symbol.value

    if (data.agent_opinions) {
      for (const op of data.agent_opinions) {
        if (agentStates[op.agent_id]) {
          agentStates[op.agent_id].status = 'speaking'
          agentStates[op.agent_id].opinion = op
        }
      }
    }
    if (data.department_reports) {
      departmentReports.value = data.department_reports
    }
    if (data.fund_manager_decision) {
      fundManagerDecision.value = data.fund_manager_decision
    }
    if (data.market_snapshot) {
      marketData.value = {
        price: data.market_snapshot.current_price || data.market_snapshot.price,
        funding_rate: data.market_snapshot.funding_rate,
        long_short_ratio: data.market_snapshot.long_short_ratio,
        change_24h: data.market_snapshot.change_24h,
      }
    }
    debateStatus.value = data.status === 'completed' ? 'completed' : 'idle'
    showHistory.value = false
  } catch (e: any) {
    console.error('Failed to load debate session', e)
  }
}

onMounted(() => loadHistory())
onUnmounted(() => {})
</script>

<template>
  <div class="debate-viewer">
    <!-- Header -->
    <div class="debate-header">
      <div>
        <h1 class="debate-title">AI 辩论分析</h1>
        <p class="debate-subtitle">多部门 AI Agent 实时协作分析市场</p>
      </div>
      <button @click="showHistory = !showHistory" class="btn btn-ghost btn-sm">
        <History class="w-4 h-4" />
        历史记录
      </button>
    </div>

    <!-- Control Panel -->
    <div class="card control-panel">
      <div class="control-row">
        <div class="control-field">
          <label class="label">交易对</label>
          <div class="select-wrap">
            <select v-model="symbol" class="input" :disabled="isRunning">
              <option v-for="s in popularSymbols" :key="s" :value="s">{{ formatSymbol(s) }}</option>
            </select>
            <ChevronDown class="select-icon" />
          </div>
        </div>
        <div class="control-field control-field-sm">
          <label class="label">周期</label>
          <div class="select-wrap">
            <select v-model="interval" class="input" :disabled="isRunning">
              <option v-for="iv in intervals" :key="iv" :value="iv">{{ iv }}</option>
            </select>
            <ChevronDown class="select-icon" />
          </div>
        </div>
        <div class="control-actions">
          <button v-if="isRunning" disabled class="btn btn-primary">
            <Loader2 class="w-4 h-4 animate-spin" />
            辩论进行中...
          </button>
          <button v-else-if="isCompleted" @click="startDebate" class="btn btn-primary">
            <RotateCcw class="w-4 h-4" />
            重新辩论
          </button>
          <button v-else @click="startDebate" class="btn btn-primary">
            <Play class="w-4 h-4" />
            开始辩论
          </button>
        </div>
      </div>

      <!-- Market Data Bar -->
      <div v-if="marketData" class="market-bar">
        <div class="market-item">
          <span class="market-label">价格</span>
          <span class="market-value font-mono">{{ marketData.price?.toFixed(6) }}</span>
        </div>
        <div class="market-divider"></div>
        <div class="market-item">
          <span class="market-label">24h涨跌</span>
          <span class="market-value font-mono" :style="{ color: marketData.change_24h >= 0 ? 'var(--profit)' : 'var(--loss)' }">
            {{ marketData.change_24h >= 0 ? '+' : '' }}{{ (marketData.change_24h * 100).toFixed(2) }}%
          </span>
        </div>
        <div class="market-divider"></div>
        <div class="market-item">
          <span class="market-label">资金费率</span>
          <span class="market-value font-mono" :style="{ color: marketData.funding_rate >= 0 ? 'var(--profit)' : 'var(--loss)' }">
            {{ (marketData.funding_rate * 100).toFixed(4) }}%
          </span>
        </div>
        <div class="market-divider"></div>
        <div class="market-item">
          <span class="market-label">多空比</span>
          <span class="market-value font-mono">{{ marketData.long_short_ratio?.toFixed(4) }}</span>
        </div>
      </div>
    </div>

    <!-- Error Banner -->
    <div v-if="error" class="error-banner">
      <AlertCircle class="w-4 h-4" />
      <span>{{ error }}</span>
    </div>

    <!-- History Panel (overlay) -->
    <div v-if="showHistory" class="history-panel card">
      <div class="history-header">
        <h3 class="history-title">历史辩论</h3>
        <button @click="showHistory = false" class="btn btn-ghost btn-sm">✕</button>
      </div>
      <div v-if="historyLoading" class="history-loading">
        <div v-for="i in 3" :key="i" class="history-skeleton"></div>
      </div>
      <div v-else-if="!debateHistory.length" class="history-empty">暂无历史记录</div>
      <div v-else class="history-list">
        <button v-for="item in debateHistory" :key="item.session_id" @click="loadDebateSession(item.session_id)" class="history-item" :class="{ active: item.session_id === sessionId }">
          <span class="history-symbol">{{ formatSymbol(item.symbol) }}</span>
          <span class="badge" :class="item.status === 'completed' ? 'badge-profit' : 'badge-warning'">{{ item.status === 'completed' ? '已完成' : '进行中' }}</span>
          <span class="history-time">{{ formatDateTime(item.created_at) }}</span>
        </button>
      </div>
    </div>

    <!-- Agent Arena -->
    <div class="agent-arena">
      <div class="arena-title">
        <Activity class="w-5 h-5" style="color: var(--primary)" />
        <h2>Agent 竞技场</h2>
        <span v-if="isRunning" class="badge badge-info">
          <Loader2 class="w-3 h-3 animate-spin" />
          实时辩论中
        </span>
        <span v-else-if="isCompleted" class="badge badge-profit">辩论完成</span>
      </div>

      <div class="agent-grid">
        <div v-for="agent in AGENTS" :key="agent.id" class="agent-card" :class="[getAvatarRingClass(agent.id), getAvatarGlowClass(agent.id)]">
          <!-- Avatar -->
          <div class="agent-avatar-wrap">
            <div class="agent-avatar" :class="getAvatarRingClass(agent.id)">
              <span class="agent-emoji">{{ agent.emoji }}</span>
            </div>
            <div v-if="agentStates[agent.id]?.status === 'thinking'" class="thinking-indicator">
              <Loader2 class="w-3 h-3 animate-spin" style="color: var(--primary)" />
            </div>
            <div v-if="agentStates[agent.id]?.status === 'error'" class="error-indicator">✕</div>
          </div>

          <!-- Name & Role -->
          <div class="agent-identity">
            <span class="agent-name">{{ agent.name }}</span>
            <span class="agent-role-badge" :style="{ background: agent.color + '15', color: agent.color }">{{ agent.role }}</span>
          </div>

          <!-- Department -->
          <div class="agent-dept">
            <component :is="getDeptIcon(agent.department)" class="w-3 h-3" :style="{ color: getDeptColor(agent.department) }" />
            <span>{{ getDeptName(agent.department) }}</span>
          </div>

          <!-- Opinion Content -->
          <template v-if="agentStates[agent.id]?.opinion">
            <div class="opinion-content">
              <!-- Sentiment Badge -->
              <div class="sentiment-row">
                <span class="sentiment-badge" :style="{ background: getSentimentBg(agentStates[agent.id].opinion.sentiment), color: getSentimentColor(agentStates[agent.id].opinion.sentiment) }">
                  {{ getSentimentText(agentStates[agent.id].opinion.sentiment) }}
                </span>
                <span class="confidence-text">{{ (agentStates[agent.id].opinion.confidence * 100).toFixed(0) }}%</span>
              </div>

              <!-- Confidence Bar -->
              <div class="confidence-bar-wrap">
                <div class="confidence-bar" :style="{ width: (agentStates[agent.id].opinion.confidence * 100) + '%', background: getSentimentColor(agentStates[agent.id].opinion.sentiment) }"></div>
              </div>

              <!-- Analysis -->
              <p class="analysis-text">{{ agentStates[agent.id].opinion.analysis }}</p>

              <!-- Key Factors -->
              <div v-if="agentStates[agent.id].opinion.key_factors?.length" class="factors-wrap">
                <span v-for="(f, idx) in agentStates[agent.id].opinion.key_factors" :key="f" class="factor-tag" :style="{ animationDelay: (idx * 100) + 'ms' }">
                  {{ f }}
                </span>
              </div>
            </div>
          </template>

          <!-- Idle / Thinking Placeholder -->
          <div v-else-if="agentStates[agent.id]?.status === 'thinking'" class="thinking-placeholder">
            <div class="thinking-dots">
              <span></span><span></span><span></span>
            </div>
            <span class="thinking-text">正在分析...</span>
          </div>

          <div v-else class="idle-placeholder">
            <Clock class="w-4 h-4" style="color: var(--text-muted); opacity: 0.4" />
            <span>等待辩论</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Department Reports -->
    <div v-if="departmentReports.length" class="dept-section">
      <div class="arena-title">
        <BarChart3 class="w-5 h-5" style="color: var(--primary)" />
        <h2>部门共识报告</h2>
      </div>

      <div class="dept-grid">
        <div v-for="report in departmentReports" :key="report.department" class="dept-card">
          <div class="dept-card-header">
            <div class="dept-card-icon" :style="{ background: getDeptColor(report.department) + '15' }">
              <component :is="getDeptIcon(report.department)" class="w-5 h-5" :style="{ color: getDeptColor(report.department) }" />
            </div>
            <span class="dept-card-name">{{ getDeptName(report.department) }}</span>
            <span class="consensus-badge" :style="{ background: getSentimentBg(report.consensus), color: getSentimentColor(report.consensus) }">
              {{ getConsensusIcon(report.consensus) }} {{ getSentimentText(report.consensus) }}
            </span>
          </div>

          <div class="dept-summaries">
            <div class="dept-summary bull-summary">
              <div class="summary-header">
                <TrendingUp class="w-4 h-4" style="color: var(--profit)" />
                <span>多头观点</span>
              </div>
              <p>{{ report.bull_summary }}</p>
            </div>
            <div class="dept-summary bear-summary">
              <div class="summary-header">
                <TrendingDown class="w-4 h-4" style="color: var(--loss)" />
                <span>空头观点</span>
              </div>
              <p>{{ report.bear_summary }}</p>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Fund Manager Decision -->
    <div v-if="fundManagerDecision" class="fund-section">
      <div class="arena-title">
        <Shield class="w-5 h-5" style="color: var(--primary)" />
        <h2>基金经理决策</h2>
      </div>

      <div class="fund-card">
        <div class="fund-header">
          <div class="fund-manager-icon">
            <Target class="w-6 h-6" style="color: white" />
          </div>
          <div class="fund-manager-info">
            <span class="fund-manager-name">基金经理</span>
            <span class="fund-manager-sub">综合决策</span>
          </div>
          <div class="fund-action-badge" :style="{ background: getActionBg(fundManagerDecision.action), color: getActionColor(fundManagerDecision.action) }">
            <Zap class="w-5 h-5" />
            <span>{{ getActionText(fundManagerDecision.action) }}</span>
            <span class="fund-confidence">{{ (fundManagerDecision.confidence * 100).toFixed(0) }}%</span>
          </div>
        </div>

        <div class="fund-stats">
          <div class="stat-card">
            <div class="stat-label">入场区间</div>
            <div class="stat-value text-lg">{{ fundManagerDecision.entry_range?.low }} - {{ fundManagerDecision.entry_range?.high }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">止损</div>
            <div class="stat-value text-lg" style="color: var(--loss)">{{ fundManagerDecision.stop_loss }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">止盈</div>
            <div class="stat-value text-lg" style="color: var(--profit)">{{ fundManagerDecision.take_profit?.join(', ') }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">杠杆</div>
            <div class="stat-value text-lg">{{ fundManagerDecision.leverage }}x</div>
          </div>
        </div>

        <div class="fund-reasoning">
          <div class="reasoning-label">决策逻辑</div>
          <p class="reasoning-text">{{ fundManagerDecision.reasoning }}</p>
        </div>
      </div>
    </div>

    <!-- Empty State -->
    <div v-if="debateStatus === 'idle' && !fundManagerDecision && !departmentReports.length" class="empty-state card">
      <div class="empty-emoji">🤖</div>
      <h3>选择交易对并开始辩论</h3>
      <p>AI Agent 将从技术、资金、新闻多维度实时分析市场</p>
    </div>
  </div>
</template>

<style scoped>
/* === Layout === */
.debate-viewer {
  display: flex;
  flex-direction: column;
  gap: 24px;
  animation: fadeIn 0.3s ease-out;
}

.debate-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.debate-title {
  font-size: 24px;
  font-weight: 700;
  color: var(--text-primary);
}

.debate-subtitle {
  font-size: 14px;
  color: var(--text-secondary);
  margin-top: 4px;
}

/* === Control Panel === */
.control-panel {
  padding: 20px;
}

.control-row {
  display: flex;
  align-items: flex-end;
  gap: 16px;
  flex-wrap: wrap;
}

.control-field {
  flex: 1;
  min-width: 160px;
}

.control-field-sm {
  min-width: 100px;
  flex: 0 0 120px;
}

.select-wrap {
  position: relative;
}

.select-icon {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%);
  width: 16px;
  height: 16px;
  color: var(--text-muted);
  pointer-events: none;
}

.control-actions {
  flex-shrink: 0;
}

/* === Market Bar === */
.market-bar {
  display: flex;
  align-items: center;
  gap: 20px;
  margin-top: 16px;
  padding: 12px 16px;
  background: var(--surface-secondary);
  border-radius: var(--radius-md);
  flex-wrap: wrap;
}

.market-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.market-label {
  font-size: 12px;
  color: var(--text-muted);
}

.market-value {
  font-size: 16px;
  font-weight: 700;
  color: var(--text-primary);
}

.market-divider {
  width: 1px;
  height: 32px;
  background: var(--border);
}

/* === Error Banner === */
.error-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  background: var(--loss-light);
  color: var(--loss);
  border-radius: var(--radius-md);
  font-size: 14px;
  font-weight: 500;
}

/* === History Panel === */
.history-panel {
  padding: 16px;
  max-height: 400px;
  overflow-y: auto;
}

.history-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.history-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.history-loading {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.history-skeleton {
  height: 48px;
  border-radius: var(--radius-md);
  background: var(--surface-tertiary);
  animation: pulse 2s ease-in-out infinite;
}

.history-empty {
  text-align: center;
  padding: 16px;
  color: var(--text-muted);
  font-size: 14px;
}

.history-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.history-item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  background: var(--surface-tertiary);
  text-align: left;
  transition: all var(--transition-fast) ease;
  border: 1px solid transparent;
}

.history-item:hover {
  background: var(--primary-bg);
  border-color: var(--primary);
}

.history-item.active {
  background: var(--primary-bg);
  border-color: var(--primary);
}

.history-symbol {
  font-weight: 600;
  font-size: 14px;
  color: var(--text-primary);
}

.history-time {
  font-size: 12px;
  color: var(--text-muted);
  margin-left: auto;
}

/* === Arena Title === */
.arena-title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 16px;
}

.arena-title h2 {
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
}

/* === Agent Arena === */
.agent-arena {
  /* no card wrapper, agents have individual cards */
}

.agent-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
}

@media (max-width: 1024px) {
  .agent-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 640px) {
  .agent-grid {
    grid-template-columns: 1fr;
  }
}

/* === Agent Card === */
.agent-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  transition: all var(--transition-base) ease;
  position: relative;
  overflow: hidden;
}

.agent-card:hover {
  border-color: var(--border-hover);
  box-shadow: var(--shadow-card-hover);
}

.agent-card.ring-thinking {
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-bg);
}

.agent-card.glow-bullish {
  animation: glow-bullish 2s ease-in-out infinite;
}

.agent-card.glow-bearish {
  animation: glow-bearish 2s ease-in-out infinite;
}

/* === Agent Avatar === */
.agent-avatar-wrap {
  position: relative;
  margin-bottom: 4px;
}

.agent-avatar {
  width: 64px;
  height: 64px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface-secondary);
  border: 3px solid var(--border);
  transition: all var(--transition-base) ease;
}

.agent-avatar.ring-idle {
  border-color: var(--border);
}

.agent-avatar.ring-thinking {
  border-color: var(--primary);
  animation: pulse-think 1.5s ease-in-out infinite;
}

.agent-avatar.ring-speaking {
  border-color: var(--profit);
}

.agent-avatar.ring-error {
  border-color: var(--loss);
}

.agent-emoji {
  font-size: 28px;
  line-height: 1;
}

.thinking-indicator {
  position: absolute;
  bottom: -2px;
  right: -2px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--surface);
  display: flex;
  align-items: center;
  justify-content: center;
  border: 2px solid var(--primary);
}

.error-indicator {
  position: absolute;
  top: -4px;
  right: -4px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--loss);
  color: white;
  font-size: 12px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* === Agent Identity === */
.agent-identity {
  display: flex;
  align-items: center;
  gap: 6px;
}

.agent-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.agent-role-badge {
  display: inline-flex;
  padding: 2px 8px;
  border-radius: var(--radius-full);
  font-size: 11px;
  font-weight: 500;
}

.agent-dept {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-muted);
}

/* === Opinion Content === */
.opinion-content {
  width: 100%;
  animation: opinion-arrive 0.4s ease-out forwards;
}

.sentiment-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}

.sentiment-badge {
  display: inline-flex;
  padding: 3px 10px;
  border-radius: var(--radius-full);
  font-size: 12px;
  font-weight: 600;
}

.confidence-text {
  font-size: 13px;
  font-weight: 700;
  color: var(--text-secondary);
  font-family: var(--font-mono);
}

.confidence-bar-wrap {
  width: 100%;
  height: 4px;
  border-radius: 2px;
  background: var(--surface-tertiary);
  overflow: hidden;
  margin-bottom: 10px;
}

.confidence-bar {
  height: 100%;
  border-radius: 2px;
  animation: confidence-fill 0.8s ease-out forwards;
}

.analysis-text {
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-secondary);
  margin-bottom: 8px;
}

.factors-wrap {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.factor-tag {
  display: inline-flex;
  padding: 2px 8px;
  border-radius: var(--radius-full);
  font-size: 11px;
  font-weight: 500;
  background: var(--surface-tertiary);
  color: var(--text-secondary);
  animation: factor-pop 0.3s ease-out forwards;
  opacity: 0;
}

/* === Thinking Placeholder === */
.thinking-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 12px 0;
}

.thinking-dots {
  display: flex;
  gap: 4px;
}

.thinking-dots span {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--primary);
  animation: thinking-bounce 1.4s ease-in-out infinite;
}

.thinking-dots span:nth-child(2) {
  animation-delay: 0.2s;
}

.thinking-dots span:nth-child(3) {
  animation-delay: 0.4s;
}

.thinking-text {
  font-size: 12px;
  color: var(--primary);
  font-weight: 500;
}

/* === Idle Placeholder === */
.idle-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 12px 0;
  color: var(--text-muted);
  font-size: 12px;
  opacity: 0.6;
}

/* === Department Section === */
.dept-section {
  margin-top: 8px;
}

.dept-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
}

@media (max-width: 768px) {
  .dept-grid {
    grid-template-columns: 1fr;
  }
}

.dept-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 20px;
  animation: opinion-arrive 0.4s ease-out forwards;
}

.dept-card-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 16px;
}

.dept-card-icon {
  width: 40px;
  height: 40px;
  border-radius: var(--radius-md);
  display: flex;
  align-items: center;
  justify-content: center;
}

.dept-card-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  flex: 1;
}

.consensus-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 12px;
  border-radius: var(--radius-full);
  font-size: 13px;
  font-weight: 600;
}

.dept-summaries {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.dept-summary {
  padding: 12px;
  border-radius: var(--radius-md);
}

.dept-summary.bull-summary {
  background: var(--profit-light);
}

.dept-summary.bear-summary {
  background: var(--loss-light);
}

.summary-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 6px;
}

.dept-summary.bull-summary .summary-header {
  color: var(--profit);
}

.dept-summary.bear-summary .summary-header {
  color: var(--loss);
}

.dept-summary p {
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-secondary);
}

/* === Fund Manager Section === */
.fund-section {
  margin-top: 8px;
}

.fund-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-xl);
  padding: 24px;
  animation: opinion-arrive 0.5s ease-out forwards;
}

.fund-header {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 20px;
  flex-wrap: wrap;
}

.fund-manager-icon {
  width: 48px;
  height: 48px;
  border-radius: var(--radius-lg);
  background: var(--primary);
  display: flex;
  align-items: center;
  justify-content: center;
}

.fund-manager-info {
  display: flex;
  flex-direction: column;
}

.fund-manager-name {
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
}

.fund-manager-sub {
  font-size: 13px;
  color: var(--text-muted);
}

.fund-action-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 20px;
  border-radius: var(--radius-lg);
  font-size: 18px;
  font-weight: 700;
  margin-left: auto;
}

.fund-confidence {
  font-size: 14px;
  font-family: var(--font-mono);
  opacity: 0.8;
}

.fund-stats {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
  margin-bottom: 20px;
}

@media (max-width: 768px) {
  .fund-stats {
    grid-template-columns: repeat(2, 1fr);
  }
}

.fund-reasoning {
  padding: 16px;
  background: var(--surface-tertiary);
  border-radius: var(--radius-md);
}

.reasoning-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}

.reasoning-text {
  font-size: 14px;
  line-height: 1.7;
  color: var(--text-secondary);
}

/* === Empty State === */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 64px 24px;
  text-align: center;
}

.empty-emoji {
  font-size: 48px;
  margin-bottom: 16px;
}

.empty-state h3 {
  font-size: 18px;
  font-weight: 600;
  color: var(--text-muted);
  margin-bottom: 8px;
}

.empty-state p {
  font-size: 14px;
  color: var(--text-muted);
  opacity: 0.7;
}

/* === Keyframe Animations === */
@keyframes pulse-think {
  0%, 100% { box-shadow: 0 0 0 0 rgba(37, 99, 235, 0.4); }
  50% { box-shadow: 0 0 0 12px rgba(37, 99, 235, 0); }
}

@keyframes opinion-arrive {
  from { opacity: 0; transform: translateY(10px); }
  to { opacity: 1; transform: translateY(0); }
}

@keyframes confidence-fill {
  from { width: 0; }
}

@keyframes glow-bullish {
  0%, 100% { box-shadow: 0 0 8px rgba(16, 185, 129, 0.3); }
  50% { box-shadow: 0 0 20px rgba(16, 185, 129, 0.6); }
}

@keyframes glow-bearish {
  0%, 100% { box-shadow: 0 0 8px rgba(239, 68, 68, 0.3); }
  50% { box-shadow: 0 0 20px rgba(239, 68, 68, 0.6); }
}

@keyframes factor-pop {
  from { opacity: 0; transform: scale(0.8); }
  to { opacity: 1; transform: scale(1); }
}

@keyframes thinking-bounce {
  0%, 80%, 100% { transform: scale(0); opacity: 0.5; }
  40% { transform: scale(1); opacity: 1; }
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}
</style>
