<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue'
import api from '@/api'
import { MessageSquare, Send, Plus } from 'lucide-vue-next'

const sessions = ref<any[]>([])
const messages = ref<any[]>([])
const activeSession = ref<string | null>(null)
const input = ref('')
const loading = ref(true)
const sending = ref(false)
const messagesEl = ref<HTMLElement | null>(null)

async function loadSessions() {
  try {
    const { data } = await api.get('/chat/sessions')
    sessions.value = data.items || data.sessions || data || []
  } catch (e) {
    console.error('Failed to load sessions', e)
  } finally {
    loading.value = false
  }
}

async function loadMessages(sessionId: string) {
  activeSession.value = sessionId
  try {
    const { data } = await api.get(`/chat/sessions/${sessionId}/messages`)
    messages.value = data.items || data.messages || data || []
    await nextTick()
    scrollToBottom()
  } catch (e) {
    console.error('Failed to load messages', e)
  }
}

async function createSession() {
  try {
    const { data } = await api.post('/chat/sessions', { title: '新对话' })
    sessions.value.unshift(data)
    await loadMessages(data.id)
  } catch (e) {
    console.error('Create session failed', e)
  }
}

async function sendMessage() {
  if (!input.value.trim() || !activeSession.value) return
  const text = input.value
  input.value = ''
  messages.value.push({ role: 'user', content: text, created_at: new Date().toISOString() })
  sending.value = true
  await nextTick()
  scrollToBottom()
  try {
    const { data } = await api.post(`/chat/sessions/${activeSession.value}/messages`, { content: text })
    messages.value.push(data)
    await nextTick()
    scrollToBottom()
  } catch (e) {
    console.error('Send failed', e)
  } finally {
    sending.value = false
  }
}

function scrollToBottom() {
  if (messagesEl.value) messagesEl.value.scrollTop = messagesEl.value.scrollHeight
}

onMounted(() => { loadSessions() })
</script>

<template>
  <div class="space-y-6 h-[calc(100vh-8rem)]">
    <div class="flex items-center gap-3">
      <MessageSquare class="w-6 h-6" style="color: var(--gold)" />
      <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">AI 对话</h1>
    </div>

    <div class="flex gap-4 h-[calc(100%-3.5rem)]">
      <div class="w-64 flex-shrink-0 card flex flex-col">
        <button @click="createSession" class="btn-primary w-full flex items-center justify-center gap-2 mb-3">
          <Plus class="w-4 h-4" /> 新建会话
        </button>
        <div class="flex-1 overflow-y-auto space-y-1">
          <div v-if="loading" class="space-y-2">
            <div v-for="i in 3" :key="i" class="h-10 rounded-lg animate-pulse" style="background: var(--border)"></div>
          </div>
          <button v-for="s in sessions" :key="s.id" @click="loadMessages(s.id)"
            class="w-full text-left px-3 py-2 rounded-lg text-sm truncate transition-colors"
            :style="activeSession === s.id ? 'background: var(--gold-glow); color: var(--gold)' : 'color: var(--text-secondary)'"
            :class="activeSession !== s.id ? 'hover:bg-[#222839]' : ''">
            {{ s.title || '对话 ' + s.id.slice(0, 8) }}
          </button>
        </div>
      </div>

      <div class="flex-1 card flex flex-col">
        <div v-if="!activeSession" class="flex-1 flex items-center justify-center" style="color: var(--text-muted)">
          选择或创建一个会话开始对话
        </div>
        <template v-else>
          <div ref="messagesEl" class="flex-1 overflow-y-auto space-y-3 p-2">
            <div v-for="(m, i) in messages" :key="i" class="flex" :class="m.role === 'user' ? 'justify-end' : 'justify-start'">
              <div class="max-w-[70%] rounded-lg px-4 py-2.5 text-sm"
                :style="m.role === 'user' ? 'background: var(--bg-card); border: 1px solid var(--gold); color: var(--text-primary)' : 'background: var(--border); color: var(--text-secondary)'">
                {{ m.content }}
              </div>
            </div>
            <div v-if="sending" class="flex justify-start">
              <div class="rounded-lg px-4 py-2.5 text-sm animate-pulse" style="background: var(--border); color: var(--text-muted)">思考中...</div>
            </div>
          </div>
          <div class="flex gap-2 pt-3 border-t" style="border-color: var(--border)">
            <input v-model="input" @keyup.enter="sendMessage" class="input-field flex-1" placeholder="输入消息..." :disabled="sending" />
            <button @click="sendMessage" class="btn-primary flex items-center gap-2" :disabled="sending || !input.trim()">
              <Send class="w-4 h-4" />
            </button>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>
