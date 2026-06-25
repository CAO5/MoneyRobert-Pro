<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import {
  LayoutDashboard, TrendingUp, LineChart, Bot, Settings, LogOut,
  Menu, X, ChevronDown, User, Bell, Wallet, Brain, MessageSquare,
  BookOpen, Newspaper, FileText, BarChart3, Shield, Target,
  ChevronRight, Swords, Layers, FlaskConical, Database,
  Gauge, ShieldAlert, Activity, Microscope,
  GitBranch, Network, Calculator, Repeat,
  BadgeCheck, GitCompare
} from 'lucide-vue-next'

const route = useRoute()
const router = useRouter()
const authStore = useAuthStore()
const sidebarCollapsed = ref(false)
const mobileMenuOpen = ref(false)
const userMenuOpen = ref(false)

const navigation = [
  {
    group: '概览',
    items: [
      { name: '仪表盘', path: '/dashboard', icon: LayoutDashboard },
    ]
  },
  {
    group: '交易',
    items: [
      { name: '交易中心', path: '/trading', icon: TrendingUp },
      { name: '策略管理', path: '/strategies', icon: Target },
      { name: '自动交易', path: '/auto-trading', icon: BarChart3 },
    ]
  },
  {
    group: '行情',
    items: [
      { name: '行情分析', path: '/market', icon: LineChart },
      { name: '新闻资讯', path: '/news', icon: Newspaper },
    ]
  },
  {
    group: 'AI',
    items: [
      { name: 'AI 分析', path: '/ai', icon: Brain },
      { name: 'AI 对话', path: '/ai/chat', icon: MessageSquare },
      { name: 'AI 预测', path: '/ai/predictions', icon: Wallet },
    ]
  },
  {
    group: '量化信号',
    items: [
      { name: '概率决策卡', path: '/signals/decision-card', icon: Layers },
      { name: '概率校准', path: '/signals/calibration', icon: Gauge },
      { name: '模型卡', path: '/signals/model-card', icon: BadgeCheck },
      { name: '回测中心', path: '/backtest', icon: FlaskConical },
      { name: '归因分析', path: '/backtest/attribution', icon: BarChart3 },
      { name: '反事实解释', path: '/backtest/counterfactual', icon: GitCompare },
      { name: '失效告警', path: '/backtest/strategy-failure', icon: ShieldAlert },
      { name: 'Walk-forward', path: '/backtest/walk-forward', icon: Repeat },
      { name: '组合风险', path: '/backtest/portfolio-risk', icon: Network },
      { name: 'Kelly 仓位', path: '/backtest/position-sizing', icon: Calculator },
    ]
  },
  {
    group: '数据监控',
    items: [
      { name: '微结构数据', path: '/microstructure', icon: Microscope },
      { name: '数据质量', path: '/data-quality', icon: Database },
      { name: '特征血缘', path: '/features/lineage', icon: GitBranch },
    ]
  },
  {
    group: 'Agent',
    items: [
      { name: 'Agent 仪表盘', path: '/agent', icon: Bot },
      { name: '模拟交易', path: '/paper-trading', icon: BookOpen },
      { name: 'AI 辩论', path: '/agent/debate', icon: Swords },
      { name: '交易历史', path: '/agent/history', icon: FileText },
    ]
  },
  {
    group: '系统',
    items: [
      { name: '通知中心', path: '/notifications', icon: Bell },
      { name: '报告中心', path: '/reports', icon: FileText },
      { name: '系统设置', path: '/settings', icon: Settings },
    ]
  },
]

// Flatten for active check
const allNavItems = navigation.flatMap(g => g.items)

const isActive = (path: string) => {
  if (path === '/dashboard') return route.path === '/' || route.path === '/dashboard'
  return route.path.startsWith(path)
}

const currentGroupName = computed(() => {
  const item = allNavItems.find(n => isActive(n.path))
  if (!item) return ''
  const group = navigation.find(g => g.items.includes(item))
  return group ? group.group : ''
})

const toggleSidebar = () => {
  sidebarCollapsed.value = !sidebarCollapsed.value
}

const handleLogout = async () => {
  await authStore.logout()
  router.push('/login')
}
</script>

<template>
  <div class="min-h-screen flex" style="background: var(--surface-secondary)">
    <!-- Sidebar -->
    <aside
      class="fixed inset-y-0 left-0 z-50 flex flex-col transition-all duration-300 overflow-hidden"
      :class="sidebarCollapsed ? 'w-[68px]' : 'w-60'"
      style="background: var(--surface); border-right: 1px solid var(--border)"
    >
      <!-- Logo -->
      <div class="h-14 flex items-center px-4 flex-shrink-0" style="border-bottom: 1px solid var(--border)">
        <div class="flex items-center gap-2.5 min-w-0">
          <div class="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0" style="background: var(--primary)">
            <Wallet class="w-4 h-4" style="color: var(--text-inverse)" />
          </div>
          <span v-if="!sidebarCollapsed" class="font-bold text-base truncate" style="color: var(--text-primary)">
            MoneyRobert
          </span>
        </div>
      </div>

      <!-- Navigation -->
      <nav class="flex-1 overflow-y-auto py-3 px-2.5 space-y-4">
        <div v-for="group in navigation" :key="group.group">
          <div v-if="!sidebarCollapsed" class="px-3 mb-1.5">
            <span class="text-[11px] font-semibold uppercase tracking-wider" style="color: var(--text-muted)">{{ group.group }}</span>
          </div>
          <div v-else class="flex justify-center mb-1">
            <div class="w-5 h-px" style="background: var(--border)"></div>
          </div>
          <div class="space-y-0.5">
            <router-link
              v-for="item in group.items"
              :key="item.path"
              :to="item.path"
              class="flex items-center gap-2.5 px-2.5 py-2 rounded-lg transition-all duration-150 group relative"
              :class="isActive(item.path) ? 'nav-item-active' : 'nav-item'"
            >
              <component
                :is="item.icon"
                class="w-[18px] h-[18px] flex-shrink-0"
                :style="{ color: isActive(item.path) ? 'var(--primary)' : 'var(--text-muted)' }"
              />
              <span
                v-if="!sidebarCollapsed"
                class="text-[13px] font-medium truncate"
                :style="{ color: isActive(item.path) ? 'var(--primary)' : 'var(--text-secondary)' }"
              >
                {{ item.name }}
              </span>
              <!-- Tooltip for collapsed sidebar -->
              <div
                v-if="sidebarCollapsed"
                class="absolute left-full ml-2 px-2 py-1 rounded text-xs font-medium whitespace-nowrap opacity-0 pointer-events-none group-hover:opacity-100 transition-opacity z-50"
                style="background: var(--text-primary); color: var(--text-inverse)"
              >
                {{ item.name }}
              </div>
            </router-link>
          </div>
        </div>
      </nav>

      <!-- Collapse Toggle -->
      <div class="p-2.5 flex-shrink-0" style="border-top: 1px solid var(--border)">
        <button
          @click="toggleSidebar"
          class="w-full flex items-center justify-center gap-2 px-2.5 py-2 rounded-lg transition-colors hover:bg-[var(--surface-tertiary)]"
          style="color: var(--text-muted)"
        >
          <Menu class="w-[18px] h-[18px]" />
          <span v-if="!sidebarCollapsed" class="text-[13px]">收起</span>
        </button>
      </div>
    </aside>

    <!-- Main Content -->
    <div
      class="flex-1 flex flex-col transition-all duration-300 min-w-0"
      :class="sidebarCollapsed ? 'ml-[68px]' : 'ml-60'"
    >
      <!-- Top Header -->
      <header
        class="h-14 flex items-center justify-between px-5 sticky top-0 z-40 flex-shrink-0"
        style="background: var(--surface); border-bottom: 1px solid var(--border)"
      >
        <!-- Mobile Menu Button -->
        <button
          class="lg:hidden p-2 rounded-lg hover:bg-[var(--surface-tertiary)]"
          style="color: var(--text-secondary)"
          @click="mobileMenuOpen = !mobileMenuOpen"
        >
          <Menu class="w-5 h-5" />
        </button>

        <!-- Breadcrumb -->
        <div class="hidden lg:flex items-center gap-2 text-sm">
          <span v-if="currentGroupName" style="color: var(--text-muted)">{{ currentGroupName }}</span>
          <ChevronRight v-if="currentGroupName" class="w-3.5 h-3.5" style="color: var(--border)" />
          <span class="font-medium" style="color: var(--text-primary)">
            {{ allNavItems.find(n => isActive(n.path))?.name || '仪表盘' }}
          </span>
        </div>

        <!-- Right Actions -->
        <div class="flex items-center gap-2">
          <!-- Notifications -->
          <router-link
            to="/notifications"
            class="relative p-2 rounded-lg transition-colors hover:bg-[var(--surface-tertiary)]"
            style="color: var(--text-secondary)"
          >
            <Bell class="w-[18px] h-[18px]" />
            <span class="absolute top-1.5 right-1.5 w-2 h-2 rounded-full" style="background: var(--loss)"></span>
          </router-link>

          <!-- User Menu -->
          <div class="relative">
            <button
              @click="userMenuOpen = !userMenuOpen"
              class="flex items-center gap-2 px-2.5 py-1.5 rounded-lg transition-colors hover:bg-[var(--surface-tertiary)]"
              :style="userMenuOpen ? 'background: var(--surface-tertiary)' : ''"
            >
              <div class="w-7 h-7 rounded-full flex items-center justify-center" style="background: var(--primary-bg); color: var(--primary)">
                <User class="w-3.5 h-3.5" />
              </div>
              <span class="hidden sm:block text-[13px] font-medium" style="color: var(--text-primary)">
                {{ authStore.user?.username || '用户' }}
              </span>
              <ChevronDown class="w-3.5 h-3.5" style="color: var(--text-muted)" />
            </button>

            <!-- Dropdown -->
            <Transition name="dropdown">
              <div
                v-if="userMenuOpen"
                class="absolute right-0 mt-1.5 w-48 py-1.5 rounded-lg shadow-lg z-50"
                style="background: var(--surface); border: 1px solid var(--border)"
                @click="userMenuOpen = false"
              >
                <div class="px-3 py-2" style="border-bottom: 1px solid var(--border)">
                  <div class="text-sm font-medium" style="color: var(--text-primary)">{{ authStore.user?.username || '用户' }}</div>
                  <div class="text-xs" style="color: var(--text-muted)">{{ authStore.user?.email || '' }}</div>
                </div>
                <div class="py-1">
                  <router-link
                    to="/settings"
                    class="flex items-center gap-2 px-3 py-2 text-sm transition-colors hover:bg-[var(--surface-tertiary)]"
                    style="color: var(--text-secondary)"
                  >
                    <Settings class="w-4 h-4" />
                    系统设置
                  </router-link>
                  <router-link
                    v-if="authStore.isAdmin"
                    to="/admin"
                    class="flex items-center gap-2 px-3 py-2 text-sm transition-colors hover:bg-[var(--surface-tertiary)]"
                    style="color: var(--text-secondary)"
                  >
                    <Shield class="w-4 h-4" />
                    管理后台
                  </router-link>
                </div>
                <div style="border-top: 1px solid var(--border)" class="py-1">
                  <button
                    @click="handleLogout"
                    class="w-full flex items-center gap-2 px-3 py-2 text-sm transition-colors hover:bg-[var(--loss-light)]"
                    style="color: var(--loss)"
                  >
                    <LogOut class="w-4 h-4" />
                    退出登录
                  </button>
                </div>
              </div>
            </Transition>
          </div>
        </div>
      </header>

      <!-- Page Content -->
      <main class="flex-1 p-5 overflow-auto">
        <router-view v-slot="{ Component }">
          <Transition name="page" mode="out-in">
            <component :is="Component" />
          </Transition>
        </router-view>
      </main>
    </div>

    <!-- Mobile Sidebar Overlay -->
    <Transition name="fade">
      <div
        v-if="mobileMenuOpen"
        class="fixed inset-0 z-40 lg:hidden"
        style="background: rgba(0, 0, 0, 0.5)"
        @click="mobileMenuOpen = false"
      ></div>
    </Transition>

    <!-- Mobile Sidebar -->
    <Transition name="slide">
      <aside
        v-if="mobileMenuOpen"
        class="fixed inset-y-0 left-0 z-50 w-60 lg:hidden overflow-y-auto"
        style="background: var(--surface); border-right: 1px solid var(--border)"
      >
        <div class="h-14 flex items-center justify-between px-4" style="border-bottom: 1px solid var(--border)">
          <span class="font-bold text-base" style="color: var(--text-primary)">MoneyRobert</span>
          <button @click="mobileMenuOpen = false" style="color: var(--text-muted)">
            <X class="w-5 h-5" />
          </button>
        </div>
        <nav class="p-3 space-y-4">
          <div v-for="group in navigation" :key="group.group">
            <div class="px-2.5 mb-1.5">
              <span class="text-[11px] font-semibold uppercase tracking-wider" style="color: var(--text-muted)">{{ group.group }}</span>
            </div>
            <div class="space-y-0.5">
              <router-link
                v-for="item in group.items"
                :key="item.path"
                :to="item.path"
                class="flex items-center gap-2.5 px-2.5 py-2 rounded-lg"
                :class="isActive(item.path) ? 'nav-item-active' : 'nav-item'"
                @click="mobileMenuOpen = false"
              >
                <component :is="item.icon" class="w-[18px] h-[18px]" />
                <span class="text-[13px] font-medium">{{ item.name }}</span>
              </router-link>
            </div>
          </div>
        </nav>
      </aside>
    </Transition>
  </div>
</template>

<style scoped>
.nav-item {
  color: var(--text-secondary);
}
.nav-item:hover {
  background: var(--surface-tertiary);
  color: var(--text-primary);
}
.nav-item-active {
  background: var(--primary-bg);
  color: var(--primary);
}

/* Page Transition */
.page-enter-active,
.page-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.page-enter-from {
  opacity: 0;
  transform: translateY(6px);
}
.page-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

/* Dropdown Transition */
.dropdown-enter-active,
.dropdown-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

/* Fade Transition */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* Slide Transition */
.slide-enter-active,
.slide-leave-active {
  transition: transform 0.25s ease;
}
.slide-enter-from,
.slide-leave-to {
  transform: translateX(-100%);
}

/* Scrollbar */
nav::-webkit-scrollbar { width: 4px; }
nav::-webkit-scrollbar-track { background: transparent; }
nav::-webkit-scrollbar-thumb { background: var(--border); border-radius: 4px; }
nav::-webkit-scrollbar-thumb:hover { background: var(--border-hover); }
</style>
