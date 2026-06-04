<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import {
  Bot,
  Brain,
  TrendingUp,
  TrendingDown,
  DollarSign,
  Newspaper,
  User,
  MessageSquare,
  Clock,
  CheckCircle2,
  AlertCircle,
  RefreshCw,
  ChevronRight,
  Loader2,
  Play,
  History
} from 'lucide-vue-next'
import api from '@/api'

interface AgentOpinion {
  agent_id: string
  agent_name: string
  department: string
  sentiment: string
  confidence: number
  analysis: string
  key_factors: string[]
  source?: string
}

interface DepartmentReport {
  department: string
  consensus: {
    overall_bias: string
    confidence: number
  }
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
  reasoning: {
    primary_thesis: string
    key_risks: string[]
  }
}

interface DebateResult {
  session_id: string
  symbol: string
  status: string
  progress: number
  agent_opinions: AgentOpinion[]
  department_reports: DepartmentReport[]
  fund_manager_decision: FundManagerDecision | null
  market_snapshot: Record<string, any>
  source?: string
}

interface DebateHistoryItem {
  session_id: string
  symbol: string
  status: string
  progress: number
  created_at: string
  source?: string
}

const popularSymbols = [
  'BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP',
  'DOGE-USDT-SWAP', 'XRP-USDT-SWAP', 'ADA-USDT-SWAP',
  'AVAX-USDT-SWAP', 'DOT-USDT-SWAP', 'LINK-USDT-SWAP',
  'LTC-USDT-SWAP', 'UNI-USDT-SWAP', 'ATOM-USDT-SWAP',
  'ARB-USDT-SWAP', 'OP-USDT-SWAP', 'NEAR-USDT-SWAP',
  'SUI-USDT-SWAP', 'PEPE-USDT-SWAP', 'FIL-USDT-SWAP',
  'APT-USDT-SWAP',
]

const intervals = ['1m', '5m', '15m', '30m', '1H', '4H', '1D']

const selectedSymbol = ref('BTC-USDT-SWAP')
const selectedInterval = ref('1H')

const debateResult = ref<DebateResult | null>(null)
const debateHistory = ref<DebateHistoryItem[]>([])
const activeTab = ref<'agents' | 'departments' | 'decision'>('agents')
const loading = ref(false)
const historyLoading = ref(false)
const debateRunning = ref(false)
let pollTimer: ReturnType<typeof setInterval> | null = null

const DEPARTMENT_MAP: Record<string, { name: string; icon: any; color: string }> = {
  Technical: { name: '技术分析部', icon: Brain, color: 'var(--profit)' },
  Capital: { name: '资金分析部', icon: DollarSign, color: 'var(--gold)' },
  News: { name: '新闻分析部', icon: Newspaper, color: 'var(--text-primary)' },
}

function formatSymbol(symbol: string): string {
  return symbol
    .replace('-USDT-SWAP', '/USDT')
    .replace('-USDT', '/USDT')
}

function getDeptInfo(dept: string) {
  return DEPARTMENT_MAP[dept] || { name: dept, icon: MessageSquare, color: 'var(--text-muted)' }
}

function getViewpointColor(viewpoint: string): string {
  switch (viewpoint) {
    case 'bullish': return 'var(--profit)'
    case 'bearish': return 'var(--loss)'
    default: return 'var(--text-muted)'
  }
}

function getViewpointText(viewpoint: string): string {
  switch (viewpoint) {
    case 'bullish': return '看多'
    case 'bearish': return '看空'
    default: return '中性'
  }
}

function getActionText(action: string): string {
  switch (action) {
    case 'long': return '做多'
    case 'short': return '做空'
    case 'hold': return '持有'
    case 'close': return '平仓'
    default: return action
  }
}

function getActionColor(action: string): string {
  switch (action) {
    case 'long': return 'var(--profit)'
    case 'short': return 'var(--loss)'
    default: return 'var(--text-muted)'
  }
}

function formatTime(isoString: string): string {
  if (!isoString) return ''
  const date = new Date(isoString)
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}

function formatDateTime(isoString: string): string {
  if (!isoString) return ''
  const date = new Date(isoString)
  return date.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit'
  })
}

const isDebateCompleted = computed(() => {
  return debateResult.value?.status === 'completed' && debateResult.value?.progress === 100
})

async function startDebate() {
  debateRunning.value = true
  loading.value = true
  debateResult.value = null
  activeTab.value = 'agents'

  try {
    const { data } = await api.post('/ai/debate', {
      symbol: selectedSymbol.value,
      interval: selectedInterval.value
    })

    // If the response already contains the full result (synchronous)
    if (data.agent_opinions || data.status === 'completed') {
      debateResult.value = data
      debateRunning.value = false
      loading.value = false
      await loadHistory()
      return
    }

    // If async, start polling
    const sessionId = data.session_id
    if (sessionId) {
      startPolling(sessionId)
    }
  } catch (e: any) {
    console.error('Failed to start debate', e)
    alert('启动辩论失败: ' + (e.response?.data?.message || e.message))
    debateRunning.value = false
    loading.value = false
  }
}

function startPolling(sessionId: string) {
  if (pollTimer) clearInterval(pollTimer)

  pollTimer = setInterval(async () => {
    try {
      const { data } = await api.get(`/ai/debate/${sessionId}`)
      debateResult.value = data

      if (data.status === 'completed' || data.status === 'failed') {
        if (pollTimer) clearInterval(pollTimer)
        pollTimer = null
        debateRunning.value = false
        loading.value = false
        await loadHistory()
      }
    } catch (e) {
      console.error('Polling failed', e)
      if (pollTimer) clearInterval(pollTimer)
      pollTimer = null
      debateRunning.value = false
      loading.value = false
    }
  }, 3000)
}

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

async function loadDebateSession(sessionId: string) {
  loading.value = true
  try {
    const { data } = await api.get(`/ai/debate/${sessionId}`)
    debateResult.value = data
    selectedSymbol.value = data.symbol || selectedSymbol.value
  } catch (e: any) {
    console.error('Failed to load debate session', e)
    alert('加载辩论会话失败: ' + (e.response?.data?.message || e.message))
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  loadHistory()
})

onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer)
  }
})
</script>

<template>
  <div class="space-y-6">
    <!-- 页面标题 -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">
          Agent 辩论分析
        </h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          观看多部门 AI Agent 的辩论过程，获取深度市场分析
        </p>
      </div>
    </div>

    <!-- 控制面板：交易对选择 + 开始辩论 -->
    <div class="card">
      <div class="flex flex-wrap items-center gap-4">
        <div>
          <label class="text-xs block mb-1" style="color: var(--text-muted)">交易对</label>
          <select v-model="selectedSymbol" class="input-field text-sm" :disabled="debateRunning">
            <option v-for="s in popularSymbols" :key="s" :value="s">{{ formatSymbol(s) }}</option>
          </select>
        </div>
        <div>
          <label class="text-xs block mb-1" style="color: var(--text-muted)">时间周期</label>
          <select v-model="selectedInterval" class="input-field text-sm" :disabled="debateRunning">
            <option v-for="iv in intervals" :key="iv" :value="iv">{{ iv }}</option>
          </select>
        </div>
        <div class="flex-1"></div>
        <div class="flex items-end gap-2">
          <button
            @click="startDebate"
            :disabled="debateRunning"
            class="btn-primary flex items-center gap-2"
          >
            <Loader2 v-if="debateRunning" class="w-4 h-4 animate-spin" />
            <Play v-else class="w-4 h-4" />
            {{ debateRunning ? '辩论进行中...' : '开始辩论' }}
          </button>
        </div>
      </div>

      <!-- 进度条 -->
      <div v-if="debateRunning || (debateResult && debateResult.progress < 100)" class="mt-4">
        <div class="flex items-center justify-between text-sm mb-2">
          <span style="color: var(--text-secondary)">
            {{ debateResult?.status === 'analyzing' ? '分析中...' : debateResult?.status === 'intra_debate' ? '部门内辩论...' : debateResult?.status === 'inter_debate' ? '跨部门辩论...' : debateResult?.status === 'deciding' ? '基金经理决策中...' : '处理中...' }}
          </span>
          <span style="color: var(--gold)">{{ debateResult?.progress || 0 }}%</span>
        </div>
        <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
          <div
            class="h-full rounded-full transition-all duration-500"
            style="background: linear-gradient(90deg, var(--gold-dim), var(--gold))"
            :style="{ width: (debateResult?.progress || 0) + '%' }"
          ></div>
        </div>
      </div>
    </div>

    <!-- 主要内容区域 -->
    <div v-if="debateResult" class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- 左侧：Agent 列表和意见 -->
      <div class="lg:col-span-2 space-y-4">
        <!-- 当前会话信息 -->
        <div class="card">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-4">
              <div>
                <span class="text-sm" style="color: var(--text-secondary)">交易对</span>
                <div class="text-xl font-bold" style="color: var(--text-primary)">{{ formatSymbol(debateResult.symbol) }}</div>
              </div>
              <div class="h-8 w-px" style="background: var(--border)"></div>
              <div>
                <span class="text-sm" style="color: var(--text-secondary)">状态</span>
                <div class="font-medium" :style="{ color: isDebateCompleted ? 'var(--profit)' : 'var(--gold)' }">
                  {{ isDebateCompleted ? '已完成' : '进行中' }}
                </div>
              </div>
              <div class="h-8 w-px" style="background: var(--border)"></div>
              <div v-if="debateResult.market_snapshot?.current_price">
                <span class="text-sm" style="color: var(--text-secondary)">当前价格</span>
                <div class="font-mono font-bold" style="color: var(--text-primary)">{{ debateResult.market_snapshot.current_price }}</div>
              </div>
              <div v-if="debateResult.source" class="h-8 w-px" style="background: var(--border)"></div>
              <div v-if="debateResult.source">
                <span class="text-sm" style="color: var(--text-secondary)">数据源</span>
                <div class="text-xs font-mono" style="color: var(--text-muted)">{{ debateResult.source }}</div>
              </div>
            </div>
          </div>
        </div>

        <!-- 标签切换 -->
        <div class="flex gap-2">
          <button
            @click="activeTab = 'agents'"
            class="px-4 py-2 rounded-lg text-sm font-medium transition-all"
            :style="activeTab === 'agents' 
              ? 'background: var(--gold-glow); color: var(--gold)' 
              : 'background: var(--bg-card-secondary); color: var(--text-secondary)'"
          >
            <div class="flex items-center gap-2">
              <Bot class="w-4 h-4" />
              单个 Agent
            </div>
          </button>
          <button
            @click="activeTab = 'departments'"
            class="px-4 py-2 rounded-lg text-sm font-medium transition-all"
            :style="activeTab === 'departments' 
              ? 'background: var(--gold-glow); color: var(--gold)' 
              : 'background: var(--bg-card-secondary); color: var(--text-secondary)'"
          >
            <div class="flex items-center gap-2">
              <Brain class="w-4 h-4" />
              部门报告
            </div>
          </button>
          <button
            @click="activeTab = 'decision'"
            class="px-4 py-2 rounded-lg text-sm font-medium transition-all"
            :style="activeTab === 'decision' 
              ? 'background: var(--gold-glow); color: var(--gold)' 
              : 'background: var(--bg-card-secondary); color: var(--text-secondary)'"
            :disabled="!isDebateCompleted"
          >
            <div class="flex items-center gap-2">
              <User class="w-4 h-4" />
              基金经理决策
            </div>
          </button>
        </div>

        <!-- Agent 意见视图 -->
        <div v-if="activeTab === 'agents'" class="space-y-4">
          <div v-if="debateResult.agent_opinions.length === 0" class="card py-8 text-center" style="color: var(--text-muted)">
            暂无 Agent 意见
          </div>
          <div v-for="opinion in debateResult.agent_opinions" :key="opinion.agent_id" class="card">
            <div class="flex items-start justify-between mb-4">
              <div class="flex items-center gap-3">
                <div
                  class="w-10 h-10 rounded-xl flex items-center justify-center"
                  :style="{ background: `${getDeptInfo(opinion.department).color}20` }"
                >
                  <component
                    :is="getDeptInfo(opinion.department).icon"
                    class="w-5 h-5"
                    :style="{ color: getDeptInfo(opinion.department).color }"
                  />
                </div>
                <div>
                  <div class="font-medium" style="color: var(--text-primary)">
                    {{ opinion.agent_name }}
                  </div>
                  <div class="text-xs" style="color: var(--text-muted)">
                    {{ getDeptInfo(opinion.department).name }}
                  </div>
                </div>
              </div>
              <div class="flex items-center gap-3">
                <div
                  class="px-3 py-1 rounded-full text-sm font-medium"
                  :style="{
                    background: `${getViewpointColor(opinion.sentiment)}20`,
                    color: getViewpointColor(opinion.sentiment)
                  }"
                >
                  {{ getViewpointText(opinion.sentiment) }}
                  {{ (opinion.confidence * 100).toFixed(0) }}%
                </div>
                <div v-if="opinion.source" class="text-xs font-mono" style="color: var(--text-muted)">
                  {{ opinion.source }}
                </div>
              </div>
            </div>

            <div class="space-y-4">
              <p class="text-sm leading-relaxed" style="color: var(--text-secondary)">
                {{ opinion.analysis }}
              </p>

              <div v-if="opinion.key_factors?.length" class="flex flex-wrap gap-2">
                <span
                  v-for="factor in opinion.key_factors"
                  :key="factor"
                  class="px-3 py-1 rounded-full text-xs"
                  style="background: var(--bg-card-secondary); color: var(--text-muted)"
                >
                  {{ factor }}
                </span>
              </div>
            </div>
          </div>
        </div>

        <!-- 部门报告视图 -->
        <div v-else-if="activeTab === 'departments'" class="space-y-4">
          <div v-if="debateResult.department_reports.length === 0" class="card py-8 text-center" style="color: var(--text-muted)">
            暂无部门报告
          </div>
          <div v-for="report in debateResult.department_reports" :key="report.department" class="card">
            <div class="flex items-start justify-between mb-4">
              <div class="flex items-center gap-3">
                <div
                  class="w-10 h-10 rounded-xl flex items-center justify-center"
                  :style="{ background: `${getDeptInfo(report.department).color}20` }"
                >
                  <component :is="getDeptInfo(report.department).icon" class="w-5 h-5" :style="{ color: getDeptInfo(report.department).color }" />
                </div>
                <div>
                  <div class="font-medium" style="color: var(--text-primary)">{{ getDeptInfo(report.department).name }}</div>
                </div>
              </div>
              <div class="flex items-center gap-3">
                <div
                  class="px-3 py-1 rounded-full text-sm font-medium"
                  :style="{
                    background: `${getViewpointColor(report.consensus.overall_bias)}20`,
                    color: getViewpointColor(report.consensus.overall_bias)
                  }"
                >
                  {{ getViewpointText(report.consensus.overall_bias) }}
                  {{ (report.consensus.confidence * 100).toFixed(0) }}%
                </div>
              </div>
            </div>

            <div class="space-y-4">
              <div class="grid grid-cols-2 gap-4">
                <div class="p-4 rounded-xl" style="background: var(--profit-glow)">
                  <div class="flex items-center gap-2 mb-2">
                    <TrendingUp class="w-4 h-4" style="color: var(--profit)" />
                    <span class="font-medium text-sm" style="color: var(--profit)">多头观点</span>
                  </div>
                  <p class="text-sm" style="color: var(--text-secondary)">
                    {{ report.bull_summary }}
                  </p>
                </div>
                <div class="p-4 rounded-xl" style="background: var(--loss-glow)">
                  <div class="flex items-center gap-2 mb-2">
                    <TrendingDown class="w-4 h-4" style="color: var(--loss)" />
                    <span class="font-medium text-sm" style="color: var(--loss)">空头观点</span>
                  </div>
                  <p class="text-sm" style="color: var(--text-secondary)">
                    {{ report.bear_summary }}
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 基金经理决策视图 -->
        <div v-else-if="activeTab === 'decision'" class="card">
          <div v-if="debateResult.fund_manager_decision" class="space-y-6">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <div class="w-12 h-12 rounded-xl flex items-center justify-center" style="background: linear-gradient(135deg, var(--gold), var(--gold-dim))">
                  <User class="w-6 h-6" style="color: var(--bg-primary)" />
                </div>
                <div>
                  <div class="font-bold text-lg" style="color: var(--text-primary)">基金经理</div>
                </div>
              </div>
              <div
                class="px-4 py-2 rounded-xl text-lg font-bold"
                :style="{
                  background: getActionColor(debateResult.fund_manager_decision.action) + '20',
                  color: getActionColor(debateResult.fund_manager_decision.action)
                }"
              >
                {{ getActionText(debateResult.fund_manager_decision.action) }}
                {{ (debateResult.fund_manager_decision.confidence * 100).toFixed(0) }}%
              </div>
            </div>

            <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">入场区间</div>
                <div class="text-xl font-bold font-mono" style="color: var(--text-primary)">
                  {{ debateResult.fund_manager_decision.entry_range?.low }}-{{ debateResult.fund_manager_decision.entry_range?.high }}
                </div>
              </div>
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">止损</div>
                <div class="text-xl font-bold font-mono" style="color: var(--loss)">{{ debateResult.fund_manager_decision.stop_loss }}</div>
              </div>
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">止盈</div>
                <div class="text-xl font-bold font-mono" style="color: var(--profit)">
                  {{ debateResult.fund_manager_decision.take_profit?.join(', ') }}
                </div>
              </div>
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">杠杆</div>
                <div class="text-xl font-bold" style="color: var(--text-primary)">{{ debateResult.fund_manager_decision.leverage }}x</div>
              </div>
            </div>

            <div class="p-5 rounded-xl" style="background: var(--bg-card-secondary)">
              <div class="text-sm font-medium mb-3" style="color: var(--text-primary)">核心逻辑</div>
              <p class="text-sm leading-relaxed" style="color: var(--text-secondary)">{{ debateResult.fund_manager_decision.reasoning?.primary_thesis }}</p>
            </div>

            <div v-if="debateResult.fund_manager_decision.reasoning?.key_risks?.length" class="p-5 rounded-xl" style="background: var(--loss-glow)">
              <div class="text-sm font-medium mb-3 flex items-center gap-2" style="color: var(--loss)">
                <AlertCircle class="w-4 h-4" />
                风险提示
              </div>
              <ul class="space-y-2">
                <li
                  v-for="risk in debateResult.fund_manager_decision.reasoning.key_risks"
                  :key="risk"
                  class="flex items-start gap-2 text-sm"
                  style="color: var(--text-secondary)"
                >
                  <ChevronRight class="w-4 h-4 mt-0.5 flex-shrink-0" style="color: var(--loss)" />
                  {{ risk }}
                </li>
              </ul>
            </div>
          </div>

          <div v-else class="flex flex-col items-center justify-center py-16">
            <User class="w-16 h-16 mb-4" style="color: var(--text-muted)" />
            <div class="text-lg font-medium" style="color: var(--text-muted)">等待基金经理决策...</div>
            <div class="text-sm mt-2" style="color: var(--text-muted)">完成所有辩论阶段后即可查看最终决策</div>
          </div>
        </div>
      </div>

      <!-- 右侧：辩论进度和历史 -->
      <div class="space-y-4">
        <!-- 辩论阶段 -->
        <div class="card">
          <h3 class="font-medium mb-4" style="color: var(--text-primary)">辩论阶段</h3>
          <div class="space-y-4">
            <div class="flex items-center gap-3" :class="{ 'opacity-50': !debateResult.agent_opinions?.length }">
              <div class="w-8 h-8 rounded-full flex items-center justify-center"
                :style="debateResult.agent_opinions?.length ? 'background: var(--profit-glow); color: var(--profit)' : 'background: var(--border); color: var(--text-muted)'">
                <CheckCircle2 v-if="debateResult.agent_opinions?.length" class="w-4 h-4" />
                <Bot v-else class="w-4 h-4" />
              </div>
              <div class="flex-1">
                <div class="font-medium text-sm" style="color: var(--text-primary)">部门内部分析</div>
                <div class="text-xs" style="color: var(--text-muted)">
                  {{ debateResult.agent_opinions?.length ? `已完成 (${debateResult.agent_opinions.length} 个 Agent)` : '待开始' }}
                </div>
              </div>
            </div>
            <div class="flex items-center gap-3" :class="{ 'opacity-50': !debateResult.department_reports?.length }">
              <div class="w-8 h-8 rounded-full flex items-center justify-center"
                :style="debateResult.department_reports?.length ? 'background: var(--gold-glow); color: var(--gold)' : 'background: var(--border); color: var(--text-muted)'">
                <CheckCircle2 v-if="debateResult.department_reports?.length" class="w-4 h-4" />
                <Brain v-else class="w-4 h-4" />
              </div>
              <div class="flex-1">
                <div class="font-medium text-sm" style="color: var(--text-primary)">跨部门辩论</div>
                <div class="text-xs" style="color: var(--text-muted)">
                  {{ debateResult.department_reports?.length ? `已完成 (${debateResult.department_reports.length} 个部门)` : '待开始' }}
                </div>
              </div>
            </div>
            <div class="flex items-center gap-3" :class="{ 'opacity-50': !debateResult.fund_manager_decision }">
              <div class="w-8 h-8 rounded-full flex items-center justify-center"
                :style="debateResult.fund_manager_decision ? 'background: var(--profit-glow); color: var(--profit)' : 'background: var(--border); color: var(--text-muted)'">
                <CheckCircle2 v-if="debateResult.fund_manager_decision" class="w-4 h-4" />
                <User v-else class="w-4 h-4" />
              </div>
              <div class="flex-1">
                <div class="font-medium text-sm" style="color: var(--text-primary)">基金经理决策</div>
                <div class="text-xs" style="color: var(--text-muted)">
                  {{ debateResult.fund_manager_decision ? '已完成' : '待开始' }}
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 部门共识 -->
        <div v-if="debateResult.department_reports?.length" class="card">
          <h3 class="font-medium mb-4" style="color: var(--text-primary)">部门共识</h3>
          <div class="space-y-3">
            <div v-for="report in debateResult.department_reports" :key="report.department">
              <div class="flex justify-between text-sm mb-1">
                <span style="color: var(--text-secondary)">{{ getDeptInfo(report.department).name }}</span>
                <span :style="{ color: getViewpointColor(report.consensus.overall_bias) }">
                  {{ getViewpointText(report.consensus.overall_bias) }} {{ (report.consensus.confidence * 100).toFixed(0) }}%
                </span>
              </div>
              <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
                <div 
                  class="h-full rounded-full transition-all duration-500"
                  :style="{ 
                    background: getViewpointColor(report.consensus.overall_bias),
                    width: (report.consensus.confidence * 100) + '%'
                  }"
                ></div>
              </div>
            </div>
          </div>
        </div>

        <!-- 历史记录 -->
        <div class="card">
          <div class="flex items-center gap-2 mb-4">
            <History class="w-4 h-4" style="color: var(--text-muted)" />
            <h3 class="font-medium" style="color: var(--text-primary)">历史辩论</h3>
          </div>

          <div v-if="historyLoading" class="space-y-2">
            <div v-for="i in 3" :key="i" class="h-12 rounded-lg animate-pulse" style="background: var(--bg-primary)"></div>
          </div>

          <div v-else-if="debateHistory.length === 0" class="py-4 text-center text-sm" style="color: var(--text-muted)">
            暂无历史记录
          </div>

          <div v-else class="space-y-2 max-h-80 overflow-y-auto">
            <button
              v-for="item in debateHistory"
              :key="item.session_id"
              @click="loadDebateSession(item.session_id)"
              class="w-full p-3 rounded-lg text-left transition-all hover:opacity-80"
              :style="item.session_id === debateResult?.session_id ? 'background: var(--gold-glow)' : 'background: var(--bg-primary)'"
            >
              <div class="flex items-center justify-between">
                <span class="font-medium text-sm" style="color: var(--text-primary)">{{ formatSymbol(item.symbol) }}</span>
                <span 
                  class="text-xs px-2 py-0.5 rounded-full"
                  :class="item.status === 'completed' ? 'bg-green-500/20 text-green-400' : 'bg-yellow-500/20 text-yellow-400'"
                >
                  {{ item.status === 'completed' ? '已完成' : '进行中' }}
                </span>
              </div>
              <div class="text-xs mt-1" style="color: var(--text-muted)">
                {{ formatDateTime(item.created_at) }}
                <span v-if="item.source" class="ml-2 font-mono">{{ item.source }}</span>
              </div>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 空状态 -->
    <div v-else-if="!loading && !debateRunning" class="card py-16">
      <div class="flex flex-col items-center justify-center">
        <Bot class="w-16 h-16 mb-4" style="color: var(--text-muted)" />
        <div class="text-lg font-medium" style="color: var(--text-muted)">选择交易对并开始辩论</div>
        <div class="text-sm mt-2" style="color: var(--text-muted)">AI Agent 将从技术、资金、新闻多维度分析市场</div>
      </div>
    </div>

    <!-- 加载状态 -->
    <div v-if="loading && !debateResult" class="card py-16">
      <div class="flex flex-col items-center justify-center">
        <Loader2 class="w-12 h-12 animate-spin mb-4" style="color: var(--gold)" />
        <div class="text-lg font-medium" style="color: var(--text-muted)">加载中...</div>
      </div>
    </div>
  </div>
</template>
