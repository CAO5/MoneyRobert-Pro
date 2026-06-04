<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { createChart, LineSeries, HistogramSeries, ColorType } from 'lightweight-charts'
import type { IChartApi } from 'lightweight-charts'

const props = defineProps<{
  data: Array<{ time: number; value: number; color?: string }>
  title?: string
  height?: number
  lineColor?: string
  type?: 'line' | 'histogram'
}>()

const chartContainer = ref<HTMLDivElement>()
let chart: IChartApi | null = null
let series: any = null

function initChart() {
  if (!chartContainer.value) return

  chart = createChart(chartContainer.value, {
    width: chartContainer.value.clientWidth,
    height: props.height || 250,
    layout: {
      background: { type: ColorType.Solid, color: '#111622' },
      textColor: '#94A3B8',
      fontSize: 11,
    },
    grid: {
      vertLines: { color: 'rgba(255, 255, 255, 0.04)' },
      horzLines: { color: 'rgba(255, 255, 255, 0.04)' },
    },
    crosshair: {
      vertLine: { color: 'rgba(212, 175, 55, 0.3)', labelBackgroundColor: '#D4AF37' },
      horzLine: { color: 'rgba(212, 175, 55, 0.3)', labelBackgroundColor: '#D4AF37' },
    },
    rightPriceScale: {
      borderColor: 'rgba(255, 255, 255, 0.1)',
    },
    timeScale: {
      borderColor: 'rgba(255, 255, 255, 0.1)',
      timeVisible: true,
      secondsVisible: false,
    },
    localization: {
      timeFormatter: (time: number) => {
        const date = new Date(time * 1000)
        const y = date.getFullYear()
        const m = String(date.getMonth() + 1).padStart(2, '0')
        const d = String(date.getDate()).padStart(2, '0')
        const h = String(date.getHours()).padStart(2, '0')
        const min = String(date.getMinutes()).padStart(2, '0')
        return `${y}-${m}-${d} ${h}:${min}`
      },
    },
  })

  const color = props.lineColor || '#3B82F6'

  if (props.type === 'histogram') {
    series = chart.addSeries(HistogramSeries, {
      priceFormat: { type: 'percent' },
    })
  } else {
    series = chart.addSeries(LineSeries, {
      color,
      lineWidth: 2,
      priceFormat: { type: props.title?.includes('费率') ? 'percent' : 'price' },
    })
  }

  updateData()
}

function updateData() {
  if (!series || !props.data.length) return

  // Sort by time and deduplicate (lightweight-charts requires strictly ascending time)
  const sortedData = [...props.data].sort((a, b) => a.time - b.time)
  const dedupedData = sortedData.filter((d, i) => i === 0 || d.time !== sortedData[i - 1].time)

  if (props.type === 'histogram') {
    const histSeries = series
    histSeries.setData(dedupedData.map(d => ({
      time: d.time,
      value: d.value,
      color: d.color || (d.value >= 0 ? 'rgba(0, 200, 83, 0.5)' : 'rgba(255, 23, 68, 0.5)'),
    })))
  } else {
    const lineSeries = series
    lineSeries.setData(dedupedData.map(d => ({
      time: d.time,
      value: d.value,
    })))
  }

  chart?.timeScale().fitContent()
}

function handleResize() {
  if (chart && chartContainer.value) {
    chart.applyOptions({ width: chartContainer.value.clientWidth })
  }
}

watch(() => props.data, () => {
  updateData()
}, { deep: true })

onMounted(() => {
  initChart()
  window.addEventListener('resize', handleResize)
})

onUnmounted(() => {
  window.removeEventListener('resize', handleResize)
  if (chart) {
    chart.remove()
    chart = null
  }
})
</script>

<template>
  <div class="w-full">
    <div v-if="title" class="text-sm font-medium mb-2" style="color: var(--text-secondary)">{{ title }}</div>
    <div ref="chartContainer" class="w-full rounded-lg overflow-hidden" style="background: var(--bg-secondary)"></div>
  </div>
</template>
