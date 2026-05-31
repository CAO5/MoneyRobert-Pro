<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import {
  TrendingUp,
  TrendingDown,
  Play,
  Square,
  Bot,
  Target,
  Activity,
  Trophy,
  AlertCircle,
  CheckCircle2,
  Clock,
  MessageSquare
} from 'lucide-vue-next'
import api from '@/api'

const router = useRouter()

interface AgentLevel {
  level: number
  name: string
  progress: number
  nextLevelPoints: number
  currentPoints: number
}

interface TradingStats {
  totalTrades: number
  winRate: number
  totalPnl: number
  avgPnlPercent: number
  currentStreak: number
}

interface AgentStatus {
  simulationRunning: boolean
  autonomousTrading: boolean
  currentSessionId: string | null
  lastUpdate: string
}

interface PromotionStatus {
  eligible: boolean
  currentRank: string
  nextRank: string
  criteriaMet: string[]
  criteriaMissing: string[]
}

const agentLevel = ref<AgentLevel>({
  level: 3,
  name: '资深交易员',
  progress: 65,
  nextLevelPoints: 1000,
  currentPoints: 650
})

const tradingStats = ref<TradingStats>({
  totalTrades: 47,
  winRate: 68.1,
  totalPnl: 1245.67,
  avgPnlPercent: 2.4,
  currentStreak: 5
})

const agentStatus = ref<AgentStatus>({
  simulationRunning: false,
  autonomousTrading: false,
  currentSessionId: null,
  lastUpdate: new Date().toISOString()
})

const promotionStatus = ref<PromotionStatus>({
  eligible: false,
  currentRank: '资深交易员',
  nextRank: '基金经理',
  criteriaMet: [
    '连续 7 天稳定盈利',
    '胜率保持在 60% 以上',
    '最大回撤 < 5%'
  ],
  criteriaMissing: [
    '完成 100 笔模拟交易',
    '通过风险控制考核'
  ]
})

const loading = ref(true)
const starting = ref(false)
const stopping = ref(false)

async function loadData() {
  loading.value = true
  try {
    // 这里可以调用实际的 API
    // const [levelRes, statsRes, statusRes, promotionRes] = await Promise.all([
    //   api.get('/agent/level'),
    //   api.get('/agent/stats'),
    //   api.get('/agent/status'),
    //   api.get('/agent/promotion')
    // ])
    // agentLevel.value = levelRes.data
    // tradingStats.value = statsRes.data
    // agentStatus.value = statusRes.data
    // promotionStatus.value = promotionRes.data
  } catch (e) {
    console.error('Failed to load agent dashboard data:', e)
  } finally {
    loading.value = false
  }
}

async function toggleSimulation() {
  if (agentStatus.value.simulationRunning) {
    await stopSimulation()
  } else {
    await startSimulation()
  }
}

async function startSimulation() {
  starting.value = true
  try {
    // await api.post('/agent/simulation/start')
    agentStatus.value.simulationRunning = true
    agentStatus.value.lastUpdate = new Date().toISOString()
  } catch (e) {
    console.error('Failed to start simulation:', e)
  } finally {
    starting.value = false
  }
}

async function stopSimulation() {
  stopping.value = true
  try {
    // await api.post('/agent/simulation/stop')
    agentStatus.value.simulationRunning = false
    agentStatus.value.lastUpdate = new Date().toISOString()
  } catch (e) {
    console.error('Failed to stop simulation:', e)
  } finally {
    stopping.value = false
  }
}

async function toggleAutonomousTrading() {
  if (agentStatus.value.autonomousTrading) {
    await stopAutonomousTrading()
  } else {
    await startAutonomousTrading()
  }
}

async function startAutonomousTrading() {
  starting.value = true
  try {
    // await api.post('/agent/autonomous/start')
    agentStatus.value.autonomousTrading = true
    agentStatus.value.lastUpdate = new Date().toISOString()
  } catch (e) {
    console.error('Failed to start autonomous trading:', e)
  } finally {
    starting.value = false
  }
}

async function stopAutonomousTrading() {
  stopping.value = true
  try {
    // await api.post('/agent/autonomous/stop')
    agentStatus.value.autonomousTrading = false
    agentStatus.value.lastUpdate = new Date().toISOString()
  } catch (e) {
    console.error('Failed to stop autonomous trading:', e)
  } finally {
    stopping.value = false
  }
}

function goToDebateViewer() {
  router.push('/agent/debate')
}

function goToTradingHistory() {
  router.push('/agent/history')
}

onMounted(() => {
  loadData()
})
</script>

<template>
  <div class="space-y-6">
    <!-- 页面标题 -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">
          Agent 仪表盘
        </h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          管理您的 AI 交易 Agent，查看实时分析和交易记录
        </p>
      </div>
    </div>

    <!-- 加载状态 -->
    <div v-if="loading" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
      <div v-for="i in 4" :key="i" class="card animate-pulse h-32"></div>
    </div>

    <div v-else>
      <!-- Agent 等级和状态 -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <!-- 左侧：等级卡片 -->
        <div class="lg:col-span-2 card">
          <div class="flex items-start justify-between mb-6">
            <div>
              <h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary)">
                Agent 等级
              </h2>
              <div class="flex items-center gap-3">
                <div class="w-12 h-12 rounded-xl flex items-center justify-center" style="background: linear-gradient(135deg, var(--gold), var(--gold-dim))">
                  <Trophy class="w-6 h-6" style="color: var(--bg-primary)" />
                </div>
                <div>
                  <div class="text-2xl font-bold" style="color: var(--gold)">
                    Lv.{{ agentLevel.level }}
                  </div>
                  <div class="text-sm" style="color: var(--text-secondary)">
                    {{ agentLevel.name }}
                  </div>
                </div>
              </div>
            </div>
            <div class="text-right">
              <div class="text-sm" style="color: var(--text-secondary)">
                经验值
              </div>
              <div class="text-xl font-bold" style="color: var(--text-primary)">
                {{ agentLevel.currentPoints }} / {{ agentLevel.nextLevelPoints }}
              </div>
            </div>
          </div>

          <!-- 进度条 -->
          <div class="mb-6">
            <div class="flex justify-between text-sm mb-2">
              <span style="color: var(--text-secondary)">升级进度</span>
              <span style="color: var(--gold)">{{ agentLevel.progress }}%</span>
            </div>
            <div class="h-3 rounded-full overflow-hidden" style="background: var(--border)">
              <div
                class="h-full rounded-full transition-all duration-500"
                style="background: linear-gradient(90deg, var(--gold-dim), var(--gold)); width: {{ agentLevel.progress }}%"
              ></div>
            </div>
          </div>

          <!-- 晋级状态 -->
          <div class="border-t pt-6" style="border-color: var(--border)">
            <div class="flex items-center gap-2 mb-4">
              <Activity class="w-5 h-5" style="color: var(--text-secondary)" />
              <h3 class="font-semibold" style="color: var(--text-primary)">晋级状态</h3>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <div class="text-sm mb-2" style="color: var(--text-secondary)">
                  当前：{{ promotionStatus.currentRank }} → 下一：{{ promotionStatus.nextRank }}
                </div>
                <div class="space-y-2">
                  <div v-for="criteria in promotionStatus.criteriaMet" :key="criteria" class="flex items-center gap-2">
                    <CheckCircle2 class="w-4 h-4" style="color: var(--profit)" />
                    <span class="text-sm" style="color: var(--text-secondary)">{{ criteria }}</span>
                  </div>
                  <div v-for="criteria in promotionStatus.criteriaMissing" :key="criteria" class="flex items-center gap-2">
                    <AlertCircle class="w-4 h-4" style="color: var(--text-muted)" />
                    <span class="text-sm" style="color: var(--text-muted)">{{ criteria }}</span>
                  </div>
                </div>
              </div>
              <div class="flex items-center justify-center">
                <div
                  class="px-6 py-3 rounded-xl text-center"
                  :style="{
                    background: promotionStatus.eligible ? 'var(--profit-glow)' : 'var(--border)',
                    color: promotionStatus.eligible ? 'var(--profit)' : 'var(--text-muted)'
                  }"
                >
                  {{ promotionStatus.eligible ? '🎉 符合晋级条件' : '📊 继续努力' }}
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 右侧：快速操作 -->
        <div class="card">
          <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">
            快速操作
          </h2>
          <div class="space-y-4">
            <!-- 模拟交易 -->
            <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
              <div class="flex items-center justify-between mb-3">
                <div class="flex items-center gap-2">
                  <Bot class="w-5 h-5" style="color: var(--text-primary)" />
                  <span class="font-medium" style="color: var(--text-primary)">模拟交易</span>
                </div>
                <div
                  class="flex items-center gap-1 text-xs px-2 py-1 rounded-full"
                  :style="{
                    background: agentStatus.simulationRunning ? 'var(--profit-glow)' : 'var(--text-muted-glow)',
                    color: agentStatus.simulationRunning ? 'var(--profit)' : 'var(--text-muted)'
                  }"
                >
                  <div
                    class="w-2 h-2 rounded-full"
                    :style="{
                      background: agentStatus.simulationRunning ? 'var(--profit)' : 'var(--text-muted)',
                      animation: agentStatus.simulationRunning ? 'pulse 2s infinite' : 'none'
                    }"
                  ></div>
                  {{ agentStatus.simulationRunning ? '运行中' : '已停止' }}
                </div>
              </div>
              <button
                @click="toggleSimulation"
                :disabled="starting || stopping"
                class="btn-primary w-full flex items-center justify-center gap-2"
                :class="agentStatus.simulationRunning ? '!bg-red-600 !hover:bg-red-700' : ''"
              >
                <component :is="agentStatus.simulationRunning ? Square : Play" class="w-4 h-4" />
                {{ starting || stopping ? '处理中...' : (agentStatus.simulationRunning ? '停止模拟' : '启动模拟') }}
              </button>
            </div>

            <!-- 自主交易 -->
            <div class="p-4 rounded-xl" style="background: var(--bg-card-secondary)">
              <div class="flex items-center justify-between mb-3">
                <div class="flex items-center gap-2">
                  <Target class="w-5 h-5" style="color: var(--text-primary)" />
                  <span class="font-medium" style="color: var(--text-primary)">自主交易</span>
                </div>
                <div
                  class="flex items-center gap-1 text-xs px-2 py-1 rounded-full"
                  :style="{
                    background: agentStatus.autonomousTrading ? 'var(--profit-glow)' : 'var(--text-muted-glow)',
                    color: agentStatus.autonomousTrading ? 'var(--profit)' : 'var(--text-muted)'
                  }"
                >
                  <div
                    class="w-2 h-2 rounded-full"
                    :style="{
                      background: agentStatus.autonomousTrading ? 'var(--profit)' : 'var(--text-muted)',
                      animation: agentStatus.autonomousTrading ? 'pulse 2s infinite' : 'none'
                    }"
                  ></div>
                  {{ agentStatus.autonomousTrading ? '运行中' : '已停止' }}
                </div>
              </div>
              <button
                @click="toggleAutonomousTrading"
                :disabled="starting || stopping"
                class="btn-secondary w-full flex items-center justify-center gap-2"
                :class="agentStatus.autonomousTrading ? '!border-red-600 !text-red-400' : ''"
              >
                <component :is="agentStatus.autonomousTrading ? Square : Play" class="w-4 h-4" />
                {{ starting || stopping ? '处理中...' : (agentStatus.autonomousTrading ? '停止自主' : '启动自主') }}
              </button>
            </div>

            <!-- 其他快捷操作 -->
            <div class="grid grid-cols-1 gap-2">
              <button
                @click="goToDebateViewer"
                class="btn-secondary w-full flex items-center justify-center gap-2"
              >
                <MessageSquare class="w-4 h-4" />
                查看辩论分析
              </button>
              <button
                @click="goToTradingHistory"
                class="btn-secondary w-full flex items-center justify-center gap-2"
              >
                <Clock class="w-4 h-4" />
                交易历史记录
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- 统计信息卡片 -->
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mt-6">
        <div class="card group cursor-default">
          <div class="flex items-center justify-between mb-3">
            <span class="text-sm" style="color: var(--text-secondary)">总交易次数</span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center" style="background: var(--text-primary-glow)">
              <Activity class="w-4 h-4" style="color: var(--text-primary)" />
            </div>
          </div>
          <div class="text-2xl font-bold" style="color: var(--text-primary)">
            {{ tradingStats.totalTrades }}
          </div>
        </div>

        <div class="card group cursor-default">
          <div class="flex items-center justify-between mb-3">
            <span class="text-sm" style="color: var(--text-secondary)">胜率</span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center" style="background: var(--profit-glow)">
              <CheckCircle2 class="w-4 h-4" style="color: var(--profit)" />
            </div>
          </div>
          <div class="text-2xl font-bold" style="color: var(--profit)">
            {{ tradingStats.winRate }}%
          </div>
        </div>

        <div class="card group cursor-default">
          <div class="flex items-center justify-between mb-3">
            <span class="text-sm" style="color: var(--text-secondary)">总盈亏</span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center" style="background: var(--profit-glow)">
              <TrendingUp class="w-4 h-4" style="color: var(--profit)" />
            </div>
          </div>
          <div class="text-2xl font-bold" style="color: var(--profit)">
            +${{ tradingStats.totalPnl.toFixed(2) }}
          </div>
        </div>

        <div class="card group cursor-default">
          <div class="flex items-center justify-between mb-3">
            <span class="text-sm" style="color: var(--text-secondary)">当前连胜</span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center" style="background: var(--gold-glow)">
              <Trophy class="w-4 h-4" style="color: var(--gold)" />
            </div>
          </div>
          <div class="text-2xl font-bold" style="color: var(--gold)">
            {{ tradingStats.currentStreak }} 连胜
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
</style>
