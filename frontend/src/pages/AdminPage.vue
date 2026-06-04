<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { Shield, Users, UserCog } from 'lucide-vue-next'

const stats = ref({ total_users: 0, active_users: 0, total_trades: 0, total_pnl: 0 })
const users = ref<any[]>([])
const loading = ref(true)

onMounted(async () => {
  try {
    const [statRes, userRes] = await Promise.all([api.get('/admin/stats'), api.get('/admin/users')])
    stats.value = statRes.data.stats || statRes.data || stats.value
    users.value = userRes.data.items || userRes.data.users || userRes.data || []
  } catch (e) {
    console.error('Failed to load admin data', e)
  } finally {
    loading.value = false
  }
})

async function toggleActive(id: string) {
  try {
    await api.post(`/admin/users/${id}/toggle-active`)
    const u = users.value.find((u: any) => u.id === id)
    if (u) u.is_active = !u.is_active
  } catch (e) {
    console.error('Toggle failed', e)
  }
}

function roleBadge(r: string) {
  if (r === 'admin') return 'badge-primary'
  return 'badge-neutral'
}
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center gap-3">
      <Shield class="w-6 h-6" style="color: var(--primary)" />
      <h1 class="text-2xl font-bold" style="color: var(--text-primary)">管理后台</h1>
    </div>

    <div v-if="loading" class="grid grid-cols-4 gap-4">
      <div v-for="i in 4" :key="i" class="card animate-pulse h-24"></div>
    </div>

    <template v-else>
      <div class="grid grid-cols-4 gap-4">
        <div class="card">
          <div class="flex items-center gap-2 mb-2"><Users class="w-4 h-4" style="color: var(--primary)" /><span class="text-sm" style="color: var(--text-secondary)">总用户</span></div>
          <div class="stat-value" style="color: var(--text-primary)">{{ stats.total_users }}</div>
        </div>
        <div class="card">
          <div class="text-sm mb-2" style="color: var(--text-secondary)">活跃用户</div>
          <div class="stat-value" style="color: var(--profit)">{{ stats.active_users }}</div>
        </div>
        <div class="card">
          <div class="text-sm mb-2" style="color: var(--text-secondary)">总交易数</div>
          <div class="stat-value" style="color: var(--text-primary)">{{ stats.total_trades }}</div>
        </div>
        <div class="card">
          <div class="text-sm mb-2" style="color: var(--text-secondary)">总盈亏</div>
          <div class="stat-value" :style="{ color: stats.total_pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">${{ stats.total_pnl?.toLocaleString('en-US', { minimumFractionDigits: 2 }) }}</div>
        </div>
      </div>

      <div class="card">
        <div class="flex items-center gap-2 mb-4">
          <UserCog class="w-5 h-5" style="color: var(--primary)" />
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">用户管理</h2>
        </div>
        <div v-if="users.length === 0" class="py-8 text-center" style="color: var(--text-muted)">暂无用户</div>
        <table v-else class="w-full">
          <thead>
            <tr class="text-xs uppercase" style="color: var(--text-muted)">
              <th class="text-left py-3 font-medium">用户名</th>
              <th class="text-left py-3 font-medium">邮箱</th>
              <th class="text-left py-3 font-medium">角色</th>
              <th class="text-left py-3 font-medium">状态</th>
              <th class="text-right py-3 font-medium">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="u in users" :key="u.id" class="border-t" style="border-color: var(--border)">
              <td class="py-3 font-medium" style="color: var(--text-primary)">{{ u.username }}</td>
              <td class="py-3" style="color: var(--text-secondary)">{{ u.email }}</td>
              <td class="py-3"><span class="badge" :class="roleBadge(u.role)">{{ u.role }}</span></td>
              <td class="py-3"><span class="badge" :class="u.is_active ? 'badge-profit' : 'badge-loss'">{{ u.is_active ? '活跃' : '停用' }}</span></td>
              <td class="py-3 text-right">
                <button @click="toggleActive(u.id)" class="btn-secondary text-xs px-3 py-1">
                  {{ u.is_active ? '停用' : '启用' }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </div>
</template>
