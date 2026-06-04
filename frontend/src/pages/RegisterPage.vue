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
  <div class="register-container">
    <!-- 背景装饰 -->
    <div class="background-effects">
      <div class="deco-circle deco-circle-1"></div>
      <div class="deco-circle deco-circle-2"></div>
    </div>

    <!-- 注册卡片 -->
    <div class="register-card card animate-fade-in-up">
      <!-- Logo 和标题 -->
      <div class="logo-section">
        <div class="logo-icon">
          <Zap class="w-7 h-7" />
        </div>
        <h1 class="logo-title">创建账户</h1>
        <p class="logo-subtitle">开始您的专业投资之旅</p>
      </div>

      <!-- 注册表单 -->
      <form @submit.prevent="handleRegister" class="register-form">
        <!-- 用户名输入 -->
        <div class="form-group">
          <label class="label">用户名</label>
          <input
            v-model="username"
            type="text"
            class="input"
            placeholder="3-50个字符"
            required
          />
        </div>

        <!-- 邮箱输入 -->
        <div class="form-group">
          <label class="label">邮箱</label>
          <input
            v-model="email"
            type="email"
            class="input"
            placeholder="your@email.com"
            required
          />
        </div>

        <!-- 密码输入 -->
        <div class="form-group">
          <label class="label">密码</label>
          <div class="password-input-wrapper">
            <input
              v-model="password"
              :type="showPassword ? 'text' : 'password'"
              class="input password-input"
              placeholder="至少6个字符"
              required
            />
            <button
              type="button"
              class="password-toggle"
              @click="showPassword = !showPassword"
              tabindex="-1"
            >
              <Eye v-if="!showPassword" class="w-4 h-4" />
              <EyeOff v-else class="w-4 h-4" />
            </button>
          </div>
        </div>

        <!-- 错误提示 -->
        <div v-if="error" class="error-message">
          {{ error }}
        </div>

        <!-- 注册按钮 -->
        <button
          type="submit"
          class="btn btn-primary register-button"
          :disabled="loading"
        >
          <span v-if="!loading">注册</span>
          <span v-else class="loading-content">
            <span class="spinner"></span>
            注册中...
          </span>
        </button>
      </form>

      <!-- 登录链接 -->
      <div class="login-link">
        <span class="text-muted">已有账户？</span>
        <router-link to="/login" class="link-primary">立即登录</router-link>
      </div>
    </div>
  </div>
</template>

<style scoped>
.register-container {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
  background: var(--surface-secondary);
  padding: 40px 20px;
}

/* 背景装饰 */
.background-effects {
  position: absolute;
  inset: 0;
  pointer-events: none;
  overflow: hidden;
}

.deco-circle {
  position: absolute;
  border-radius: 50%;
  opacity: 0.08;
}

.deco-circle-1 {
  width: 500px;
  height: 500px;
  background: var(--primary);
  top: -150px;
  right: -150px;
  filter: blur(100px);
  animation: float 20s ease-in-out infinite;
}

.deco-circle-2 {
  width: 400px;
  height: 400px;
  background: var(--info);
  bottom: -100px;
  left: -100px;
  filter: blur(100px);
  animation: float 25s ease-in-out infinite reverse;
}

@keyframes float {
  0%, 100% {
    transform: translate(0, 0);
  }
  50% {
    transform: translate(30px, 30px);
  }
}

/* 注册卡片 */
.register-card {
  width: 100%;
  max-width: 420px;
  padding: 48px 40px;
  position: relative;
  z-index: 1;
}

/* Logo 部分 */
.logo-section {
  text-align: center;
  margin-bottom: 32px;
}

.logo-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  background: linear-gradient(135deg, var(--primary) 0%, var(--primary-dark) 100%);
  border-radius: var(--radius-lg);
  color: var(--text-inverse);
  margin-bottom: 16px;
  box-shadow: 0 8px 24px rgba(37, 99, 235, 0.25);
}

.logo-title {
  font-family: var(--font-sans);
  font-size: 24px;
  font-weight: 700;
  color: var(--text-primary);
  margin-bottom: 4px;
}

.logo-subtitle {
  font-size: 14px;
  color: var(--text-secondary);
  font-weight: 400;
}

/* 表单样式 */
.register-form {
  margin-bottom: 24px;
}

.form-group {
  margin-bottom: 20px;
}

.password-input-wrapper {
  position: relative;
}

.password-input {
  padding-right: 48px;
}

.password-toggle {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%);
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color var(--transition-fast) ease;
}

.password-toggle:hover {
  color: var(--primary);
}

/* 错误消息 */
.error-message {
  padding: 12px 16px;
  background: var(--loss-light);
  border: 1px solid var(--loss);
  border-radius: var(--radius-md);
  color: var(--loss);
  font-size: 14px;
  margin-bottom: 20px;
}

/* 注册按钮 */
.register-button {
  width: 100%;
  padding: 14px 24px;
  font-size: 16px;
  font-weight: 600;
}

.loading-content {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
}

/* 登录链接 */
.login-link {
  text-align: center;
  padding-top: 24px;
  border-top: 1px solid var(--border);
}

.text-muted {
  color: var(--text-muted);
  font-size: 14px;
}

.link-primary {
  color: var(--primary);
  font-weight: 500;
  margin-left: 8px;
  transition: all var(--transition-fast) ease;
}

.link-primary:hover {
  color: var(--primary-dark);
  text-decoration: underline;
}

/* 响应式设计 */
@media (max-width: 480px) {
  .register-card {
    padding: 32px 24px;
  }
}
</style>
