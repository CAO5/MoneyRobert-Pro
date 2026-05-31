<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { Bell, CheckCheck } from 'lucide-vue-next'

const notifications = ref<any[]>([])
const loading = ref(true)

async function loadNotifications() {
  try {
    const { data } = await api.get('/notifications')
    notifications.value = data.items || data.notifications || data || []
  } catch (e) {
    console.error('Failed to load notifications', e)
  } finally {
    loading.value = false
  }
}

async function markRead(id: string) {
  try {
    await api.put(`/notifications/${id}/read`)
    const n = notifications.value.find((n: any) => n.id === id)
    if (n) n.is_read = true
  } catch (e) {
    console.error('Mark read failed', e)
  }
}

async function markAllRead() {
  try {
    await api.put('/notifications/read-all')
    notifications.value.forEach((n: any) => { n.is_read = true })
  } catch (e) {
    console.error('Mark all read failed', e)
  }
}

function typeBadge(t: string) {
  if (t === 'trade' || t === 'order') return 'badge-profit'
  if (t === 'alert' || t === 'risk') return 'badge-loss'
  if (t === 'system') return 'badge-gold'
  return 'badge-neutral'
}

onMounted(() => { loadNotifications() })
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Bell class="w-6 h-6" style="color: var(--gold)" />
        <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">通知中心</h1>
      </div>
      <button @click="markAllRead" class="btn-secondary flex items-center gap-2">
        <CheckCheck class="w-4 h-4" /> 全部已读
      </button>
    </div>

    <div v-if="loading" class="space-y-3">
      <div v-for="i in 5" :key="i" class="card animate-pulse h-20"></div>
    </div>

    <div v-else-if="notifications.length === 0" class="card py-12 text-center" style="color: var(--text-muted)">暂无通知</div>

    <div v-else class="space-y-3">
      <div v-for="n in notifications" :key="n.id" class="card flex items-start justify-between gap-4"
        :style="n.is_read ? '' : 'border-left: 3px solid var(--gold)'">
        <div class="flex-1 space-y-1">
          <div class="flex items-center gap-2">
            <h3 class="font-semibold" :style="{ color: n.is_read ? 'var(--text-secondary)' : 'var(--text-primary)' }">{{ n.title }}</h3>
            <span class="badge" :class="typeBadge(n.type)">{{ n.type }}</span>
          </div>
          <p class="text-sm" style="color: var(--text-muted)">{{ n.content }}</p>
          <span class="text-xs" style="color: var(--text-muted)">{{ new Date(n.created_at).toLocaleString('zh-CN') }}</span>
        </div>
        <button v-if="!n.is_read" @click="markRead(n.id)" class="p-2 rounded-lg transition-colors hover:bg-[#222839] flex-shrink-0" style="color: var(--gold)">
          <CheckCheck class="w-4 h-4" />
        </button>
      </div>
    </div>
  </div>
</template>
