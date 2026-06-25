<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { ModelCardApi, type ModelCardSummary, type ModelCardDetail } from '@/api'
import {
  BadgeCheck, RefreshCw, AlertTriangle, CheckCircle, XCircle,
  TrendingUp, Activity, Shield, FileText, ArrowUpCircle, ArrowDownCircle,
  Plus, Eye, RotateCcw
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const cards = ref<ModelCardSummary[]>([])
const loading = ref(false)
const error = ref('')
const selectedCard = ref<ModelCardDetail | null>(null)
const detailLoading = ref(false)
const showCreateForm = ref(false)

// 状态过滤
const statusFilter = ref('')

// 创建表单
const createForm = ref({
  model_version: '',
  model_type: 'classifier',
  model_name: '',
  description: '',
  intended_use: '',
  out_of_scope: '',
})

// =========================================================
// 计算属性
// =========================================================

const filteredCards = computed(() => {
  if (!statusFilter.value) return cards.value
  return cards.value.filter(c => c.status === statusFilter.value)
})

const statusCounts = computed(() => {
  const counts: Record<string, number> = {}
  for (const c of cards.value) {
    counts[c.status] = (counts[c.status] || 0) + 1
  }
  return counts
})

// =========================================================
// 方法
// =========================================================

async function loadCards() {
  loading.value = true
  error.value = ''
  try {
    const data = await ModelCardApi.listCards({ limit: 100 })
    cards.value = data.cards || []
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载模型卡列表失败'
  } finally {
    loading.value = false
  }
}

async function viewCard(modelVersion: string) {
  detailLoading.value = true
  selectedCard.value = null
  try {
    selectedCard.value = await ModelCardApi.getCard(modelVersion)
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载模型卡详情失败'
  } finally {
    detailLoading.value = false
  }
}

async function createCard() {
  if (!createForm.value.model_version || !createForm.value.model_name) {
    error.value = '模型版本和名称不能为空'
    return
  }
  loading.value = true
  error.value = ''
  try {
    await ModelCardApi.createCard({
      model_version: createForm.value.model_version,
      model_type: createForm.value.model_type,
      model_name: createForm.value.model_name,
      description: createForm.value.description || undefined,
      intended_use: createForm.value.intended_use || undefined,
      out_of_scope: createForm.value.out_of_scope || undefined,
    })
    showCreateForm.value = false
    createForm.value = { model_version: '', model_type: 'classifier', model_name: '', description: '', intended_use: '', out_of_scope: '' }
    await loadCards()
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '创建模型卡失败'
  } finally {
    loading.value = false
  }
}

async function promoteCard(modelVersion: string, newStatus: string) {
  if (!confirm(`确认将模型 ${modelVersion} 状态变更为 ${newStatus}？`)) return
  try {
    await ModelCardApi.promoteCard(modelVersion, newStatus)
    await loadCards()
    if (selectedCard.value?.model_version === modelVersion) {
      await viewCard(modelVersion)
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '状态变更失败'
  }
}

async function rollbackCard(modelVersion: string) {
  if (!confirm(`确认回滚模型 ${modelVersion} 到之前的版本？`)) return
  try {
    await ModelCardApi.rollbackCard(modelVersion)
    await loadCards()
    if (selectedCard.value?.model_version === modelVersion) {
      await viewCard(modelVersion)
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '回滚失败'
  }
}

function statusColor(status: string): string {
  switch (status) {
    case 'active': return 'var(--profit)'
    case 'shadow': return 'var(--primary)'
    case 'draft': return 'var(--text-muted)'
    case 'deprecated': return 'var(--warning)'
    case 'rolled_back': return 'var(--loss)'
    default: return 'var(--text-muted)'
  }
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    draft: '草稿',
    shadow: '影子盘',
    active: '活跃',
    deprecated: '已弃用',
    rolled_back: '已回滚',
  }
  return map[status] || status
}

function formatNum(v: number | undefined | null, digits = 4): string {
  if (v == null || isNaN(v)) return '-'
  return v.toFixed(digits)
}

function formatPct(v: number | undefined | null, digits = 2): string {
  if (v == null || isNaN(v)) return '-'
  return (v * 100).toFixed(digits) + '%'
}

function formatTime(t: string | undefined): string {
  if (!t) return '-'
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  })
}

function closeDetail() {
  selectedCard.value = null
}

onMounted(() => {
  loadCards()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">模型卡</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          模型发布治理：校准证据、信任评估、失效条件、版本门禁
        </p>
      </div>
      <div class="flex gap-2">
        <button @click="showCreateForm = !showCreateForm" class="btn btn-primary">
          <Plus class="w-4 h-4" />
          新建模型卡
        </button>
        <button @click="loadCards" class="btn btn-secondary">
          <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
          刷新
        </button>
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
      <button @click="error = ''" class="ml-auto text-xs" style="color: var(--text-muted)">关闭</button>
    </div>

    <!-- 创建表单 -->
    <div v-if="showCreateForm" class="card p-5">
      <h3 class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">新建模型卡</h3>
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">模型版本 *</label>
          <input v-model="createForm.model_version" class="input" placeholder="v1.0.0" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">模型类型 *</label>
          <select v-model="createForm.model_type" class="input">
            <option value="classifier">分类器</option>
            <option value="regressor">回归器</option>
            <option value="ensemble">集成模型</option>
            <option value="llm">大语言模型</option>
          </select>
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">模型名称 *</label>
          <input v-model="createForm.model_name" class="input" placeholder="BTC 趋势预测模型" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">描述</label>
          <input v-model="createForm.description" class="input" placeholder="模型描述" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">预期用途</label>
          <input v-model="createForm.intended_use" class="input" placeholder="短期趋势预测" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">不适用场景</label>
          <input v-model="createForm.out_of_scope" class="input" placeholder="极端市场条件" />
        </div>
      </div>
      <div class="flex gap-2 mt-4">
        <button @click="createCard" class="btn btn-primary" :disabled="loading">
          <BadgeCheck class="w-4 h-4" />
          聚合生成
        </button>
        <button @click="showCreateForm = false" class="btn btn-secondary">取消</button>
      </div>
    </div>

    <!-- 状态统计 -->
    <div v-if="cards.length > 0" class="grid grid-cols-2 md:grid-cols-5 gap-3">
      <div v-for="(count, status) in statusCounts" :key="status"
           class="card p-3 cursor-pointer transition hover:opacity-80"
           :style="{ borderLeft: `3px solid ${statusColor(status)}` }"
           @click="statusFilter = statusFilter === status ? '' : status">
        <div class="text-xs" style="color: var(--text-muted)">{{ statusLabel(status) }}</div>
        <div class="text-xl font-bold" :style="{ color: statusColor(status) }">{{ count }}</div>
      </div>
    </div>

    <!-- 状态过滤提示 -->
    <div v-if="statusFilter" class="flex items-center gap-2 text-sm" style="color: var(--text-secondary)">
      <span>过滤状态：</span>
      <span class="px-2 py-0.5 rounded text-xs font-medium" :style="{ background: statusColor(statusFilter), color: 'white' }">
        {{ statusLabel(statusFilter) }}
      </span>
      <button @click="statusFilter = ''" class="text-xs" style="color: var(--text-muted)">清除</button>
    </div>

    <!-- 模型卡列表 -->
    <div class="card">
      <div class="p-5" style="border-bottom: 1px solid var(--border)">
        <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
          <FileText class="w-5 h-5" />
          模型卡列表
        </h2>
      </div>
      <div v-if="loading" class="p-8 text-center" style="color: var(--text-muted)">
        <RefreshCw class="w-6 h-6 mx-auto mb-2 animate-spin" />
        加载中...
      </div>
      <div v-else-if="filteredCards.length === 0" class="p-8 text-center" style="color: var(--text-muted)">
        <BadgeCheck class="w-12 h-12 mx-auto mb-3 opacity-30" />
        {{ cards.length === 0 ? '暂无模型卡，点击"新建模型卡"开始' : '该状态下无模型卡' }}
      </div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr style="border-bottom: 1px solid var(--border)">
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">模型版本</th>
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">名称</th>
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">类型</th>
              <th class="text-center py-3 px-4" style="color: var(--text-muted)">状态</th>
              <th class="text-center py-3 px-4" style="color: var(--text-muted)">可晋级</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">Brier</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">准确率</th>
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">更新时间</th>
              <th class="text-center py-3 px-4" style="color: var(--text-muted)">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="card in filteredCards"
              :key="card.card_id"
              style="border-bottom: 1px solid var(--border)"
              class="hover:bg-[var(--surface)]"
            >
              <td class="py-2 px-4 font-mono text-xs" style="color: var(--text-primary)">
                {{ card.model_version }}
              </td>
              <td class="py-2 px-4" style="color: var(--text-primary)">{{ card.model_name }}</td>
              <td class="py-2 px-4" style="color: var(--text-secondary)">{{ card.model_type }}</td>
              <td class="text-center py-2 px-4">
                <span class="px-2 py-0.5 rounded text-xs font-medium"
                      :style="{ background: statusColor(card.status) + '20', color: statusColor(card.status) }">
                  {{ statusLabel(card.status) }}
                </span>
              </td>
              <td class="text-center py-2 px-4">
                <CheckCircle v-if="card.promotion_eligible" class="w-4 h-4 mx-auto" style="color: var(--profit)" />
                <XCircle v-else class="w-4 h-4 mx-auto" style="color: var(--text-muted)" />
              </td>
              <td class="text-right py-2 px-4 font-mono" :style="{
                color: card.brier_score != null && card.brier_score < 0.25 ? 'var(--profit)' : 'var(--warning)'
              }">
                {{ formatNum(card.brier_score) }}
              </td>
              <td class="text-right py-2 px-4 font-mono" style="color: var(--text-primary)">
                {{ formatPct(card.accuracy) }}
              </td>
              <td class="py-2 px-4 text-xs" style="color: var(--text-muted)">
                {{ formatTime(card.updated_at) }}
              </td>
              <td class="text-center py-2 px-4">
                <button @click="viewCard(card.model_version)" class="text-xs px-2 py-1 rounded hover:bg-[var(--surface)]" style="color: var(--primary)">
                  <Eye class="w-4 h-4" />
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- 模型卡详情弹窗 -->
    <div v-if="selectedCard || detailLoading" class="fixed inset-0 z-50 flex items-center justify-center" style="background: rgba(0,0,0,0.5)" @click.self="closeDetail">
      <div class="card max-w-4xl w-full max-h-[90vh] overflow-y-auto m-4">
        <!-- 详情头部 -->
        <div class="p-5 flex items-center justify-between" style="border-bottom: 1px solid var(--border)">
          <div class="flex items-center gap-3">
            <BadgeCheck class="w-6 h-6" :style="{ color: statusColor(selectedCard?.status || '') }" />
            <div>
              <h2 class="text-lg font-bold" style="color: var(--text-primary)">{{ selectedCard?.model_name || '加载中...' }}</h2>
              <p class="text-xs font-mono" style="color: var(--text-muted)">{{ selectedCard?.model_version }}</p>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <span class="px-2 py-0.5 rounded text-xs font-medium"
                  :style="{ background: statusColor(selectedCard?.status || '') + '20', color: statusColor(selectedCard?.status || '') }">
              {{ statusLabel(selectedCard?.status || '') }}
            </span>
            <button @click="closeDetail" class="text-xs" style="color: var(--text-muted)">关闭</button>
          </div>
        </div>

        <div v-if="detailLoading" class="p-8 text-center" style="color: var(--text-muted)">
          <RefreshCw class="w-6 h-6 mx-auto mb-2 animate-spin" />
          加载详情...
        </div>

        <div v-else-if="selectedCard" class="p-5 space-y-5">
          <!-- 质量指标 -->
          <div>
            <h3 class="text-sm font-semibold mb-3 flex items-center gap-2" style="color: var(--text-secondary)">
              <Activity class="w-4 h-4" />
              质量证据
            </h3>
            <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
              <div class="card p-3">
                <div class="text-xs" style="color: var(--text-muted)">Brier Score</div>
                <div class="text-lg font-bold" :style="{
                  color: selectedCard.brier_score != null && selectedCard.brier_score < 0.25 ? 'var(--profit)' : 'var(--warning)'
                }">{{ formatNum(selectedCard.brier_score) }}</div>
              </div>
              <div class="card p-3">
                <div class="text-xs" style="color: var(--text-muted)">Log Loss</div>
                <div class="text-lg font-bold" :style="{
                  color: selectedCard.log_loss != null && selectedCard.log_loss < 0.5 ? 'var(--profit)' : 'var(--warning)'
                }">{{ formatNum(selectedCard.log_loss) }}</div>
              </div>
              <div class="card p-3">
                <div class="text-xs" style="color: var(--text-muted)">准确率</div>
                <div class="text-lg font-bold" style="color: var(--text-primary)">{{ formatPct(selectedCard.accuracy) }}</div>
              </div>
              <div class="card p-3">
                <div class="text-xs" style="color: var(--text-muted)">可晋级</div>
                <div class="text-lg font-bold" :style="{ color: selectedCard.promotion_eligible ? 'var(--profit)' : 'var(--text-muted)' }">
                  {{ selectedCard.promotion_eligible ? '是' : '否' }}
                </div>
              </div>
            </div>
          </div>

          <!-- 描述信息 -->
          <div v-if="selectedCard.description || selectedCard.intended_use || selectedCard.out_of_scope" class="grid grid-cols-1 md:grid-cols-3 gap-3">
            <div v-if="selectedCard.description" class="card p-3">
              <div class="text-xs font-medium mb-1" style="color: var(--text-muted)">描述</div>
              <div class="text-sm" style="color: var(--text-secondary)">{{ selectedCard.description }}</div>
            </div>
            <div v-if="selectedCard.intended_use" class="card p-3">
              <div class="text-xs font-medium mb-1" style="color: var(--text-muted)">预期用途</div>
              <div class="text-sm" style="color: var(--text-secondary)">{{ selectedCard.intended_use }}</div>
            </div>
            <div v-if="selectedCard.out_of_scope" class="card p-3">
              <div class="text-xs font-medium mb-1" style="color: var(--text-muted)">不适用场景</div>
              <div class="text-sm" style="color: var(--text-secondary)">{{ selectedCard.out_of_scope }}</div>
            </div>
          </div>

          <!-- 失效条件 -->
          <div v-if="selectedCard.invalidation_conditions" class="card p-4">
            <h4 class="text-xs font-semibold mb-2 flex items-center gap-1" style="color: var(--warning)">
              <AlertTriangle class="w-3 h-3" />
              失效条件
            </h4>
            <div class="space-y-1">
              <div v-for="(cond, idx) in (Array.isArray(selectedCard.invalidation_conditions) ? selectedCard.invalidation_conditions : [])" :key="idx"
                   class="text-xs flex items-start gap-2" style="color: var(--text-secondary)">
                <span style="color: var(--warning)">•</span>
                <span>{{ (cond as Record<string, unknown>)?.description || (cond as Record<string, unknown>)?.condition || cond }}</span>
              </div>
            </div>
          </div>

          <!-- 已知限制 -->
          <div v-if="selectedCard.known_limitations" class="card p-4">
            <h4 class="text-xs font-semibold mb-2 flex items-center gap-1" style="color: var(--text-secondary)">
              <Shield class="w-3 h-3" />
              已知限制
            </h4>
            <div class="space-y-1">
              <div v-for="(lim, idx) in (Array.isArray(selectedCard.known_limitations) ? selectedCard.known_limitations : [])" :key="idx"
                   class="text-xs flex items-start gap-2" style="color: var(--text-secondary)">
                <span style="color: var(--text-muted)">•</span>
                <span>{{ lim }}</span>
              </div>
            </div>
          </div>

          <!-- 审计信息 -->
          <div class="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
            <div>
              <span style="color: var(--text-muted)">创建时间：</span>
              <span style="color: var(--text-secondary)">{{ formatTime(selectedCard.created_at) }}</span>
            </div>
            <div>
              <span style="color: var(--text-muted)">更新时间：</span>
              <span style="color: var(--text-secondary)">{{ formatTime(selectedCard.updated_at) }}</span>
            </div>
            <div v-if="selectedCard.approved_at">
              <span style="color: var(--text-muted)">审批时间：</span>
              <span style="color: var(--text-secondary)">{{ formatTime(selectedCard.approved_at) }}</span>
            </div>
            <div v-if="selectedCard.previous_version">
              <span style="color: var(--text-muted)">上一版本：</span>
              <span class="font-mono" style="color: var(--text-secondary)">{{ selectedCard.previous_version }}</span>
            </div>
          </div>

          <!-- 操作按钮 -->
          <div class="flex flex-wrap gap-2 pt-3" style="border-top: 1px solid var(--border)">
            <!-- draft -> shadow -->
            <button v-if="selectedCard.status === 'draft' && selectedCard.promotion_eligible"
                    @click="promoteCard(selectedCard.model_version, 'shadow')"
                    class="btn btn-primary text-xs">
              <ArrowUpCircle class="w-4 h-4" />
              晋级到影子盘
            </button>
            <!-- shadow -> active -->
            <button v-if="selectedCard.status === 'shadow'"
                    @click="promoteCard(selectedCard.model_version, 'active')"
                    class="btn btn-primary text-xs">
              <ArrowUpCircle class="w-4 h-4" />
              晋级到活跃
            </button>
            <!-- active -> deprecated -->
            <button v-if="selectedCard.status === 'active'"
                    @click="promoteCard(selectedCard.model_version, 'deprecated')"
                    class="btn btn-secondary text-xs">
              <ArrowDownCircle class="w-4 h-4" />
              弃用
            </button>
            <!-- active -> rolled_back -->
            <button v-if="selectedCard.status === 'active' && selectedCard.previous_version"
                    @click="rollbackCard(selectedCard.model_version)"
                    class="btn btn-secondary text-xs" style="color: var(--loss)">
              <RotateCcw class="w-4 h-4" />
              回滚
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
