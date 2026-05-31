import { ref, onUnmounted, readonly } from 'vue'

interface WsMessage {
  type: string
  data: any
  timestamp: number
}

const ws = ref<WebSocket | null>(null)
const connected = ref(false)
const lastMessage = ref<WsMessage | null>(null)
const listeners = new Map<string, Set<(data: any) => void>>()
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let reconnectAttempts = 0
const MAX_RECONNECT_ATTEMPTS = 20
const BASE_RECONNECT_DELAY = 1000

function getWsUrl() {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${window.location.host}/api/v1/ws/stream`
}

function connect() {
  if (ws.value && (ws.value.readyState === WebSocket.CONNECTING || ws.value.readyState === WebSocket.OPEN)) {
    return
  }

  const url = getWsUrl()
  const socket = new WebSocket(url)

  socket.onopen = () => {
    connected.value = true
    reconnectAttempts = 0
    console.log('[WS] Connected to', url)
  }

  socket.onmessage = (event) => {
    try {
      const msg: WsMessage = JSON.parse(event.data)
      lastMessage.value = msg
      const handlerSet = listeners.get(msg.type)
      if (handlerSet) {
        handlerSet.forEach(handler => handler(msg.data))
      }
      const wildcardSet = listeners.get('*')
      if (wildcardSet) {
        wildcardSet.forEach(handler => handler(msg))
      }
    } catch (e) {
      console.warn('[WS] Failed to parse message', e)
    }
  }

  socket.onclose = () => {
    connected.value = false
    ws.value = null
    scheduleReconnect()
  }

  socket.onerror = (e) => {
    console.warn('[WS] Error', e)
  }

  ws.value = socket
}

function scheduleReconnect() {
  if (reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
    console.warn('[WS] Max reconnect attempts reached')
    return
  }
  if (reconnectTimer) clearTimeout(reconnectTimer)
  const delay = Math.min(BASE_RECONNECT_DELAY * Math.pow(1.5, reconnectAttempts), 30000)
  reconnectAttempts++
  console.log(`[WS] Reconnecting in ${Math.round(delay)}ms (attempt ${reconnectAttempts})`)
  reconnectTimer = setTimeout(() => connect(), delay)
}

function disconnect() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
    reconnectTimer = null
  }
  reconnectAttempts = MAX_RECONNECT_ATTEMPTS
  if (ws.value) {
    ws.value.close()
    ws.value = null
  }
  connected.value = false
}

function on(type: string, handler: (data: any) => void) {
  if (!listeners.has(type)) {
    listeners.set(type, new Set())
  }
  listeners.get(type)!.add(handler)
}

function off(type: string, handler: (data: any) => void) {
  const handlerSet = listeners.get(type)
  if (handlerSet) {
    handlerSet.delete(handler)
    if (handlerSet.size === 0) {
      listeners.delete(type)
    }
  }
}

export function useWebSocket() {
  onUnmounted(() => {
    off('*', () => {})
  })

  return {
    connected: readonly(connected),
    lastMessage: readonly(lastMessage),
    connect,
    disconnect,
    on,
    off,
  }
}
