<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { createChart, CandlestickSeries, HistogramSeries, ColorType, CrosshairMode } from 'lightweight-charts'
import type { IChartApi } from 'lightweight-charts'

const props = defineProps<{
  data: Array<{ time: number; open: number; high: number; low: number; close: number; volume?: number }>
  height?: number
  showVolume?: boolean
}>()

const chartContainer = ref<HTMLDivElement>()
let chart: IChartApi | null = null
let candleSeries: any = null
let volumeSeries: any = null

function initChart() {
  if (!chartContainer.value) return

  chart = createChart(chartContainer.value, {
    width: chartContainer.value.clientWidth,
    height: props.height || 500,
    layout: {
      background: { type: ColorType.Solid, color: '#111622' },
      textColor: '#94A3B8',
      fontSize: 12,
    },
    grid: {
      vertLines: { color: 'rgba(255, 255, 255, 0.04)' },
      horzLines: { color: 'rgba(255, 255, 255, 0.04)' },
    },
    crosshair: {
      mode: CrosshairMode.Normal,
      vertLine: { color: 'rgba(212, 175, 55, 0.3)', labelBackgroundColor: '#D4AF37' },
      horzLine: { color: 'rgba(212, 175, 55, 0.3)', labelBackgroundColor: '#D4AF37' },
    },
    rightPriceScale: {
      borderColor: 'rgba(255, 255, 255, 0.1)',
      scaleMargins: { top: 0.1, bottom: props.showVolume ? 0.3 : 0.1 },
    },
    timeScale: {
      borderColor: 'rgba(255, 255, 255, 0.1)',
      timeVisible: true,
      secondsVisible: false,
      fixLeftEdge: true,
      fixRightEdge: true,
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

  candleSeries = chart.addSeries(CandlestickSeries, {
    upColor: '#00C853',
    downColor: '#FF1744',
    borderUpColor: '#00C853',
    borderDownColor: '#FF1744',
    wickUpColor: '#00C853',
    wickDownColor: '#FF1744',
  })

  if (props.showVolume) {
    volumeSeries = chart.addSeries(HistogramSeries, {
      priceFormat: { type: 'volume' },
      priceScaleId: 'volume',
    })
    chart.priceScale('volume').applyOptions({
      scaleMargins: { top: 0.8, bottom: 0 },
    })
  }

  updateData()
}

function updateData() {
  if (!candleSeries || !props.data.length) return

  // Sort by time and deduplicate (lightweight-charts requires strictly ascending time)
  const sortedData = [...props.data].sort((a, b) => a.time - b.time)
  const dedupedData = sortedData.filter((d, i) => i === 0 || d.time !== sortedData[i - 1].time)

  const candleData = dedupedData.map(d => ({
    time: d.time,
    open: d.open,
    high: d.high,
    low: d.low,
    close: d.close,
  }))

  candleSeries.setData(candleData)

  if (volumeSeries && props.showVolume) {
    const volData = dedupedData
      .filter(d => d.volume !== undefined)
      .map(d => ({
        time: d.time,
        value: d.volume!,
        color: d.close >= d.open ? 'rgba(0, 200, 83, 0.3)' : 'rgba(255, 23, 68, 0.3)',
      }))
    if (volData.length) volumeSeries.setData(volData)
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
  <div ref="chartContainer" class="w-full rounded-lg overflow-hidden" style="background: var(--bg-secondary)"></div>
</template>
