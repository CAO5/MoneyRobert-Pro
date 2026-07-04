<script setup lang="ts">
import { ref, computed, onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import api from '@/api'
import { encryptPassword } from '@/utils/passwordCrypto'
import { Zap, Eye, EyeOff, Mail, KeyRound, ShieldCheck, ArrowLeft, CheckCircle2, AlertCircle } from 'lucide-vue-next'

const router = useRouter()

const step = ref<1 | 2>(1)
const email = ref('')
const code = ref('')
const newPassword = ref('')
const confirmPassword = ref('')
const showPassword = ref(false)
const sending = ref(false)
const resetting = ref(false)

// 轻量级本地 toast（项目未集成 toast 库，这里用浮层提示实现）
const toast = ref<{ message: string; type: 'success' | 'error' } | null>(null)
let toastTimer: ReturnType<typeof setTimeout> | undefined
let redirectTimer: ReturnType<typeof setTimeout> | undefined

function showToast(message: string, type: 'success' | 'error') {
  toast.value = { message, type }
  if (toastTimer) clearTimeout(toastTimer)
  toastTimer = setTimeout(() => {
    toast.value = null
  }, type === 'success' ? 2500 : 4000)
}

const canSend = computed(() => /\S+@\S+\.\S+/.test(email.value) && !sending.value)

const canReset = computed(
  () =>
    !!email.value &&
    /^\d{6}$/.test(code.value) &&
    newPassword.value.length >= 6 &&
    confirmPassword.value === newPassword.value &&
    !resetting.value,
)

function onCodeInput(e: Event) {
  const target = e.target as HTMLInputElement
  code.value = target.value.replace(/\D/g, '').slice(0, 6)
}

async function sendCode() {
  if (!canSend.value) return
  sending.value = true
  try {
    await api.post('/auth/password-reset/request', { email: email.value })
    step.value = 2
  } catch (e: any) {
    showToast(e instanceof Error ? e.message : '验证码发送失败，请稍后重试', 'error')
  } finally {
    sending.value = false
  }
}

async function resetPassword() {
  if (!canReset.value) return
  resetting.value = true
  let succeeded = false
  try {
    const encryptedPassword = encryptPassword(newPassword.value)
    await api.post('/auth/password-reset/confirm', {
      email: email.value,
      code: code.value,
      new_password: encryptedPassword,
    })
    succeeded = true
    showToast('密码重置成功', 'success')
    redirectTimer = setTimeout(() => router.push('/login'), 1500)
  } catch (e: any) {
    showToast(e instanceof Error ? e.message : '密码重置失败，请稍后重试', 'error')
  } finally {
    if (!succeeded) resetting.value = false
  }
}

onBeforeUnmount(() => {
  if (toastTimer) clearTimeout(toastTimer)
  if (redirectTimer) clearTimeout(redirectTimer)
})
</script>

<template>
  <div class="reset-container">
    <!-- 背景装饰 -->
    <div class="background-effects">
      <div class="deco-circle deco-circle-1"></div>
      <div class="deco-circle deco-circle-2"></div>
    </div>

    <!-- Toast 提示 -->
    <div
      v-if="toast"
      class="toast animate-slide-in-right"
      :class="toast.type === 'success' ? 'toast-success' : 'toast-error'"
    >
      <CheckCircle2 v-if="toast.type === 'success'" class="w-5 h-5 shrink-0" />
      <AlertCircle v-else class="w-5 h-5 shrink-0" />
      <span>{{ toast.message }}</span>
    </div>

    <!-- 重置卡片 -->
    <div class="reset-card card animate-fade-in-up">
      <!-- Logo 和标题 -->
      <div class="logo-section">
        <div class="logo-icon">
          <Zap class="w-8 h-8" />
        </div>
        <h1 class="logo-title">重置密码</h1>
        <p class="logo-subtitle">找回您的账户访问权限</p>
      </div>

      <!-- 步骤指示 -->
      <div class="step-indicator">
        <div class="step" :class="{ active: step >= 1, done: step >= 2 }">
          <span class="step-num">1</span>
          <span class="step-label">验证邮箱</span>
        </div>
        <div class="step-line" :class="{ active: step >= 2 }"></div>
        <div class="step" :class="{ active: step >= 2 }">
          <span class="step-num">2</span>
          <span class="step-label">设置新密码</span>
        </div>
      </div>

      <!-- 步骤 1：发送验证码 -->
      <form v-if="step === 1" @submit.prevent="sendCode" class="reset-form">
        <div class="form-group">
          <label class="label">邮箱</label>
          <div class="input-icon-wrapper">
            <Mail class="input-icon" />
            <input
              v-model="email"
              type="email"
              class="input input-with-icon"
              placeholder="请输入注册邮箱"
              required
              autocomplete="email"
            />
          </div>
        </div>

        <button
          type="submit"
          class="btn btn-primary submit-button"
          :disabled="!canSend"
        >
          <span v-if="!sending">发送验证码</span>
          <span v-else class="loading-content">
            <span class="spinner"></span>
            发送中...
          </span>
        </button>
      </form>

      <!-- 步骤 2：设置新密码 -->
      <form v-else @submit.prevent="resetPassword" class="reset-form">
        <div class="hint">
          验证码已发送至 <strong>{{ email }}</strong>
        </div>

        <div class="form-group">
          <label class="label">邮箱</label>
          <div class="input-icon-wrapper">
            <Mail class="input-icon" />
            <input
              v-model="email"
              type="email"
              class="input input-with-icon"
              placeholder="请输入注册邮箱"
              required
              autocomplete="email"
            />
          </div>
        </div>

        <div class="form-group">
          <label class="label">验证码</label>
          <div class="input-icon-wrapper">
            <KeyRound class="input-icon" />
            <input
              :value="code"
              @input="onCodeInput"
              type="text"
              inputmode="numeric"
              maxlength="6"
              class="input input-with-icon"
              placeholder="6 位验证码"
              required
            />
          </div>
        </div>

        <div class="form-group">
          <label class="label">新密码</label>
          <div class="password-input-wrapper">
            <input
              v-model="newPassword"
              :type="showPassword ? 'text' : 'password'"
              class="input password-input"
              placeholder="至少 6 个字符"
              required
              autocomplete="new-password"
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

        <div class="form-group">
          <label class="label">确认新密码</label>
          <div class="password-input-wrapper">
            <input
              v-model="confirmPassword"
              :type="showPassword ? 'text' : 'password'"
              class="input password-input"
              placeholder="请再次输入新密码"
              required
              autocomplete="new-password"
            />
            <ShieldCheck
              v-if="confirmPassword && confirmPassword === newPassword"
              class="confirm-check"
            />
          </div>
          <p v-if="confirmPassword && confirmPassword !== newPassword" class="field-error">
            两次输入的密码不一致
          </p>
        </div>

        <button
          type="submit"
          class="btn btn-primary submit-button"
          :disabled="!canReset"
        >
          <span v-if="!resetting">重置密码</span>
          <span v-else class="loading-content">
            <span class="spinner"></span>
            重置中...
          </span>
        </button>
      </form>

      <!-- 返回登录 -->
      <div class="back-link">
        <router-link to="/login" class="link-back">
          <ArrowLeft class="w-4 h-4" /> 返回登录
        </router-link>
      </div>
    </div>
  </div>
</template>

<style scoped>
.reset-container {
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

/* Toast */
.toast {
  position: fixed;
  top: 24px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 100;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 20px;
  border-radius: var(--radius-md);
  font-size: 14px;
  font-weight: 500;
  box-shadow: var(--shadow-lg);
  max-width: calc(100vw - 40px);
}

.toast-success {
  background: var(--profit-light);
  color: var(--profit);
  border: 1px solid var(--profit);
}

.toast-error {
  background: var(--loss-light);
  color: var(--loss);
  border: 1px solid var(--loss);
}

/* 重置卡片 */
.reset-card {
  width: 100%;
  max-width: 420px;
  padding: 48px 40px;
  position: relative;
  z-index: 1;
}

/* Logo 部分 */
.logo-section {
  text-align: center;
  margin-bottom: 28px;
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
  font-size: 30px;
  font-weight: 700;
  color: var(--text-primary);
  margin-bottom: 8px;
  letter-spacing: 1px;
}

.logo-subtitle {
  font-size: 14px;
  color: var(--text-secondary);
  font-weight: 400;
  letter-spacing: 1px;
}

/* 步骤指示 */
.step-indicator {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  margin-bottom: 28px;
}

.step {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text-muted);
  transition: color var(--transition-base) ease;
}

.step.active {
  color: var(--primary);
}

.step-num {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: var(--radius-full);
  font-size: 12px;
  font-weight: 600;
  background: var(--surface-tertiary);
  color: var(--text-muted);
  transition: all var(--transition-base) ease;
}

.step.active .step-num {
  background: var(--primary);
  color: var(--text-inverse);
}

.step.done .step-num {
  background: var(--profit);
  color: var(--text-inverse);
}

.step-label {
  font-size: 13px;
  font-weight: 500;
}

.step-line {
  width: 40px;
  height: 2px;
  background: var(--border);
  border-radius: var(--radius-full);
  transition: background var(--transition-base) ease;
}

.step-line.active {
  background: var(--primary);
}

/* 表单样式 */
.reset-form {
  margin-bottom: 24px;
}

.form-group {
  margin-bottom: 20px;
}

.hint {
  font-size: 13px;
  color: var(--text-secondary);
  background: var(--primary-bg);
  border: 1px solid var(--primary);
  border-radius: var(--radius-md);
  padding: 10px 14px;
  margin-bottom: 20px;
  text-align: center;
}

.hint strong {
  color: var(--primary);
  word-break: break-all;
}

/* 带图标输入 */
.input-icon-wrapper {
  position: relative;
}

.input-icon {
  position: absolute;
  left: 14px;
  top: 50%;
  transform: translateY(-50%);
  width: 18px;
  height: 18px;
  color: var(--text-muted);
  pointer-events: none;
}

.input-with-icon {
  padding-left: 42px;
}

/* 密码输入 */
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

.confirm-check {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%);
  width: 20px;
  height: 20px;
  color: var(--profit);
  pointer-events: none;
}

.field-error {
  margin-top: 6px;
  font-size: 12px;
  color: var(--loss);
}

/* 提交按钮 */
.submit-button {
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

/* 返回登录 */
.back-link {
  text-align: center;
  padding-top: 24px;
  border-top: 1px solid var(--border);
}

.link-back {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--text-secondary);
  font-size: 14px;
  font-weight: 500;
  transition: color var(--transition-fast) ease;
}

.link-back:hover {
  color: var(--primary);
}

/* 响应式设计 */
@media (max-width: 480px) {
  .reset-card {
    padding: 32px 24px;
  }

  .logo-title {
    font-size: 26px;
  }

  .step-label {
    display: none;
  }

  .step-line {
    width: 24px;
  }
}
</style>
