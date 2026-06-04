<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { Zap, Eye, EyeOff } from 'lucide-vue-next'

const router = useRouter()
const auth = useAuthStore()

const username = ref('')
const password = ref('')
const showPassword = ref(false)
const loading = ref(false)
const error = ref('')

async function handleLogin() {
  if (!username.value || !password.value) {
    error.value = '请输入用户名和密码'
    return
  }

  error.value = ''
  loading.value = true

  try {
    await auth.login(username.value, password.value)
    router.push('/dashboard')
  } catch (e: any) {
    error.value = e instanceof Error ? e.message : '登录失败，请检查用户名和密码'
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="login-container">
    <!-- 背景装饰 -->
    <div class="background-effects">
      <div class="deco-circle deco-circle-1"></div>
      <div class="deco-circle deco-circle-2"></div>
    </div>

    <!-- 登录卡片 -->
    <div class="login-card card animate-fade-in-up">
      <!-- Logo 和标题 -->
      <div class="logo-section">
        <div class="logo-icon">
          <Zap class="w-8 h-8" />
        </div>
        <h1 class="logo-title">MoneyRobert</h1>
        <p class="logo-subtitle">Professional Trading Platform</p>
      </div>

      <!-- 登录表单 -->
      <form @submit.prevent="handleLogin" class="login-form">
        <!-- 用户名输入 -->
        <div class="form-group">
          <label class="label">用户名</label>
          <input
            v-model="username"
            type="text"
            class="input"
            placeholder="请输入用户名或邮箱"
            required
            autocomplete="username"
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
              placeholder="请输入密码"
              required
              autocomplete="current-password"
            />
            <button
              type="button"
              class="password-toggle"
              @click="showPassword = !showPassword"
              tabindex="-1"
            >
              <Eye v-if="!showPassword" class="w-5 h-5" />
              <EyeOff v-else class="w-5 h-5" />
            </button>
          </div>
        </div>

        <!-- 错误提示 -->
        <div v-if="error" class="error-message animate-shake">
          {{ error }}
        </div>

        <!-- 登录按钮 -->
        <button
          type="submit"
          class="btn btn-primary login-button"
          :disabled="loading"
        >
          <span v-if="!loading">登 录</span>
          <span v-else class="loading-content">
            <span class="spinner"></span>
            登录中...
          </span>
        </button>
      </form>

      <!-- 注册链接 -->
      <div class="register-link">
        <span class="text-muted">还没有账户？</span>
        <router-link to="/register" class="link-primary">立即注册</router-link>
      </div>
    </div>

    <!-- 底部信息 -->
    <div class="footer-info animate-fade-in-up delay-500">
      <p class="copyright">© 2026 MoneyRobert Pro. All rights reserved.</p>
      <div class="security-badges">
        <span class="badge-item">🔒 安全加密</span>
        <span class="badge-item">⚡ 高性能</span>
        <span class="badge-item">🛡️ 企业级</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.login-container {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px 20px;
  position: relative;
  overflow: hidden;
  background: var(--surface-secondary);
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
  width: 600px;
  height: 600px;
  background: var(--primary);
  top: -200px;
  left: -200px;
  filter: blur(100px);
  animation: float 20s ease-in-out infinite;
}

.deco-circle-2 {
  width: 500px;
  height: 500px;
  background: var(--info);
  bottom: -150px;
  right: -150px;
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

/* 登录卡片 */
.login-card {
  width: 100%;
  max-width: 420px;
  padding: 48px 40px;
  position: relative;
  z-index: 1;
}

/* Logo 部分 */
.logo-section {
  text-align: center;
  margin-bottom: 40px;
}

.logo-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 64px;
  height: 64px;
  background: linear-gradient(135deg, var(--primary) 0%, var(--primary-dark) 100%);
  border-radius: var(--radius-lg);
  color: var(--text-inverse);
  margin-bottom: 20px;
  box-shadow: 0 8px 24px rgba(37, 99, 235, 0.25);
}

.logo-title {
  font-family: var(--font-sans);
  font-size: 36px;
  font-weight: 700;
  color: var(--text-primary);
  margin-bottom: 8px;
  letter-spacing: 1px;
}

.logo-subtitle {
  font-size: 14px;
  color: var(--text-secondary);
  font-weight: 400;
  letter-spacing: 2px;
  text-transform: uppercase;
}

/* 表单样式 */
.login-form {
  margin-bottom: 32px;
}

.form-group {
  margin-bottom: 24px;
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

/* 登录按钮 */
.login-button {
  width: 100%;
  padding: 14px 24px;
  font-size: 16px;
  font-weight: 600;
  margin-top: 8px;
}

.loading-content {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
}

/* 注册链接 */
.register-link {
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

/* 底部信息 */
.footer-info {
  margin-top: 40px;
  text-align: center;
  position: relative;
  z-index: 1;
}

.copyright {
  font-size: 13px;
  color: var(--text-muted);
  margin-bottom: 16px;
}

.security-badges {
  display: flex;
  gap: 20px;
  justify-content: center;
  flex-wrap: wrap;
}

.badge-item {
  font-size: 12px;
  color: var(--text-secondary);
  padding: 6px 12px;
  background: var(--surface);
  border-radius: var(--radius-full);
  border: 1px solid var(--border);
}

/* 响应式设计 */
@media (max-width: 480px) {
  .login-card {
    padding: 32px 24px;
  }

  .logo-title {
    font-size: 28px;
  }

  .security-badges {
    flex-direction: column;
    gap: 12px;
  }
}
</style>
