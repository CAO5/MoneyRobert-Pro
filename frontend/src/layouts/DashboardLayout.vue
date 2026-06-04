<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { useAppStore } from '@/stores/app'
import {
  LayoutDashboard, BarChart3, Brain, MessageSquare, Target,
  ArrowLeftRight, Settings, Bell, LogOut, ChevronLeft,
  TrendingUp, Newspaper, FileText, Bot, Shield, Zap, BookOpen,
  Cpu, Users, History
} from 'lucide-vue-next'

const router = useRouter()
const route = useRoute()
const auth = useAuthStore()
const app = useAppStore()

const navItems = [
  { icon: LayoutDashboard, label: '仪表盘', path: '/dashboard' },
  { icon: BarChart3, label: '市场数据', path: '/market' },
  { icon: Brain, label: 'AI 分析', path: '/ai' },
  { icon: MessageSquare, label: 'AI 对话', path: '/ai/chat' },
  { icon: Target, label: 'AI 预测', path: '/ai/predictions' },
  { icon: Cpu, label: 'Agent 系统', path: '/agent' },
  { icon: Users, label: '辩论分析', path: '/agent/debate' },
  { icon: History, label: '交易历史', path: '/agent/history' },
  { icon: ArrowLeftRight, label: '交易中心', path: '/trading' },
  { icon: TrendingUp, label: '策略管理', path: '/strategies' },
  { icon: Bot, label: '自动交易', path: '/auto-trading' },
  { icon: BookOpen, label: '模拟交易', path: '/paper-trading' },
  { icon: Newspaper, label: '新闻资讯', path: '/news' },
  { icon: FileText, label: '报告中心', path: '/reports' },
  { icon: Settings, label: '系统设置', path: '/settings' },
]

const adminItems = [
  { icon: Shield, label: '管理后台', path: '/admin' },
]

const isActive = (path: string) => {
  if (path === '/agent') {
    return route.path === '/agent' || route.path.startsWith('/agent/')
  }
  return route.path === path
}

function handleLogout() {
  auth.logout()
  router.push('/login')
}
</script>

<template>
  <div class="flex h-screen overflow-hidden" style="background: var(--bg-primary)">
    <aside
      class="flex flex-col border-r transition-all duration-300"
      :class="app.sidebarCollapsed ? 'w-[72px]' : 'w-[240px]'"
      style="background: var(--bg-card); border-color: var(--border)"
    >
      <div class="flex items-center h-16 px-4 border-b" style="border-color: var(--border)">
        <div class="flex items-center gap-3">
          <div class="w-8 h-8 rounded-lg flex items-center justify-center" style="background: linear-gradient(135deg, var(--gold), var(--gold-dim))">
            <Zap class="w-4 h-4" style="color: var(--bg-primary)" />
          </div>
          <span v-if="!app.sidebarCollapsed" class="font-display text-lg font-bold" style="color: var(--gold)">MoneyRobert</span>
        </div>
      </div>

      <nav class="flex-1 overflow-y-auto py-4 px-3">
        <div class="space-y-1">
          <router-link
            v-for="item in navItems"
            :key="item.path"
            :to="item.path"
            class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm transition-all duration-200 group"
            :class="isActive(item.path) ? '' : 'hover:bg-[#222839]'"
            :style="isActive(item.path) ? 'background: var(--gold-glow); color: var(--gold)' : 'color: var(--text-secondary)'"
          >
            <component :is="item.icon" class="w-5 h-5 flex-shrink-0" />
            <span v-if="!app.sidebarCollapsed">{{ item.label }}</span>
          </router-link>
        </div>

        <div v-if="auth.isAdmin" class="mt-6 pt-4 border-t" style="border-color: var(--border)">
          <div v-if="!app.sidebarCollapsed" class="px-3 mb-2 text-xs font-medium uppercase tracking-wider" style="color: var(--text-muted)">管理</div>
          <router-link
            v-for="item in adminItems"
            :key="item.path"
            :to="item.path"
            class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm transition-all duration-200"
            :class="isActive(item.path) ? '' : 'hover:bg-[#222839]'"
            :style="isActive(item.path) ? 'background: var(--gold-glow); color: var(--gold)' : 'color: var(--text-secondary)'"
          >
            <component :is="item.icon" class="w-5 h-5 flex-shrink-0" />
            <span v-if="!app.sidebarCollapsed">{{ item.label }}</span>
          </router-link>
        </div>
      </nav>

      <div class="border-t p-3" style="border-color: var(--border)">
        <button
          @click="handleLogout"
          class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm w-full transition-all duration-200 hover:bg-[#222839]"
          style="color: var(--text-secondary)"
        >
          <LogOut class="w-5 h-5 flex-shrink-0" />
          <span v-if="!app.sidebarCollapsed">退出登录</span>
        </button>
      </div>
    </aside>

    <div class="flex-1 flex flex-col overflow-hidden">
      <header class="flex items-center justify-between h-16 px-6 border-b" style="background: var(--bg-card); border-color: var(--border)">
        <button @click="app.toggleSidebar" class="p-2 rounded-lg transition-colors hover:bg-[#222839]" style="color: var(--text-secondary)">
          <ChevronLeft class="w-5 h-5" :class="{ 'rotate-180': app.sidebarCollapsed }" />
        </button>

        <div class="flex items-center gap-4">
          <router-link to="/notifications" class="relative p-2 rounded-lg transition-colors hover:bg-[#222839]" style="color: var(--text-secondary)">
            <Bell class="w-5 h-5" />
            <span v-if="app.notifications > 0" class="absolute -top-0.5 -right-0.5 w-4 h-4 rounded-full text-[10px] flex items-center justify-center font-bold" style="background: var(--loss); color: white">{{ app.notifications }}</span>
          </router-link>
          <div class="flex items-center gap-3">
            <div class="w-8 h-8 rounded-full flex items-center justify-center text-xs font-bold" style="background: var(--gold-glow); color: var(--gold)">
              {{ auth.user?.username?.charAt(0).toUpperCase() || 'U' }}
            </div>
            <span class="text-sm" style="color: var(--text-secondary)">{{ auth.user?.username }}</span>
          </div>
        </div>
      </header>

      <main class="flex-1 overflow-y-auto p-6">
        <router-view />
      </main>
    </div>
  </div>
</template>
