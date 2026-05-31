<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { Zap, Eye, EyeOff } from 'lucide-vue-next'

const router = useRouter()
const auth = useAuthStore()

const username = ref('')
const email = ref('')
const password = ref('')
const showPassword = ref(false)
const loading = ref(false)
const error = ref('')

async function handleRegister() {
  error.value = ''
  loading.value = true
  try {
    await auth.register(username.value, email.value, password.value)
    await auth.login(username.value, password.value)
    router.push('/dashboard')
  } catch (e: any) {
    error.value = e.response?.data?.error?.message || '注册失败，请稍后重试'
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="min-h-screen flex items-center justify-center relative overflow-hidden" style="background: var(--bg-primary)">
    <div class="absolute inset-0 overflow-hidden">
      <div class="absolute top-1/4 right-1/4 w-96 h-96 rounded-full opacity-5" style="background: var(--gold); filter: blur(120px)"></div>
    </div>

    <div class="relative w-full max-w-md px-6">
      <div class="card-gold">
        <div class="flex flex-col items-center mb-8">
          <div class="w-14 h-14 rounded-xl flex items-center justify-center mb-4" style="background: linear-gradient(135deg, var(--gold), var(--gold-dim))">
            <Zap class="w-7 h-7" style="color: var(--bg-primary)" />
          </div>
          <h1 class="font-display text-2xl font-bold" style="color: var(--gold)">创建账户</h1>
          <p class="text-sm mt-1" style="color: var(--text-secondary)">开始您的专业投资之旅</p>
        </div>

        <form @submit.prevent="handleRegister" class="space-y-5">
          <div>
            <label class="block text-sm font-medium mb-2" style="color: var(--text-secondary)">用户名</label>
            <input v-model="username" type="text" class="input-field" placeholder="3-50个字符" required />
          </div>
          <div>
            <label class="block text-sm font-medium mb-2" style="color: var(--text-secondary)">邮箱</label>
            <input v-model="email" type="email" class="input-field" placeholder="your@email.com" required />
          </div>
          <div>
            <label class="block text-sm font-medium mb-2" style="color: var(--text-secondary)">密码</label>
            <div class="relative">
              <input v-model="password" :type="showPassword ? 'text' : 'password'" class="input-field pr-10" placeholder="至少6个字符" required />
              <button type="button" @click="showPassword = !showPassword" class="absolute right-3 top-1/2 -translate-y-1/2" style="color: var(--text-muted)">
                <Eye v-if="!showPassword" class="w-4 h-4" />
                <EyeOff v-else class="w-4 h-4" />
              </button>
            </div>
          </div>

          <div v-if="error" class="p-3 rounded-lg text-sm" style="background: rgba(248,113,113,0.1); color: var(--loss)">
            {{ error }}
          </div>

          <button type="submit" class="btn-primary w-full py-3" :disabled="loading">
            {{ loading ? '注册中...' : '注册' }}
          </button>
        </form>

        <div class="mt-6 text-center">
          <span class="text-sm" style="color: var(--text-muted)">已有账户？</span>
          <router-link to="/login" class="text-sm font-medium ml-1" style="color: var(--gold)">立即登录</router-link>
        </div>
      </div>
    </div>
  </div>
</template>
