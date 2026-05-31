import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { AgentApi, type AiSimulationConfig, type AiSimulationTrade, type DebateSession, type LevelResponse } from '@/api'

export const useAgentStore = defineStore('agent', () => {
  const currentConfig = ref<AiSimulationConfig | null>(null)
  const debateSession = ref<DebateSession | null>(null)
  const trades = ref<AiSimulationTrade[]>([])
  const levelInfo = ref<LevelResponse | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  const wsConnection = ref<WebSocket | null>(null)
  const wsConnected = ref(false)
  const wsMessages = ref<any[]>([])

  const hasConfig = computed(() => currentConfig.value !== null)
  const isRunning = computed(() => currentConfig.value?.status === 'running')
  const isAutonomous = computed(() => currentConfig.value?.autonomous_mode_enabled === true)
  const hasActiveDebate = computed(() => debateSession.value !== null && debateSession.value.status === 'in_progress')

  async function fetchSimulationStatus() {
    loading.value = true
    error.value = null
    try {
      const response = await AgentApi.getSimulationStatus()
      currentConfig.value = response.config
    } catch (err) {
      error.value = err instanceof Error ? err.message : '获取模拟状态失败'
    } finally {
      loading.value = false
    }
  }

  async function startSimulation(symbol: string, initialBalance?: number) {
    loading.value = true
    error.value = null
    try {
      const response = await AgentApi.startSimulation({ symbol, initial_balance: initialBalance })
      await fetchSimulationStatus()
      return response
    } catch (err) {
      error.value = err instanceof Error ? err.message : '启动模拟失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function stopSimulation() {
    if (!currentConfig.value) return
    loading.value = true
    error.value = null
    try {
      await AgentApi.stopSimulation(currentConfig.value.id)
      await fetchSimulationStatus()
    } catch (err) {
      error.value = err instanceof Error ? err.message : '停止模拟失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function fetchTrades() {
    loading.value = true
    error.value = null
    try {
      const response = await AgentApi.getTrades()
      trades.value = response.trades
    } catch (err) {
      error.value = err instanceof Error ? err.message : '获取交易记录失败'
    } finally {
      loading.value = false
    }
  }

  async function fetchStats() {
    loading.value = true
    error.value = null
    try {
      const response = await AgentApi.getStats()
      currentConfig.value = response.config
    } catch (err) {
      error.value = err instanceof Error ? err.message : '获取统计数据失败'
    } finally {
      loading.value = false
    }
  }

  async function fetchLevel() {
    loading.value = true
    error.value = null
    try {
      const response = await AgentApi.getLevel()
      levelInfo.value = response
    } catch (err) {
      error.value = err instanceof Error ? err.message : '获取等级信息失败'
    } finally {
      loading.value = false
    }
  }

  async function startDebate(symbol: string, configId?: string) {
    loading.value = true
    error.value = null
    try {
      const response = await AgentApi.startDebate({ symbol, config_id: configId })
      await fetchDebateSession(response.session_id)
      return response
    } catch (err) {
      error.value = err instanceof Error ? err.message : '启动辩论失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function fetchDebateSession(sessionId: string) {
    loading.value = true
    error.value = null
    try {
      const session = await AgentApi.getDebateSession(sessionId)
      debateSession.value = session
    } catch (err) {
      error.value = err instanceof Error ? err.message : '获取辩论会话失败'
    } finally {
      loading.value = false
    }
  }

  async function approvePromotion(auditId: string, reviewComment?: string) {
    loading.value = true
    error.value = null
    try {
      const response = await AgentApi.approvePromotion({ audit_id: auditId, review_comment: reviewComment })
      currentConfig.value = response.config
      await fetchLevel()
      return response
    } catch (err) {
      error.value = err instanceof Error ? err.message : '审批升级失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function signRiskConfirmation(version: string, maxAcceptableLoss: number, configId?: string, acceptReason?: string) {
    loading.value = true
    error.value = null
    try {
      const response = await AgentApi.signRiskConfirmation({
        config_id: configId,
        version,
        max_acceptable_loss: maxAcceptableLoss,
        accept_reason: acceptReason
      })
      await fetchSimulationStatus()
      return response
    } catch (err) {
      error.value = err instanceof Error ? err.message : '签署风险确认失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function startAutonomous() {
    if (!currentConfig.value) return
    loading.value = true
    error.value = null
    try {
      await AgentApi.startAutonomous(currentConfig.value.id)
      await fetchSimulationStatus()
    } catch (err) {
      error.value = err instanceof Error ? err.message : '启动自主交易失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function stopAutonomous() {
    if (!currentConfig.value) return
    loading.value = true
    error.value = null
    try {
      await AgentApi.stopAutonomous(currentConfig.value.id)
      await fetchSimulationStatus()
    } catch (err) {
      error.value = err instanceof Error ? err.message : '停止自主交易失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function emergencyStop() {
    if (!currentConfig.value) return
    loading.value = true
    error.value = null
    try {
      await AgentApi.emergencyStop(currentConfig.value.id)
      await fetchSimulationStatus()
    } catch (err) {
      error.value = err instanceof Error ? err.message : '紧急停止失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  function connectWebSocket() {
    if (wsConnection.value) return

    const wsUrl = `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/api/v1/agent/ws`
    wsConnection.value = new WebSocket(wsUrl)

    wsConnection.value.onopen = () => {
      wsConnected.value = true
    }

    wsConnection.value.onmessage = (event) => {
      try {
        const message = JSON.parse(event.data)
        wsMessages.value.push(message)
        handleWebSocketMessage(message)
      } catch {
        console.error('Failed to parse WebSocket message')
      }
    }

    wsConnection.value.onclose = () => {
      wsConnected.value = false
      wsConnection.value = null
    }

    wsConnection.value.onerror = (err) => {
      console.error('WebSocket error:', err)
      wsConnected.value = false
    }
  }

  function disconnectWebSocket() {
    if (wsConnection.value) {
      wsConnection.value.close()
      wsConnection.value = null
      wsConnected.value = false
    }
  }

  function handleWebSocketMessage(message: any) {
    switch (message.type) {
      case 'agent_update':
        if (debateSession.value?.id === message.data.session_id) {
          fetchDebateSession(message.data.session_id)
        }
        break
      case 'trade_update':
        fetchTrades()
        break
      case 'config_update':
        fetchSimulationStatus()
        break
    }
  }

  function clearError() {
    error.value = null
  }

  function resetStore() {
    currentConfig.value = null
    debateSession.value = null
    trades.value = []
    levelInfo.value = null
    loading.value = false
    error.value = null
    disconnectWebSocket()
    wsMessages.value = []
  }

  return {
    currentConfig,
    debateSession,
    trades,
    levelInfo,
    loading,
    error,
    wsConnection,
    wsConnected,
    wsMessages,
    hasConfig,
    isRunning,
    isAutonomous,
    hasActiveDebate,
    fetchSimulationStatus,
    startSimulation,
    stopSimulation,
    fetchTrades,
    fetchStats,
    fetchLevel,
    startDebate,
    fetchDebateSession,
    approvePromotion,
    signRiskConfirmation,
    startAutonomous,
    stopAutonomous,
    emergencyStop,
    connectWebSocket,
    disconnectWebSocket,
    clearError,
    resetStore
  }
})
