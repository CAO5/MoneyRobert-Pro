<script setup lang="ts">
import { ref } from 'vue'
import { BacktestApi } from '@/api'
import {
  FlaskConical, Zap, AlertTriangle, CheckCircle, Clock,
  TrendingUp, TrendingDown, BarChart3, Calendar
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const form = ref({
  train_window_days: 90,
  test_window_days: 30,
  step_days: 30,
  purge_days: 1,
  embargo_days: 1,
  start_time: '',
  end_time: '',
})

const result = ref<Record<string, unknown> | null>(null)
const loading = ref(false)
const error = ref('')

// =========================================================
// 方法
// =========================================================

async function generate() {
  if (!form.value.start_time || !form.value.end_time) {
    error.value = '请选择起始和结束时间'
    return
  }
  loading.value = true
  error.value = ''
  try {
    result.value = await BacktestApi.generateWalkForwardWindows({
      train_window_days: form.value.train_window_days,
      test_window_days: form.value.test_window_days,
      step_days: form.value.step_days,
      purge_days: form.value.purge_days,
      embargo_days: form.value.embargo_days,
      start_time: new Date(form.value.start_time).toISOString(),
      end_time: new Date(form.value.end_time).toISOString(),
    })
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '生成窗口失败'
  } finally {
    loading.value = false
  }
}

function formatTime(t: string): string {
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  })
}

function setQuickRange(months: number) {
  const end = new Date()
  const start = new Date(end.getTime() - months * 30 * 24 * 60 * 60 * 1000)
  form.value.start_time = start.toISOString().slice(0, 16)
  form.value.end_time = end.toISOString().slice(0, 16)
}
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">Walk-forward 验证</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          滚动窗口样本外验证，含 purge 和 embargo 防数据泄露
        </p>
      </div>
    </div>

    <!-- 配置表单 -->
    <div class="card p-5">
      <h3 class="text-sm font-semibold mb-4 flex items-center gap-2" style="color: var(--text-secondary)">
        <Calendar class="w-4 h-4" />
        验证配置
      </h3>

      <!-- 快速选择 -->
      <div class="flex gap-2 mb-4">
        <button @click="setQuickRange(3)" class="btn btn-sm btn-secondary">近 3 个月</button>
        <button @click="setQuickRange(6)" class="btn btn-sm btn-secondary">近 6 个月</button>
        <button @click="setQuickRange(12)" class="btn btn-sm btn-secondary">近 1 年</button>
        <button @click="setQuickRange(24)" class="btn btn-sm btn-secondary">近 2 年</button>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">起始时间</label>
          <input v-model="form.start_time" type="datetime-local" class="input" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">结束时间</label>
          <input v-model="form.end_time" type="datetime-local" class="input" />
        </div>
        <div class="flex items-end">
          <button @click="generate" class="btn btn-primary w-full" :disabled="loading">
            <Zap class="w-4 h-4" :class="loading ? 'animate-pulse' : ''" />
            {{ loading ? '生成中...' : '生成窗口' }}
          </button>
        </div>
      </div>

      <!-- 窗口参数 -->
      <div class="grid grid-cols-2 md:grid-cols-5 gap-4 mt-4">
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">训练窗口(天)</label>
          <input v-model.number="form.train_window_days" type="number" class="input" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">测试窗口(天)</label>
          <input v-model.number="form.test_window_days" type="number" class="input" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">步进(天)</label>
          <input v-model.number="form.step_days" type="number" class="input" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">Purge(天)</label>
          <input v-model.number="form.purge_days" type="number" class="input" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">Embargo(天)</label>
          <input v-model.number="form.embargo_days" type="number" class="input" />
        </div>
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <!-- 结果 -->
    <div v-if="result">
      <!-- 配置摘要 -->
      <div class="card p-4 mb-4">
        <div class="flex items-center gap-2 mb-2">
          <BarChart3 class="w-4 h-4" style="color: var(--primary)" />
          <span class="text-sm font-semibold" style="color: var(--text-primary)">配置摘要</span>
        </div>
        <div class="grid grid-cols-2 md:grid-cols-6 gap-3 text-sm">
          <div><span style="color: var(--text-muted)">训练窗口:</span> <span style="color: var(--text-primary)">{{ (result.config as Record<string, number>).train_window_days }} 天</span></div>
          <div><span style="color: var(--text-muted)">测试窗口:</span> <span style="color: var(--text-primary)">{{ (result.config as Record<string, number>).test_window_days }} 天</span></div>
          <div><span style="color: var(--text-muted)">步进:</span> <span style="color: var(--text-primary)">{{ (result.config as Record<string, number>).step_days }} 天</span></div>
          <div><span style="color: var(--text-muted)">Purge:</span> <span style="color: var(--text-primary)">{{ (result.config as Record<string, number>).purge_days }} 天</span></div>
          <div><span style="color: var(--text-muted)">Embargo:</span> <span style="color: var(--text-primary)">{{ (result.config as Record<string, number>).embargo_days }} 天</span></div>
          <div><span style="color: var(--text-muted)">总窗口:</span> <span class="font-bold" style="color: var(--primary)">{{ result.total_windows }}</span></div>
        </div>
      </div>

      <!-- 窗口列表 -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <FlaskConical class="w-5 h-5" />
            Walk-forward 窗口
          </h2>
        </div>
        <div class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr style="border-bottom: 1px solid var(--border)">
                <th class="text-center py-3 px-4" style="color: var(--text-muted)">#</th>
                <th class="text-left py-3 px-4" style="color: var(--text-muted)">训练集</th>
                <th class="text-left py-3 px-4" style="color: var(--text-muted)">测试集</th>
                <th class="text-center py-3 px-4" style="color: var(--text-muted)">Purge</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="w in (result.windows as Array<Record<string, unknown>>)"
                :key="(w.window_index as number)"
                style="border-bottom: 1px solid var(--border)"
                class="hover:bg-[var(--surface)]"
              >
                <td class="text-center py-2 px-4 font-semibold" style="color: var(--primary)">
                  {{ (w.window_index as number) + 1 }}
                </td>
                <td class="py-2 px-4" style="color: var(--text-secondary)">
                  {{ formatTime(w.train_start as string) }} ~ {{ formatTime(w.train_end as string) }}
                </td>
                <td class="py-2 px-4" style="color: var(--text-primary)">
                  {{ formatTime(w.test_start as string) }} ~ {{ formatTime(w.test_end as string) }}
                </td>
                <td class="text-center py-2 px-4">
                  <span class="text-xs px-2 py-0.5 rounded" style="background: var(--surface); color: var(--warning)">
                    {{ form.purge_days }}d
                  </span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- 说明 -->
      <div class="card p-4 flex items-start gap-3" style="border-left: 3px solid var(--primary)">
        <CheckCircle class="w-5 h-5 mt-0.5" style="color: var(--primary)" />
        <div class="text-sm" style="color: var(--text-secondary)">
          <p class="mb-1"><strong style="color: var(--text-primary)">Purge（清洗期）</strong>：训练集和测试集之间的隔离期，防止训练集末尾标签泄露到测试集。</p>
          <p class="mb-1"><strong style="color: var(--text-primary)">Embargo（禁运期）</strong>：测试集后的观察禁止期，防止测试集标签泄露到下一个训练集。</p>
          <p>验证标准：至少 3 个窗口、正收益占比 > 50%、平均收益 > 0。</p>
        </div>
      </div>
    </div>
  </div>
</template>
