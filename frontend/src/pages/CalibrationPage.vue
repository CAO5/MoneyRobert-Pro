<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { SignalApi } from '@/api'
import {
  Gauge, RefreshCw, AlertTriangle, CheckCircle, TrendingUp,
  Activity, Target, BarChart3
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const calibration = ref<Record<string, unknown> | null>(null)
const loading = ref(false)
const computing = ref(false)
const error = ref('')

const modelVersion = ref('v1.0.0')
const symbol = ref('')
const startTime = ref('')
const endTime = ref('')

// =========================================================
// 计算属性
// =========================================================

const calibrationCurve = computed<Array<{ bin: string; predicted: number; actual: number }>>(() => {
  if (!calibration.value) return []
  const curve = calibration.value.calibration_curve as Array<{ bin_center: number; predicted_prob: number; actual_freq: number; count: number }> | undefined
  if (!curve || !Array.isArray(curve)) return []
  return curve.map((p, i) => ({
    bin: `Bin ${i + 1}`,
    predicted: p.predicted_prob ?? p.bin_center ?? 0,
    actual: p.actual_freq ?? 0,
  }))
})

const isWellCalibrated = computed(() => {
  return calibration.value?.is_well_calibrated as boolean ?? false
})

const degradationDetected = computed(() => {
  return calibration.value?.degradation_detected as boolean ?? false
})

// =========================================================
// 方法
// =========================================================

async function loadCalibration() {
  loading.value = true
  error.value = ''
  try {
    calibration.value = await SignalApi.getCalibration(modelVersion.value) as unknown as Record<string, unknown>
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载校准报告失败'
  } finally {
    loading.value = false
  }
}

async function computeCalibration() {
  computing.value = true
  error.value = ''
  try {
    const now = new Date()
    const weekAgo = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000)
    calibration.value = await SignalApi.computeCalibration({
      model_version: modelVersion.value,
      symbol: symbol.value || undefined,
      start_time: startTime.value || weekAgo.toISOString(),
      end_time: endTime.value || now.toISOString(),
    }) as unknown as Record<string, unknown>
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '计算校准失败'
  } finally {
    computing.value = false
  }
}

function formatNum(v: number | undefined, digits = 4): string {
  if (v == null || isNaN(v)) return '-'
  return v.toFixed(digits)
}

function formatPct(v: number | undefined, digits = 2): string {
  if (v == null || isNaN(v)) return '-'
  return (v * 100).toFixed(digits) + '%'
}

onMounted(() => {
  loadCalibration()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">概率校准</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          Brier Score、Log Loss、校准曲线：验证预测概率与实际频率的一致性
        </p>
      </div>
      <button @click="loadCalibration" class="btn btn-secondary">
        <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
        刷新
      </button>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <!-- 查询/计算表单 -->
    <div class="card p-5">
      <h3 class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">校准参数</h3>
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">模型版本</label>
          <input v-model="modelVersion" class="input" placeholder="v1.0.0" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">交易对（可选）</label>
          <input v-model="symbol" class="input" placeholder="BTC-USDT-SWAP" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">开始时间</label>
          <input v-model="startTime" type="datetime-local" class="input" />
        </div>
        <div>
          <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">结束时间</label>
          <input v-model="endTime" type="datetime-local" class="input" />
        </div>
      </div>
      <div class="flex gap-2 mt-4">
        <button @click="computeCalibration" class="btn btn-primary" :disabled="computing">
          <Activity class="w-4 h-4" :class="computing ? 'animate-pulse' : ''" />
          {{ computing ? '计算中...' : '触发校准计算' }}
        </button>
      </div>
    </div>

    <!-- 校准结果 -->
    <div v-if="calibration">
      <!-- 状态指示器 -->
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div class="card p-4 flex items-center gap-3">
          <component
            :is="isWellCalibrated ? CheckCircle : AlertTriangle"
            class="w-8 h-8"
            :style="{ color: isWellCalibrated ? 'var(--profit)' : 'var(--warning)' }"
          />
          <div>
            <div class="text-xs" style="color: var(--text-muted)">校准状态</div>
            <div class="font-semibold" :style="{ color: isWellCalibrated ? 'var(--profit)' : 'var(--warning)' }">
              {{ isWellCalibrated ? '校准良好' : '需要改进' }}
            </div>
          </div>
        </div>
        <div class="card p-4 flex items-center gap-3">
          <component
            :is="degradationDetected ? AlertTriangle : CheckCircle"
            class="w-8 h-8"
            :style="{ color: degradationDetected ? 'var(--loss)' : 'var(--profit)' }"
          />
          <div>
            <div class="text-xs" style="color: var(--text-muted)">退化检测</div>
            <div class="font-semibold" :style="{ color: degradationDetected ? 'var(--loss)' : 'var(--profit)' }">
              {{ degradationDetected ? '检测到退化' : '无退化' }}
            </div>
          </div>
        </div>
        <div class="card p-4 flex items-center gap-3">
          <Target class="w-8 h-8" style="color: var(--primary)" />
          <div>
            <div class="text-xs" style="color: var(--text-muted)">样本量</div>
            <div class="font-semibold" style="color: var(--text-primary)">
              {{ calibration.sample_size || calibration.total_samples || 0 }}
            </div>
          </div>
        </div>
      </div>

      <!-- 指标卡片 -->
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">Brier Score</div>
          <div class="text-xl font-bold" :style="{
            color: (calibration.brier_score as number) < 0.25 ? 'var(--profit)' : 'var(--warning)'
          }">
            {{ formatNum(calibration.brier_score as number) }}
          </div>
          <div class="text-xs mt-1" style="color: var(--text-muted)">越低越好 (0=完美)</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">Log Loss</div>
          <div class="text-xl font-bold" :style="{
            color: (calibration.log_loss as number) < 0.5 ? 'var(--profit)' : 'var(--warning)'
          }">
            {{ formatNum(calibration.log_loss as number) }}
          </div>
          <div class="text-xs mt-1" style="color: var(--text-muted)">越低越好 (0=完美)</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">准确率</div>
          <div class="text-xl font-bold" style="color: var(--text-primary)">
            {{ formatPct(calibration.accuracy as number) }}
          </div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">校准误差</div>
          <div class="text-xl font-bold" :style="{
            color: (calibration.calibration_error as number) < 0.1 ? 'var(--profit)' : 'var(--warning)'
          }">
            {{ formatNum(calibration.calibration_error as number) }}
          </div>
          <div class="text-xs mt-1" style="color: var(--text-muted)">越低越好</div>
        </div>
      </div>

      <!-- 校准曲线 -->
      <div v-if="calibrationCurve.length > 0" class="card p-5">
        <h3 class="text-sm font-semibold mb-4 flex items-center gap-2" style="color: var(--text-secondary)">
          <Gauge class="w-4 h-4" />
          校准曲线（预测概率 vs 实际频率）
        </h3>
        <div class="space-y-2">
          <div
            v-for="point in calibrationCurve"
            :key="point.bin"
            class="flex items-center gap-3"
          >
            <div class="w-16 text-xs text-right" style="color: var(--text-muted)">
              {{ (point.predicted * 100).toFixed(0) }}%
            </div>
            <div class="flex-1 relative h-6 rounded" style="background: var(--surface)">
              <!-- 预测概率条 -->
              <div
                class="absolute h-full rounded opacity-30"
                :style="{
                  width: (point.predicted * 100) + '%',
                  background: 'var(--primary)'
                }"
              />
              <!-- 实际频率条 -->
              <div
                class="absolute h-full rounded"
                :style="{
                  width: (point.actual * 100) + '%',
                  background: point.actual >= point.predicted - 0.05 && point.actual <= point.predicted + 0.05
                    ? 'var(--profit)'
                    : 'var(--warning)'
                }"
              />
            </div>
            <div class="w-16 text-xs" :style="{
              color: point.actual >= point.predicted - 0.05 && point.actual <= point.predicted + 0.05
                ? 'var(--profit)' : 'var(--warning)'
            }">
              {{ (point.actual * 100).toFixed(1) }}%
            </div>
          </div>
        </div>
        <div class="flex items-center gap-4 mt-4 text-xs" style="color: var(--text-muted)">
          <span class="flex items-center gap-1">
            <span class="w-3 h-3 rounded" style="background: var(--primary); opacity: 0.3" />
            预测概率
          </span>
          <span class="flex items-center gap-1">
            <span class="w-3 h-3 rounded" style="background: var(--profit)" />
            实际频率（匹配）
          </span>
          <span class="flex items-center gap-1">
            <span class="w-3 h-3 rounded" style="background: var(--warning)" />
            实际频率（偏离）
          </span>
        </div>
      </div>

      <!-- 理想校准线说明 -->
      <div class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--primary)">
        <BarChart3 class="w-5 h-5" style="color: var(--primary)" />
        <div class="text-sm" style="color: var(--text-secondary)">
          理想校准：预测概率 = 实际频率（对角线）。偏离越大，校准越差。
          Brier Score &lt; 0.25 为可接受，&lt; 0.15 为良好。
        </div>
      </div>
    </div>

    <!-- 空状态 -->
    <div v-else-if="!loading && !error" class="card p-12 text-center">
      <Gauge class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted)" />
      <p class="text-sm" style="color: var(--text-muted)">暂无校准数据，点击"触发校准计算"开始</p>
    </div>
  </div>
</template>
