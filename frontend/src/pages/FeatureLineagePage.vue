<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { BacktestApi } from '@/api'
import api from '@/api'
import {
  GitBranch, RefreshCw, AlertTriangle, Database, Clock,
  Hash, FileText, Code, Layers
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const definitions = ref<Array<{ feature_id: string; name: string; category: string; version: string }>>([])
const selectedFeatureId = ref('')
const symbol = ref('BTC-USDT-SWAP')
const lineages = ref<Array<Record<string, unknown>>>([])
const loading = ref(false)
const error = ref('')

// =========================================================
// 方法
// =========================================================

async function loadDefinitions() {
  try {
    const { data } = await api.get('/features/definitions')
    definitions.value = (data.definitions || data || []).map((d: Record<string, unknown>) => ({
      feature_id: d.feature_id as string,
      name: d.name as string,
      category: d.category as string,
      version: d.version as string,
    }))
    if (definitions.value.length > 0 && !selectedFeatureId.value) {
      selectedFeatureId.value = definitions.value[0].feature_id
      await loadLineage()
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载特征定义失败'
  }
}

async function loadLineage() {
  if (!selectedFeatureId.value) return
  loading.value = true
  error.value = ''
  try {
    const data = await BacktestApi.queryFeatureLineage({
      feature_id: selectedFeatureId.value,
      symbol: symbol.value,
    })
    lineages.value = data.lineages || []
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载血缘失败'
  } finally {
    loading.value = false
  }
}

function formatTime(t: string): string {
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

function categoryColor(cat: string): string {
  switch (cat) {
    case 'momentum': return 'var(--primary)'
    case 'volatility': return 'var(--warning)'
    case 'volume': return 'var(--profit)'
    case 'microstructure': return 'var(--loss)'
    default: return 'var(--text-muted)'
  }
}

onMounted(() => {
  loadDefinitions()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">特征血缘</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          数据源、计算版本、参数 hash、上游特征追溯，确保特征可复现
        </p>
      </div>
      <button @click="loadLineage" class="btn btn-secondary" :disabled="!selectedFeatureId">
        <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
        刷新
      </button>
    </div>

    <!-- 查询条件 -->
    <div class="card p-4 flex items-center gap-4 flex-wrap">
      <div class="flex items-center gap-2">
        <label class="text-sm font-medium" style="color: var(--text-secondary)">特征</label>
        <select v-model="selectedFeatureId" @change="loadLineage" class="input" style="min-width: 250px">
          <option v-for="d in definitions" :key="d.feature_id" :value="d.feature_id">
            {{ d.name }} ({{ d.category }})
          </option>
        </select>
      </div>
      <div class="flex items-center gap-2">
        <label class="text-sm font-medium" style="color: var(--text-secondary)">交易对</label>
        <input v-model="symbol" @keyup.enter="loadLineage" class="input" style="width: 180px" />
      </div>
      <div class="ml-auto text-sm" style="color: var(--text-muted)">
        共 {{ lineages.length }} 条记录
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <!-- 血缘列表 -->
    <div v-if="lineages.length === 0 && !loading" class="card p-12 text-center">
      <GitBranch class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted)" />
      <p class="text-sm" style="color: var(--text-muted)">暂无血缘数据</p>
      <p class="text-xs mt-1" style="color: var(--text-muted)">特征计算时会自动写入血缘记录</p>
    </div>

    <div v-else class="space-y-3">
      <div
        v-for="lineage in lineages.slice(0, 50)"
        :key="(lineage.lineage_id as string)"
        class="card p-4"
      >
        <div class="flex items-start justify-between mb-3">
          <div class="flex items-center gap-3">
            <div class="w-10 h-10 rounded-lg flex items-center justify-center" :style="{
              background: 'var(--surface)'
            }">
              <GitBranch class="w-5 h-5" style="color: var(--primary)" />
            </div>
            <div>
              <div class="font-semibold" style="color: var(--text-primary)">
                {{ lineage.symbol }}
              </div>
              <div class="text-xs flex items-center gap-2 mt-0.5" style="color: var(--text-muted)">
                <Clock class="w-3 h-3" />
                {{ formatTime(lineage.timestamp as string) }}
              </div>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <span class="text-xs px-2 py-1 rounded" :style="{
              background: 'var(--surface)',
              color: 'var(--primary)'
            }">
              {{ lineage.calc_version }}
            </span>
          </div>
        </div>

        <!-- 血缘详情 -->
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
          <div class="flex items-center gap-2">
            <Database class="w-3.5 h-3.5" style="color: var(--text-muted)" />
            <div>
              <div style="color: var(--text-muted)">数据源</div>
              <div class="font-medium" style="color: var(--text-primary)">{{ lineage.data_source }}</div>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <Hash class="w-3.5 h-3.5" style="color: var(--text-muted)" />
            <div>
              <div style="color: var(--text-muted)">参数 Hash</div>
              <div class="font-mono text-[10px]" style="color: var(--text-secondary)">
                {{ (lineage.parameters_hash as string)?.substring(0, 16) }}...
              </div>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <Clock class="w-3.5 h-3.5" style="color: var(--text-muted)" />
            <div>
              <div style="color: var(--text-muted)">源时间范围</div>
              <div style="color: var(--text-secondary)">
                {{ lineage.source_time_start ? formatTime(lineage.source_time_start as string) : '-' }}
              </div>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <FileText class="w-3.5 h-3.5" style="color: var(--text-muted)" />
            <div>
              <div style="color: var(--text-muted)">原始数据引用</div>
              <div class="font-mono text-[10px]" style="color: var(--text-secondary)">
                {{ lineage.raw_data_refs ? '有' : '无' }}
              </div>
            </div>
          </div>
        </div>

        <!-- 参数 JSON -->
        <div v-if="lineage.parameters" class="mt-3 p-2 rounded text-xs" style="background: var(--surface)">
          <div class="flex items-center gap-1 mb-1">
            <Code class="w-3 h-3" style="color: var(--text-muted)" />
            <span style="color: var(--text-muted)">计算参数</span>
          </div>
          <pre class="font-mono text-[10px] overflow-x-auto" style="color: var(--text-secondary)">{{ JSON.stringify(lineage.parameters, null, 2) }}</pre>
        </div>
      </div>
    </div>

    <!-- 说明 -->
    <div class="card p-4 flex items-start gap-3" style="border-left: 3px solid var(--primary)">
      <Layers class="w-5 h-5 mt-0.5" style="color: var(--primary)" />
      <div class="text-sm" style="color: var(--text-secondary)">
        <p>特征血缘记录每个特征值的<strong style="color: var(--text-primary)">数据来源</strong>、<strong style="color: var(--text-primary)">计算版本</strong>、<strong style="color: var(--text-primary)">参数 hash</strong>，确保特征可追溯、可复现。</p>
        <p class="mt-1">每次特征计算时自动写入，用于审计和模型版本管理。</p>
      </div>
    </div>
  </div>
</template>
