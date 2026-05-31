<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
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
  ChevronRight
} from 'lucide-vue-next'
import api from '@/api'

interface Agent {
  id: string
  name: string
  department: 'tech' | 'capital' | 'news'
  role: string
  icon: any
  color: string
}

interface AgentOpinion {
  agentId: string
  viewpoint: 'bullish' | 'bearish' | 'neutral'
  confidence: number
  analysis: string
  evidence: string[]
  keyLevels?: {
    support: number[]
    resistance: number[]
  }
  timestamp: string
  status: 'analyzing' | 'debating' | 'completed'
}

interface DepartmentReport {
  department: 'tech' | 'capital' | 'news'
  name: string
  consensus: {
    bullishEvidence: string[]
    bearishEvidence: string[]
    overallBias: 'bullish' | 'bearish' | 'neutral'
    confidence: number
    keyDisagreements: string[]
  }
  bullSummary: string
  bearSummary: string
  completed: boolean
}

interface FundManagerDecision {
  decisionId: string
  timestamp: string
  symbol: string
  action: 'long' | 'short' | 'hold' | 'close'
  confidence: number
  positionSizePercent: number
  entryPriceRange: { low: number; high: number }
  stopLoss: number
  takeProfit: number[]
  leverage: number
  holdingPeriod: string
  riskRewardRatio: number
  reasoning: {
    primaryThesis: string
    keyRisks: string[]
    departmentWeights: Record<string, number>
    overriddenSignals?: Array<{ department: string; signal: string; reason: string }>
  }
  debateSummary: string
  historicalReference: string
}

interface DebateSession {
  id: string
  symbol: string
  status: 'idle' | 'analyzing' | 'intra_debate' | 'inter_debate' | 'deciding' | 'completed'
  phase: string
  progress: number
  startedAt: string
  estimatedEndAt: string
}

const agents: Agent[] = [
  { id: 'tech_kline', name: 'K线形态分析师', department: 'tech', role: '技术分析', icon: TrendingUp, color: 'var(--profit)' },
  { id: 'tech_indicator', name: '技术指标分析师', department: 'tech', role: '技术分析', icon: Brain, color: 'var(--profit)' },
  { id: 'capital_funding', name: '资金费率分析师', department: 'capital', role: '资金分析', icon: DollarSign, color: 'var(--gold)' },
  { id: 'capital_position', name: '持仓结构分析师', department: 'capital', role: '资金分析', icon: CheckCircle2, color: 'var(--gold)' },
  { id: 'news_sentiment', name: '舆情情绪分析师', department: 'news', role: '新闻分析', icon: Newspaper, color: 'var(--text-primary)' },
  { id: 'news_kol', name: 'KOL/鲸鱼监控师', department: 'news', role: '新闻分析', icon: User, color: 'var(--text-primary)' }
]

const departments = [
  { id: 'tech', name: '技术分析部', icon: Brain, color: 'var(--profit)' },
  { id: 'capital', name: '资金分析部', icon: DollarSign, color: 'var(--gold)' },
  { id: 'news', name: '新闻分析部', icon: Newspaper, color: 'var(--text-primary)' }
]

const session = ref<DebateSession>({
  id: 'session_123',
  symbol: 'DOGE-USDT-SWAP',
  status: 'inter_debate',
  phase: '跨部门辩论阶段',
  progress: 65,
  startedAt: new Date(Date.now() - 15 * 60 * 1000).toISOString(),
  estimatedEndAt: new Date(Date.now() + 10 * 60 * 1000).toISOString()
})

const agentOpinions = ref<AgentOpinion[]>([
  {
    agentId: 'tech_kline',
    viewpoint: 'bullish',
    confidence: 0.75,
    analysis: 'DOGE 在 0.22 位置形成了清晰的双底形态，颈线位于 0.24。成交量在第二个底部时出现了明显的放大，显示出较强的买盘力量。',
    evidence: [
      '双底形态确认',
      '支撑位 0.21-0.22 测试成功',
      'MACD 底部背离信号',
      'RSI 从超卖区域回升'
    ],
    keyLevels: {
      support: [0.22, 0.20],
      resistance: [0.25, 0.28]
    },
    timestamp: new Date(Date.now() - 12 * 60 * 1000).toISOString(),
    status: 'completed'
  },
  {
    agentId: 'tech_indicator',
    viewpoint: 'neutral',
    confidence: 0.55,
    analysis: '技术指标呈现混合信号。RSI 处于中性区域，MACD 金叉但动能不足。布林带收窄，预示着可能的突破方向尚未明确。',
    evidence: [
      'RSI 52（中性）',
      'MACD 金叉但柱状图较弱',
      '布林带宽度降至 30 天均值',
      '成交量处于平均水平'
    ],
    timestamp: new Date(Date.now() - 10 * 60 * 1000).toISOString(),
    status: 'completed'
  },
  {
    agentId: 'capital_funding',
    viewpoint: 'bullish',
    confidence: 0.8,
    analysis: '资金费率持续为负，说明空头较为拥挤。当前费率为 -0.015%，处于近 30 天的低位，可能触发逼空行情。',
    evidence: [
      '资金费率 -0.015%（过去 24 小时平均）',
      '连续 5 天负费率',
      '空头持仓比例 58%',
      '历史上类似情况后上涨概率 70%'
    ],
    timestamp: new Date(Date.now() - 8 * 60 * 1000).toISOString(),
    status: 'completed'
  },
  {
    agentId: 'capital_position',
    viewpoint: 'bearish',
    confidence: 0.6,
    analysis: '持仓量（OI）在过去 24 小时下降了 12%，显示资金正在流出市场。大户持仓变化不明显，但散户资金在流出。',
    evidence: [
      'OI 24 小时变化 -12%',
      '大户多空比 45:55',
      '交易所净流出 500 万 DOGE',
      '成交量下降 20%'
    ],
    timestamp: new Date(Date.now() - 6 * 60 * 1000).toISOString(),
    status: 'completed'
  },
  {
    agentId: 'news_sentiment',
    viewpoint: 'bullish',
    confidence: 0.7,
    analysis: '社交媒体情绪在过去 24 小时有所改善。Twitter 上关于 DOGE 的正面提及比例从 45% 上升到 58%。',
    evidence: [
      '社交媒体情绪指数 0.62',
      'Twitter 正面提及率 58%',
      'Reddit r/dogecoin 活跃度上升 30%',
      '谷歌搜索量增加 25%'
    ],
    timestamp: new Date(Date.now() - 4 * 60 * 1000).toISOString(),
    status: 'debating'
  },
  {
    agentId: 'news_kol',
    viewpoint: 'neutral',
    confidence: 0.5,
    analysis: 'Elon Musk 近期没有直接提及 DOGE，但 DOGE 基金会有一些技术更新的讨论。鲸鱼钱包活动平静，没有大额异动。',
    evidence: [
      'Musk 近期无 DOGE 相关推文',
      'DOGE 基金会讨论 Layer 2 方案',
      '鲸鱼钱包（前 10 名）持仓稳定',
      '无重大交易所公告'
    ],
    timestamp: new Date(Date.now() - 2 * 60 * 1000).toISOString(),
    status: 'analyzing'
  }
])

const departmentReports = ref<DepartmentReport[]>([
  {
    department: 'tech',
    name: '技术分析部',
    consensus: {
      bullishEvidence: ['双底形态确认', 'RSI 从超卖回升', 'MACD 底部背离'],
      bearishEvidence: ['成交量未能有效放大', '长期趋势仍为下行'],
      overallBias: 'bullish',
      confidence: 0.65,
      keyDisagreements: ['关于突破的时间点存在分歧，部分认为需要等待确认']
    },
    bullSummary: '技术面显示出筑底迹象，双底形态配合指标背离，可能迎来反弹。',
    bearSummary: '主要趋势仍未反转，需要更多确认信号，建议等待明确突破。',
    completed: true
  },
  {
    department: 'capital',
    name: '资金分析部',
    consensus: {
      bullishEvidence: ['资金费率极低', '空头拥挤可能逼空'],
      bearishEvidence: ['OI 下降显示资金流出', '大户偏向空头'],
      overallBias: 'neutral',
      confidence: 0.55,
      keyDisagreements: ['对资金流出的解读存在分歧，是获利了结还是看空后市']
    },
    bullSummary: '负费率是强烈的反向指标，历史上类似情况后经常出现逼空行情。',
    bearSummary: '资金正在离场，OI 下降不是好信号，可能继续下跌。',
    completed: true
  },
  {
    department: 'news',
    name: '新闻分析部',
    consensus: {
      bullishEvidence: ['社交媒体情绪改善', '社区活跃度上升'],
      bearishEvidence: ['缺乏重大利好催化剂', 'Musk 保持沉默'],
      overallBias: 'neutral',
      confidence: 0.5,
      keyDisagreements: ['对情绪改善的可持续性有不同看法']
    },
    bullSummary: '情绪面正在好转，如果能配合技术面突破，可能形成上涨趋势。',
    bearSummary: '没有实质性利好，情绪改善可能只是暂时的，难以持续。',
    completed: false
  }
])

const fundManagerDecision = ref<FundManagerDecision | null>(null)

const activeTab = ref<'agents' | 'departments' | 'decision'>('agents')
const selectedDepartment = ref<'tech' | 'capital' | 'news' | null>(null)
const loading = ref(false)
const refreshing = ref(false)
let ws: WebSocket | null = null

async function loadSession() {
  loading.value = true
  try {
    // const sessionRes = await api.get(`/agent/sessions/${sessionId}`)
    // const opinionsRes = await api.get(`/agent/sessions/${sessionId}/opinions`)
    // const reportsRes = await api.get(`/agent/sessions/${sessionId}/reports`)
    // const decisionRes = await api.get(`/agent/sessions/${sessionId}/decision`)
    // session.value = sessionRes.data
    // agentOpinions.value = opinionsRes.data
    // departmentReports.value = reportsRes.data
    // fundManagerDecision.value = decisionRes.data
  } catch (e) {
    console.error('Failed to load debate session:', e)
  } finally {
    loading.value = false
  }
}

async function refreshSession() {
  refreshing.value = true
  try {
    await loadSession()
  } finally {
    refreshing.value = false
  }
}

function getAgent(agentId: string): Agent | undefined {
  return agents.find(a => a.id === agentId)
}

function getDepartmentReport(dept: 'tech' | 'capital' | 'news'): DepartmentReport | undefined {
  return departmentReports.value.find(r => r.department === dept)
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

function formatTime(isoString: string): string {
  const date = new Date(isoString)
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}

function initWebSocket() {
  // 这里可以连接 WebSocket 实现实时更新
  // ws = new WebSocket(`${import.meta.env.VITE_WS_URL}/agent/sessions/${sessionId}/ws`)
  // ws.onmessage = (event) => {
  //   const data = JSON.parse(event.data)
  //   if (data.type === 'agent_update') {
  //     updateAgentOpinion(data.data)
  //   } else if (data.type === 'report_update') {
  //     updateDepartmentReport(data.data)
  //   } else if (data.type === 'decision_update') {
  //     fundManagerDecision.value = data.data
  //   } else if (data.type === 'session_update') {
  //     session.value = data.data
  //   }
  // }
}

onMounted(() => {
  loadSession()
  initWebSocket()
})

onUnmounted(() => {
  if (ws) {
    ws.close()
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
      <button
        @click="refreshSession"
        :disabled="refreshing"
        class="btn-secondary flex items-center gap-2"
      >
        <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': refreshing }" />
        刷新
      </button>
    </div>

    <!-- 当前会话信息 -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-4">
          <div>
            <span class="text-sm" style="color: var(--text-secondary)">交易对</span>
            <div class="text-xl font-bold" style="color: var(--text-primary)">{{ session.symbol }}</div>
          </div>
          <div class="h-8 w-px" style="background: var(--border)"></div>
          <div>
            <span class="text-sm" style="color: var(--text-secondary)">阶段</span>
            <div class="font-medium" style="color: var(--text-primary)">{{ session.phase }}</div>
          </div>
          <div class="h-8 w-px" style="background: var(--border)"></div>
          <div>
            <span class="text-sm" style="color: var(--text-secondary)">进度</span>
            <div class="font-medium" style="color: var(--gold)">{{ session.progress }}%</div>
          </div>
        </div>
        <div class="flex items-center gap-2 text-sm" style="color: var(--text-muted)">
          <Clock class="w-4 h-4" />
          <span>开始于 {{ formatTime(session.startedAt) }}</span>
        </div>
      </div>
      
      <!-- 进度条 -->
      <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
        <div
          class="h-full rounded-full transition-all duration-500"
          style="background: linear-gradient(90deg, var(--gold-dim), var(--gold)); width: {{ session.progress }}%"
        ></div>
      </div>
    </div>

    <!-- 主要内容区域 -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- 左侧：Agent 列表和意见 -->
      <div class="lg:col-span-2 space-y-4">
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
            :disabled="session.status !== 'completed'"
          >
            <div class="flex items-center gap-2">
              <User class="w-4 h-4" />
              基金经理决策
            </div>
          </button>
        </div>

        <!-- Agent 意见视图 -->
        <div v-if="activeTab === 'agents'" class="space-y-4">
          <div v-for="opinion in agentOpinions" :key="opinion.agentId" class="card">
            <div class="flex items-start justify-between mb-4">
              <div class="flex items-center gap-3">
                <div
                  class="w-10 h-10 rounded-xl flex items-center justify-center"
                  :style="{ background: `${getAgent(opinion.agentId)?.color}20` }"
                >
                  <component
                    :is="getAgent(opinion.agentId)?.icon"
                    class="w-5 h-5"
                    :style="{ color: getAgent(opinion.agentId)?.color }"
                  />
                </div>
                <div>
                  <div class="font-medium" style="color: var(--text-primary)">
                    {{ getAgent(opinion.agentId)?.name }}
                  </div>
                  <div class="text-xs" style="color: var(--text-muted)">
                    {{ getAgent(opinion.agentId)?.department === 'tech' ? '技术分析部' : getAgent(opinion.agentId)?.department === 'capital' ? '资金分析部' : '新闻分析部' }}
                  </div>
                </div>
              </div>
              <div class="flex items-center gap-3">
                <div
                  class="px-3 py-1 rounded-full text-sm font-medium"
                  :style="{
                    background: `${getViewpointColor(opinion.viewpoint)}20`,
                    color: getViewpointColor(opinion.viewpoint)
                  }"
                >
                  {{ getViewpointText(opinion.viewpoint) }}
                  {{ (opinion.confidence * 100).toFixed(0) }}%
                </div>
                <div class="text-xs" style="color: var(--text-muted)">
                  {{ formatTime(opinion.timestamp) }}
                </div>
              </div>
            </div>

            <div v-if="opinion.status === 'completed' || opinion.status === 'debating'" class="space-y-4">
              <p class="text-sm leading-relaxed" style="color: var(--text-secondary)">
                {{ opinion.analysis }}
              </p>

              <div class="flex flex-wrap gap-2">
                <span
                  v-for="evidence in opinion.evidence"
                  :key="evidence"
                  class="px-3 py-1 rounded-full text-xs"
                  style="background: var(--bg-card-secondary); color: var(--text-muted)"
                >
                  {{ evidence }}
                </span>
              </div>

              <div v-if="opinion.keyLevels" class="flex gap-6 pt-3 border-t" style="border-color: var(--border)">
                <div>
                  <span class="text-xs" style="color: var(--text-muted)">支撑位</span>
                  <div class="flex gap-2 mt-1">
                    <span
                      v-for="level in opinion.keyLevels.support"
                      :key="level"
                      class="px-2 py-1 rounded text-xs font-mono"
                      style="background: var(--profit-glow); color: var(--profit)"
                    >
                      {{ level }}
                    </span>
                  </div>
                </div>
                <div>
                  <span class="text-xs" style="color: var(--text-muted)">阻力位</span>
                  <div class="flex gap-2 mt-1">
                    <span
                      v-for="level in opinion.keyLevels.resistance"
                      :key="level"
                      class="px-2 py-1 rounded text-xs font-mono"
                      style="background: var(--loss-glow); color: var(--loss)"
                    >
                      {{ level }}
                    </span>
                  </div>
                </div>
              </div>
            </div>

            <div v-else class="flex items-center gap-2 py-4">
              <RefreshCw class="w-4 h-4 animate-spin" style="color: var(--text-muted)" />
              <span class="text-sm" style="color: var(--text-muted)">
                {{ opinion.status === 'analyzing' ? '分析中...' : '辩论中...' }}
              </span>
            </div>
          </div>
        </div>

        <!-- 部门报告视图 -->
        <div v-else-if="activeTab === 'departments'" class="space-y-4">
          <div v-for="dept in departments" :key="dept.id" class="card">
            <div class="flex items-start justify-between mb-4">
              <div class="flex items-center gap-3">
                <div
                  class="w-10 h-10 rounded-xl flex items-center justify-center"
                  :style="{ background: `${dept.color}20` }"
                >
                  <component :is="dept.icon" class="w-5 h-5" :style="{ color: dept.color }" />
                </div>
                <div>
                  <div class="font-medium" style="color: var(--text-primary)">{{ dept.name }}</div>
                  <div class="text-xs" style="color: var(--text-muted)">
                    {{ getDepartmentReport(dept.id as any)?.completed ? '报告已完成' : '生成中...' }}
                  </div>
                </div>
              </div>
              <div v-if="getDepartmentReport(dept.id as any)" class="flex items-center gap-3">
                <div
                  class="px-3 py-1 rounded-full text-sm font-medium"
                  :style="{
                    background: `${getViewpointColor(getDepartmentReport(dept.id as any)!.consensus.overallBias)}20`,
                    color: getViewpointColor(getDepartmentReport(dept.id as any)!.consensus.overallBias)
                  }"
                >
                  {{ getViewpointText(getDepartmentReport(dept.id as any)!.consensus.overallBias) }}
                  {{ (getDepartmentReport(dept.id as any)!.consensus.confidence * 100).toFixed(0) }}%
                </div>
              </div>
            </div>

            <div v-if="getDepartmentReport(dept.id as any)?.completed" class="space-y-4">
              <div class="grid grid-cols-2 gap-4">
                <div class="p-4 rounded-xl" style="background: var(--profit-glow)">
                  <div class="flex items-center gap-2 mb-2">
                    <TrendingUp class="w-4 h-4" style="color: var(--profit)" />
                    <span class="font-medium text-sm" style="color: var(--profit)">多头观点</span>
                  </div>
                  <p class="text-sm" style="color: var(--text-secondary)">
                    {{ getDepartmentReport(dept.id as any)?.bullSummary }}
                  </p>
                </div>
                <div class="p-4 rounded-xl" style="background: var(--loss-glow)">
                  <div class="flex items-center gap-2 mb-2">
                    <TrendingDown class="w-4 h-4" style="color: var(--loss)" />
                    <span class="font-medium text-sm" style="color: var(--loss)">空头观点</span>
                  </div>
                  <p class="text-sm" style="color: var(--text-secondary)">
                    {{ getDepartmentReport(dept.id as any)?.bearSummary }}
                  </p>
                </div>
              </div>

              <div class="grid grid-cols-2 gap-4">
                <div>
                  <div class="text-sm font-medium mb-2" style="color: var(--text-primary)">看多证据</div>
                  <ul class="space-y-1">
                    <li
                      v-for="evidence in getDepartmentReport(dept.id as any)?.consensus.bullishEvidence"
                      :key="evidence"
                      class="flex items-start gap-2 text-sm"
                      style="color: var(--text-secondary)"
                    >
                      <CheckCircle2 class="w-4 h-4 mt-0.5 flex-shrink-0" style="color: var(--profit)" />
                      {{ evidence }}
                    </li>
                  </ul>
                </div>
                <div>
                  <div class="text-sm font-medium mb-2" style="color: var(--text-primary)">看空证据</div>
                  <ul class="space-y-1">
                    <li
                      v-for="evidence in getDepartmentReport(dept.id as any)?.consensus.bearishEvidence"
                      :key="evidence"
                      class="flex items-start gap-2 text-sm"
                      style="color: var(--text-secondary)"
                    >
                      <AlertCircle class="w-4 h-4 mt-0.5 flex-shrink-0" style="color: var(--loss)" />
                      {{ evidence }}
                    </li>
                  </ul>
                </div>
              </div>

              <div v-if="getDepartmentReport(dept.id as any)?.consensus.keyDisagreements.length" class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-sm font-medium mb-2" style="color: var(--text-primary)">关键分歧</div>
                <ul class="space-y-1">
                  <li
                    v-for="disagreement in getDepartmentReport(dept.id as any)?.consensus.keyDisagreements"
                    :key="disagreement"
                    class="flex items-start gap-2 text-sm"
                    style="color: var(--text-muted)"
                  >
                    <AlertCircle class="w-4 h-4 mt-0.5 flex-shrink-0" />
                    {{ disagreement }}
                  </li>
                </ul>
              </div>
            </div>

            <div v-else class="flex items-center gap-2 py-8 justify-center">
              <RefreshCw class="w-5 h-5 animate-spin" style="color: var(--text-muted)" />
              <span style="color: var(--text-muted)">正在生成部门报告...</span>
            </div>
          </div>
        </div>

        <!-- 基金经理决策视图 -->
        <div v-else-if="activeTab === 'decision'" class="card">
          <div v-if="fundManagerDecision" class="space-y-6">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <div class="w-12 h-12 rounded-xl flex items-center justify-center" style="background: linear-gradient(135deg, var(--gold), var(--gold-dim))">
                  <User class="w-6 h-6" style="color: var(--bg-primary)" />
                </div>
                <div>
                  <div class="font-bold text-lg" style="color: var(--text-primary)">基金经理</div>
                  <div class="text-sm" style="color: var(--text-muted)">{{ formatTime(fundManagerDecision.timestamp) }}</div>
                </div>
              </div>
              <div
                class="px-4 py-2 rounded-xl text-lg font-bold"
                :style="{
                  background: fundManagerDecision.action === 'long' ? 'var(--profit-glow)' : fundManagerDecision.action === 'short' ? 'var(--loss-glow)' : 'var(--bg-card-secondary)',
                  color: fundManagerDecision.action === 'long' ? 'var(--profit)' : fundManagerDecision.action === 'short' ? 'var(--loss)' : 'var(--text-muted)'
                }"
              >
                {{ fundManagerDecision.action === 'long' ? '做多' : fundManagerDecision.action === 'short' ? '做空' : fundManagerDecision.action === 'hold' ? '持有' : '平仓' }}
                {{ (fundManagerDecision.confidence * 100).toFixed(0) }}%
              </div>
            </div>

            <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">仓位比例</div>
                <div class="text-xl font-bold" style="color: var(--text-primary)">{{ fundManagerDecision.positionSizePercent }}%</div>
              </div>
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">入场区间</div>
                <div class="text-xl font-bold font-mono" style="color: var(--text-primary)">
                  {{ fundManagerDecision.entryPriceRange.low }}-{{ fundManagerDecision.entryPriceRange.high }}
                </div>
              </div>
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">止损</div>
                <div class="text-xl font-bold font-mono" style="color: var(--loss)">{{ fundManagerDecision.stopLoss }}</div>
              </div>
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">止盈</div>
                <div class="text-xl font-bold font-mono" style="color: var(--profit)">
                  {{ fundManagerDecision.takeProfit.join(', ') }}
                </div>
              </div>
            </div>

            <div class="grid grid-cols-2 gap-4">
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">杠杆</div>
                <div class="text-xl font-bold" style="color: var(--text-primary)">{{ fundManagerDecision.leverage }}x</div>
              </div>
              <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">盈亏比</div>
                <div class="text-xl font-bold" style="color: var(--text-primary)">{{ fundManagerDecision.riskRewardRatio }}:1</div>
              </div>
            </div>

            <div class="p-5 rounded-xl" style="background: var(--bg-card-secondary)">
              <div class="text-sm font-medium mb-3" style="color: var(--text-primary)">核心逻辑</div>
              <p class="text-sm leading-relaxed" style="color: var(--text-secondary)">{{ fundManagerDecision.reasoning.primaryThesis }}</p>
            </div>

            <div v-if="fundManagerDecision.reasoning.keyRisks.length" class="p-5 rounded-xl" style="background: var(--loss-glow)">
              <div class="text-sm font-medium mb-3 flex items-center gap-2" style="color: var(--loss)">
                <AlertCircle class="w-4 h-4" />
                风险提示
              </div>
              <ul class="space-y-2">
                <li
                  v-for="risk in fundManagerDecision.reasoning.keyRisks"
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
        <div class="card">
          <h3 class="font-medium mb-4" style="color: var(--text-primary)">辩论阶段</h3>
          <div class="space-y-4">
            <div class="flex items-center gap-3">
              <div class="w-8 h-8 rounded-full flex items-center justify-center" style="background: var(--profit-glow); color: var(--profit)">
                <CheckCircle2 class="w-4 h-4" />
              </div>
              <div class="flex-1">
                <div class="font-medium text-sm" style="color: var(--text-primary)">部门内部分析</div>
                <div class="text-xs" style="color: var(--text-muted)">已完成</div>
              </div>
            </div>
            <div class="flex items-center gap-3">
              <div class="w-8 h-8 rounded-full flex items-center justify-center" style="background: var(--gold-glow); color: var(--gold)">
                <RefreshCw class="w-4 h-4 animate-spin" />
              </div>
              <div class="flex-1">
                <div class="font-medium text-sm" style="color: var(--text-primary)">跨部门辩论</div>
                <div class="text-xs" style="color: var(--text-muted)">进行中</div>
              </div>
            </div>
            <div class="flex items-center gap-3 opacity-50">
              <div class="w-8 h-8 rounded-full flex items-center justify-center" style="background: var(--border); color: var(--text-muted)">
                <User class="w-4 h-4" />
              </div>
              <div class="flex-1">
                <div class="font-medium text-sm" style="color: var(--text-muted)">基金经理决策</div>
                <div class="text-xs" style="color: var(--text-muted)">待开始</div>
              </div>
            </div>
          </div>
        </div>

        <div class="card">
          <h3 class="font-medium mb-4" style="color: var(--text-primary)">部门权重</h3>
          <div class="space-y-3">
            <div>
              <div class="flex justify-between text-sm mb-1">
                <span style="color: var(--text-secondary)">技术分析部</span>
                <span style="color: var(--profit)">35%</span>
              </div>
              <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
                <div class="h-full rounded-full" style="background: var(--profit); width: 35%"></div>
              </div>
            </div>
            <div>
              <div class="flex justify-between text-sm mb-1">
                <span style="color: var(--text-secondary)">资金分析部</span>
                <span style="color: var(--gold)">35%</span>
              </div>
              <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
                <div class="h-full rounded-full" style="background: var(--gold); width: 35%"></div>
              </div>
            </div>
            <div>
              <div class="flex justify-between text-sm mb-1">
                <span style="color: var(--text-secondary)">新闻分析部</span>
                <span style="color: var(--text-primary)">30%</span>
              </div>
              <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
                <div class="h-full rounded-full" style="background: var(--text-primary); width: 30%"></div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
