import React, { useEffect, useMemo, useState } from 'react';
import { ScrollView, Text, View } from '@tarojs/components';
import Taro, { usePullDownRefresh } from '@tarojs/taro';
import {
  Button,
  Empty,
  Progress,
  Skeleton,
  Switch,
  Tag,
} from '@nutui/nutui-react-taro';
import { agentService } from '@/services/agent';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import type {
  AgentAccuracy,
  AgentInfo,
  AgentLogEntry,
  AgentPermission,
  AgentStatus,
  AgentTrainingStatus,
  AgentDashboard,
  SimulationStatus,
} from '@/types/agent';
import styles from './index.module.scss';

type DetailTab = 'training' | 'accuracy' | 'permission' | 'log';
type LogLevel = AgentLogEntry['level'] | 'all';

// Agent 状态 → NutUI Tag 类型映射
const STATUS_TAG_TYPE: Record<
  AgentStatus,
  'default' | 'success' | 'primary' | 'warning' | 'danger'
> = {
  idle: 'default',
  running: 'success',
  training: 'primary',
  paused: 'warning',
  stopped: 'default',
  error: 'danger',
};

const STATUS_LABEL_KEY: Record<AgentStatus, string> = {
  idle: '空闲',
  running: '运行中',
  training: '训练中',
  paused: '已暂停',
  stopped: '已停止',
  error: '异常',
};

const LOG_LEVEL_TYPE: Record<
  AgentLogEntry['level'],
  'default' | 'primary' | 'warning' | 'danger'
> = {
  info: 'primary',
  warning: 'warning',
  error: 'danger',
  debug: 'default',
};

// 单个 Agent 的详情缓存
interface AgentDetail {
  training: AgentTrainingStatus | null;
  accuracy: AgentAccuracy | null;
  permission: AgentPermission | null;
  logs: AgentLogEntry[];
  loading: boolean;
}

const AgentPage: React.FC = () => {
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  const { t, locale } = useI18n();
  useLocalizedTitle('Agent');

  const [dashboard, setDashboard] = useState<AgentDashboard | null>(null);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [simulation, setSimulation] = useState<SimulationStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);

  // 当前展开的 Agent
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<DetailTab>('training');
  const [logFilter, setLogFilter] = useState<LogLevel>('all');
  // 详情缓存：agentId → detail
  const [detailCache, setDetailCache] = useState<Record<string, AgentDetail>>({});

  const loadData = async () => {
    setLoading(true);
    try {
      const [dash, list, sim] = await Promise.all([
        agentService.getDashboard(),
        agentService.getAgents(),
        agentService.getSimulationStatus().catch(() => null),
      ]);
      setDashboard(dash);
      setAgents(list && list.length > 0 ? list : dash.agents || []);
      setSimulation(sim);
    } catch {
      Taro.showToast({ title: t('数据开小差了，请稍后重试'), icon: 'none' });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!isAuthenticated) {
      return;
    }
    loadData();
  }, [isAuthenticated]);

  usePullDownRefresh(async () => {
    await loadData();
    Taro.stopPullDownRefresh();
  });

  // 展开某个 Agent 时加载详情
  const loadAgentDetail = async (agentId: string) => {
    setDetailCache((prev) => ({
      ...prev,
      [agentId]: {
        training: prev[agentId]?.training ?? null,
        accuracy: prev[agentId]?.accuracy ?? null,
        permission: prev[agentId]?.permission ?? null,
        logs: prev[agentId]?.logs ?? [],
        loading: true,
      },
    }));
    try {
      const [training, accuracy, permission, logs] = await Promise.all([
        agentService.getTrainingStatus(agentId).catch(() => null),
        agentService.getAccuracy(agentId).catch(() => null),
        agentService.getPermissions(agentId).catch(() => null),
        agentService.getLogs(agentId, 50).catch(() => [] as AgentLogEntry[]),
      ]);
      setDetailCache((prev) => ({
        ...prev,
        [agentId]: { training, accuracy, permission, logs, loading: false },
      }));
    } catch {
      setDetailCache((prev) => ({
        ...prev,
        [agentId]: {
          ...(prev[agentId] || {
            training: null,
            accuracy: null,
            permission: null,
            logs: [],
          }),
          loading: false,
        },
      }));
    }
  };

  const handleToggleAgent = (agentId: string) => {
    if (expandedId === agentId) {
      setExpandedId(null);
      return;
    }
    setExpandedId(agentId);
    setActiveTab('training');
    if (!detailCache[agentId]) {
      loadAgentDetail(agentId);
    }
  };

  const handleStartAutonomous = async () => {
    setActionLoading(true);
    try {
      await agentService.startAutonomous();
      Taro.showToast({ title: t('已开启自主模式'), icon: 'success' });
      await loadData();
    } catch {
      Taro.showToast({ title: t('操作失败'), icon: 'none' });
    } finally {
      setActionLoading(false);
    }
  };

  const handleEmergencyStop = () => {
    Taro.showModal({
      title: t('紧急停止'),
      content: t('确认立即停止所有 Agent 运行与模拟？此操作不可撤销。'),
      confirmText: t('确认停止'),
      cancelText: t('取消'),
      confirmColor: '#f53f3f',
      success: async (res) => {
        if (!res.confirm) return;
        setActionLoading(true);
        try {
          await agentService.emergencyStop();
          Taro.showToast({ title: t('已紧急停止'), icon: 'success' });
          await loadData();
        } catch {
          Taro.showToast({ title: t('操作失败'), icon: 'none' });
        } finally {
          setActionLoading(false);
        }
      },
    });
  };

  const handleStartSimulation = () => {
    Taro.showModal({
      title: t('启动新模拟'),
      content: t('请输入模拟配置 ID'),
      editable: true,
      placeholderText: 'sim-config-001',
      confirmText: t('启动'),
      cancelText: t('取消'),
      success: async (res) => {
        if (!res.confirm) return;
        const configId = (res.content || '').trim();
        if (!configId) {
          Taro.showToast({ title: t('请输入配置 ID'), icon: 'none' });
          return;
        }
        setActionLoading(true);
        try {
          await agentService.startSimulation(configId);
          Taro.showToast({ title: t('模拟已启动'), icon: 'success' });
          await loadData();
        } catch {
          Taro.showToast({ title: t('操作失败'), icon: 'none' });
        } finally {
          setActionLoading(false);
        }
      },
    });
  };

  // 计算胜率
  const computeWinRate = (agent: AgentInfo): number => {
    if (!agent.total_trades || agent.total_trades <= 0) return 0;
    const wins = agent.winning_trades ?? 0;
    return Math.round((wins / agent.total_trades) * 100);
  };

  // 准确率进度条颜色
  const accuracyColor = (value: number): string => {
    if (value >= 70) return '#00b42a';
    if (value >= 50) return '#165dff';
    if (value >= 30) return '#ff7d00';
    return '#f53f3f';
  };

  // 过滤后的日志
  const filteredLogs = useMemo(() => {
    if (!expandedId) return [];
    const detail = detailCache[expandedId];
    if (!detail) return [];
    if (logFilter === 'all') return detail.logs;
    return detail.logs.filter((log) => log.level === logFilter);
  }, [expandedId, detailCache, logFilter]);

  const formatTime = (iso: string): string => {
    try {
      return new Date(iso).toLocaleString(locale);
    } catch {
      return iso;
    }
  };

  // 未登录引导
  if (!isAuthenticated) {
    return (
      <View className={styles.page}>
        <View className={styles.notAuth}>
          <Empty
            title={t('未登录')}
            description={t('登录后可查看个人信息与设置')}
            actions={[
              {
                text: t('去登录'),
                type: 'primary',
                onClick: () => Taro.reLaunch({ url: '/pages/login/index' }),
              },
            ]}
          />
        </View>
      </View>
    );
  }

  // 加载骨架
  if (loading && !dashboard) {
    return (
      <View className={styles.page}>
        <View className={styles.topBar}>
          <Skeleton rows={1} animated style={{ width: '180rpx', height: '44rpx' }} />
        </View>
        <View className={styles.skeletonCard}>
          <Skeleton rows={4} animated />
        </View>
        <View className={styles.skeletonCard}>
          <Skeleton rows={5} animated />
        </View>
      </View>
    );
  }

  const runningCount = dashboard?.active_agents ?? 0;
  const trainingCount = dashboard?.training_agents ?? 0;
  const totalAccuracy = dashboard?.total_accuracy ?? 0;

  return (
    <View className={styles.page}>
      {/* 顶部标题栏 */}
      <View className={styles.topBar}>
        <View>
          <Text className={styles.brand}>MoneyRobert</Text>
          <Text className={styles.subtitle}>{t('Agent 管理中心')}</Text>
        </View>
      </View>

      <ScrollView scrollY className={styles.scroll}>
        {/* 1. 顶部汇总卡片 */}
        <View className={styles.summaryCard}>
          <View className={styles.summaryHeader}>
            <Text className={styles.summaryTitle}>{t('Agent 总览')}</Text>
            <Tag type="primary" round>
              <Text>{t('共 {count} 个', { count: dashboard?.total_agents ?? 0 })}</Text>
            </Tag>
          </View>
          <View className={styles.summaryGrid}>
            <View className={styles.summaryItem}>
              <Text className={styles.summaryValue}>{dashboard?.total_agents ?? 0}</Text>
              <Text className={styles.summaryLabel}>{t('总 Agent')}</Text>
            </View>
            <View className={styles.summaryItem}>
              <Text className={`${styles.summaryValue} ${styles.textSuccess}`}>
                {runningCount}
              </Text>
              <Text className={styles.summaryLabel}>{t('运行中')}</Text>
            </View>
            <View className={styles.summaryItem}>
              <Text className={`${styles.summaryValue} ${styles.textPrimary}`}>
                {trainingCount}
              </Text>
              <Text className={styles.summaryLabel}>{t('训练中')}</Text>
            </View>
            <View className={styles.summaryItem}>
              <Text className={`${styles.summaryValue} ${styles.textWarning}`}>
                {totalAccuracy.toFixed(1)}%
              </Text>
              <Text className={styles.summaryLabel}>{t('整体准确率')}</Text>
            </View>
          </View>
        </View>

        {/* 3. 操作按钮组 */}
        <View className={styles.actionGroup}>
          <Button
            shape="round"
            type="primary"
            size="small"
            loading={actionLoading}
            className={styles.actionPrimary}
            onClick={handleStartAutonomous}
          >
            {t('启动自主模式')}
          </Button>
          <Button
            shape="round"
            type="danger"
            size="small"
            className={styles.actionDanger}
            onClick={handleEmergencyStop}
          >
            {t('紧急停止')}
          </Button>
          <Button
            shape="round"
            size="small"
            className={styles.actionSim}
            onClick={handleStartSimulation}
          >
            {t('启动新模拟')}
          </Button>
        </View>

        {/* 4. 模拟状态卡片 */}
        {simulation && (
          <View className={styles.sectionCard}>
            <View className={styles.cardHeader}>
              <Text className={styles.cardTitle}>{t('模拟状态')}</Text>
              <Tag
                type={simulation.status === 'running' ? 'success' : 'default'}
                round
              >
                <Text>{t(simulation.status === 'running' ? '运行中' : '已停止')}</Text>
              </Tag>
            </View>
            <View className={styles.simBody}>
              <View className={styles.simRow}>
                <Text className={styles.simLabel}>{t('模拟 ID')}</Text>
                <Text className={styles.simValue}>{simulation.simulation_id}</Text>
              </View>
              <View className={styles.simRow}>
                <Text className={styles.simLabel}>{t('成交数')}</Text>
                <Text className={styles.simValue}>{simulation.trades_count}</Text>
              </View>
              <View className={styles.simRow}>
                <Text className={styles.simLabel}>{t('当前权益')}</Text>
                <Text className={styles.simValue}>
                  {(simulation.current_equity ?? 0).toLocaleString()}
                </Text>
              </View>
              <View className={styles.simRow}>
                <Text className={styles.simLabel}>{t('收益率')}</Text>
                <Text
                  className={`${styles.simValue} ${
                    (simulation.return_pct ?? 0) >= 0 ? styles.textSuccess : styles.textError
                  }`}
                >
                  {(simulation.return_pct ?? 0) >= 0 ? '+' : ''}
                  {(simulation.return_pct ?? 0).toFixed(2)}%
                </Text>
              </View>
              <View className={styles.simRow}>
                <Text className={styles.simLabel}>{t('开始时间')}</Text>
                <Text className={styles.simValue}>{formatTime(simulation.started_at)}</Text>
              </View>
            </View>
          </View>
        )}

        {/* 2. Agent 列表 */}
        <View className={styles.sectionHeader}>
          <Text className={styles.sectionTitle}>{t('Agent 列表')}</Text>
          <Text className={styles.sectionHint}>{t('点击展开详情')}</Text>
        </View>

        {agents.length === 0 ? (
          <View className={styles.emptyWrap}>
            <Empty title={t('暂无 Agent')} description={t('暂无数据')} />
          </View>
        ) : (
          agents.map((agent) => {
            const acc = agent.accuracy ?? 0;
            const winRate = computeWinRate(agent);
            const expanded = expandedId === agent.agent_id;
            const detail = detailCache[agent.agent_id];
            return (
              <View key={agent.agent_id} className={styles.agentCard}>
                {/* Agent 卡片头部 */}
                <View
                  className={styles.agentHead}
                  onClick={() => handleToggleAgent(agent.agent_id)}
                >
                  <View className={styles.agentInfo}>
                    <View className={styles.agentNameRow}>
                      <Text className={styles.agentName}>{agent.name}</Text>
                      {agent.version && (
                        <Tag type="default" className={styles.versionTag}>
                          <Text>v{agent.version}</Text>
                        </Tag>
                      )}
                    </View>
                    <Tag type={STATUS_TAG_TYPE[agent.status]} round className={styles.statusTag}>
                      <Text>{t(STATUS_LABEL_KEY[agent.status])}</Text>
                    </Tag>
                  </View>

                  {/* 准确率进度条 */}
                  <View className={styles.accBlock}>
                    <View className={styles.accLabel}>
                      <Text>{t('准确率')}</Text>
                      <Text className={styles.accValue}>{acc.toFixed(1)}%</Text>
                    </View>
                    <Progress
                      percentage={acc}
                      strokeColor={accuracyColor(acc)}
                      strokeWidth={10}
                      showBackground
                    />
                  </View>

                  {/* 交易统计 */}
                  <View className={styles.tradeStats}>
                    <View className={styles.tradeItem}>
                      <Text className={styles.tradeLabel}>{t('总交易数')}</Text>
                      <Text className={styles.tradeValue}>{agent.total_trades ?? 0}</Text>
                    </View>
                    <View className={styles.tradeItem}>
                      <Text className={styles.tradeLabel}>{t('胜率')}</Text>
                      <Text
                        className={`${styles.tradeValue} ${
                          winRate >= 50 ? styles.textSuccess : styles.textError
                        }`}
                      >
                        {winRate}%
                      </Text>
                    </View>
                  </View>

                  <View className={styles.agentChevron}>
                    <Text className={`${styles.chevron} ${expanded ? styles.chevronUp : ''}`}>
                      ›
                    </Text>
                  </View>
                </View>

                {/* 展开详情 */}
                {expanded && (
                  <View className={styles.agentDetail}>
                    {detail?.loading && <Skeleton rows={4} animated />}
                    {!detail?.loading && (
                      <>
                        {/* Tab 切换 */}
                        <View className={styles.tabBar}>
                          {(
                            [
                              { key: 'training' as const, label: t('训练') },
                              { key: 'accuracy' as const, label: t('准确率') },
                              { key: 'permission' as const, label: t('权限') },
                              { key: 'log' as const, label: t('日志') },
                            ]
                          ).map((tab) => (
                            <View
                              key={tab.key}
                              className={`${styles.tabItem} ${
                                activeTab === tab.key ? styles.tabActive : ''
                              }`}
                              onClick={() => setActiveTab(tab.key)}
                            >
                              <Text>{tab.label}</Text>
                            </View>
                          ))}
                        </View>

                        {/* 5. 训练情况 Tab */}
                        {activeTab === 'training' && (
                          <View className={styles.tabPanel}>
                            {detail?.training ? (
                              <>
                                <View className={styles.accLabel}>
                                  <Text>{t('训练进度')}</Text>
                                  <Text className={styles.accValue}>
                                    {detail.training.progress}%
                                  </Text>
                                </View>
                                <Progress
                                  percentage={detail.training.progress}
                                  strokeColor="#165dff"
                                  strokeWidth={10}
                                  showBackground
                                />
                                <View className={styles.detailGrid}>
                                  <View className={styles.detailItem}>
                                    <Text className={styles.detailLabel}>
                                      {t('当前 Epoch')}
                                    </Text>
                                    <Text className={styles.detailValue}>
                                      {detail.training.current_epoch ?? 0} /{' '}
                                      {detail.training.total_epochs ?? 0}
                                    </Text>
                                  </View>
                                  <View className={styles.detailItem}>
                                    <Text className={styles.detailLabel}>{t('Loss')}</Text>
                                    <Text className={styles.detailValue}>
                                      {(detail.training.loss ?? 0).toFixed(4)}
                                    </Text>
                                  </View>
                                  <View className={styles.detailItem}>
                                    <Text className={styles.detailLabel}>{t('准确率')}</Text>
                                    <Text className={styles.detailValue}>
                                      {(detail.training.accuracy ?? 0).toFixed(1)}%
                                    </Text>
                                  </View>
                                  <View className={styles.detailItem}>
                                    <Text className={styles.detailLabel}>{t('状态')}</Text>
                                    <Tag type={STATUS_TAG_TYPE[detail.training.status]} round>
                                      <Text>{t(STATUS_LABEL_KEY[detail.training.status])}</Text>
                                    </Tag>
                                  </View>
                                </View>
                              </>
                            ) : (
                              <Empty title={t('暂无训练数据')} />
                            )}
                          </View>
                        )}

                        {/* 6. 准确率 Tab */}
                        {activeTab === 'accuracy' && (
                          <View className={styles.tabPanel}>
                            {detail?.accuracy ? (
                              <>
                                <View className={styles.detailGrid}>
                                  <View className={styles.detailItem}>
                                    <Text className={styles.detailLabel}>
                                      {t('整体准确率')}
                                    </Text>
                                    <Text className={styles.detailValue}>
                                      {(detail.accuracy.overall_accuracy ?? 0).toFixed(1)}%
                                    </Text>
                                  </View>
                                  <View className={styles.detailItem}>
                                    <Text className={styles.detailLabel}>
                                      {t('近期准确率')}
                                    </Text>
                                    <Text className={styles.detailValue}>
                                      {(detail.accuracy.recent_accuracy ?? 0).toFixed(1)}%
                                    </Text>
                                  </View>
                                  <View className={styles.detailItem}>
                                    <Text className={styles.detailLabel}>
                                      {t('总预测数')}
                                    </Text>
                                    <Text className={styles.detailValue}>
                                      {detail.accuracy.total_predictions}
                                    </Text>
                                  </View>
                                  <View className={styles.detailItem}>
                                    <Text className={styles.detailLabel}>
                                      {t('正确预测')}
                                    </Text>
                                    <Text className={styles.detailValue}>
                                      {detail.accuracy.correct_predictions}
                                    </Text>
                                  </View>
                                </View>

                                {detail.accuracy.by_symbol &&
                                  detail.accuracy.by_symbol.length > 0 && (
                                    <View className={styles.subBlock}>
                                      <Text className={styles.subTitle}>
                                        {t('按品种准确率分布')}
                                      </Text>
                                      {detail.accuracy.by_symbol.map((item) => (
                                        <View key={item.symbol} className={styles.distRow}>
                                          <Text className={styles.distLabel}>
                                            {item.symbol}
                                          </Text>
                                          <View className={styles.distTrack}>
                                            <Progress
                                              percentage={item.accuracy}
                                              strokeColor={accuracyColor(item.accuracy)}
                                              strokeWidth={8}
                                              showBackground
                                            />
                                          </View>
                                          <Text className={styles.distValue}>
                                            {(item.accuracy ?? 0).toFixed(1)}%
                                          </Text>
                                        </View>
                                      ))}
                                    </View>
                                  )}

                                {detail.accuracy.by_horizon &&
                                  detail.accuracy.by_horizon.length > 0 && (
                                    <View className={styles.subBlock}>
                                      <Text className={styles.subTitle}>
                                        {t('按时间周期准确率分布')}
                                      </Text>
                                      {detail.accuracy.by_horizon.map((item) => (
                                        <View key={item.horizon} className={styles.distRow}>
                                          <Text className={styles.distLabel}>
                                            {item.horizon}
                                          </Text>
                                          <View className={styles.distTrack}>
                                            <Progress
                                              percentage={item.accuracy}
                                              strokeColor={accuracyColor(item.accuracy)}
                                              strokeWidth={8}
                                              showBackground
                                            />
                                          </View>
                                          <Text className={styles.distValue}>
                                            {(item.accuracy ?? 0).toFixed(1)}%
                                          </Text>
                                        </View>
                                      ))}
                                    </View>
                                  )}
                              </>
                            ) : (
                              <Empty title={t('暂无准确率数据')} />
                            )}
                          </View>
                        )}

                        {/* 7. 权限 Tab */}
                        {activeTab === 'permission' && (
                          <View className={styles.tabPanel}>
                            {detail?.permission ? (
                              <>
                                <View className={styles.permRow}>
                                  <Text className={styles.permLabel}>{t('可交易')}</Text>
                                  <Switch checked={detail.permission.can_trade} disabled />
                                </View>
                                <View className={styles.permRow}>
                                  <Text className={styles.permLabel}>
                                    {t('可修改策略')}
                                  </Text>
                                  <Switch
                                    checked={detail.permission.can_modify_strategy}
                                    disabled
                                  />
                                </View>
                                <View className={styles.permRow}>
                                  <Text className={styles.permLabel}>
                                    {t('最大仓位')}
                                  </Text>
                                  <Text className={styles.permValue}>
                                    {(detail.permission.max_position_size ?? 0).toLocaleString()}
                                  </Text>
                                </View>
                                <View className={styles.permRow}>
                                  <Text className={styles.permLabel}>
                                    {t('最大杠杆')}
                                  </Text>
                                  <Text className={styles.permValue}>
                                    {detail.permission.max_leverage}x
                                  </Text>
                                </View>
                                <View className={styles.permRow}>
                                  <Text className={styles.permLabel}>
                                    {t('允许品种')}
                                  </Text>
                                  <View className={styles.tagWrap}>
                                    {detail.permission.allowed_symbols.length > 0 ? (
                                      detail.permission.allowed_symbols.map((sym) => (
                                        <Tag
                                          key={sym}
                                          type="primary"
                                          className={styles.permTag}
                                        >
                                          <Text>{sym}</Text>
                                        </Tag>
                                      ))
                                    ) : (
                                      <Text className={styles.permEmpty}>{t('无')}</Text>
                                    )}
                                  </View>
                                </View>
                                <View className={styles.permRow}>
                                  <Text className={styles.permLabel}>
                                    {t('限制操作')}
                                  </Text>
                                  <View className={styles.tagWrap}>
                                    {detail.permission.restricted_actions.length > 0 ? (
                                      detail.permission.restricted_actions.map((act) => (
                                        <Tag
                                          key={act}
                                          type="danger"
                                          className={styles.permTag}
                                        >
                                          <Text>{act}</Text>
                                        </Tag>
                                      ))
                                    ) : (
                                      <Text className={styles.permEmpty}>{t('无')}</Text>
                                    )}
                                  </View>
                                </View>
                              </>
                            ) : (
                              <Empty title={t('暂无权限数据')} />
                            )}
                          </View>
                        )}

                        {/* 8. 日志 Tab */}
                        {activeTab === 'log' && (
                          <View className={styles.tabPanel}>
                            {/* 级别过滤 */}
                            <View className={styles.logFilter}>
                              {(
                                [
                                  'all',
                                  'info',
                                  'warning',
                                  'error',
                                  'debug',
                                ] as LogLevel[]
                              ).map((lv) => (
                                <View
                                  key={lv}
                                  className={`${styles.logFilterItem} ${
                                    logFilter === lv ? styles.logFilterActive : ''
                                  }`}
                                  onClick={() => setLogFilter(lv)}
                                >
                                  <Text>
                                    {lv === 'all'
                                      ? t('全部')
                                      : t(
                                          lv === 'info'
                                            ? '信息'
                                            : lv === 'warning'
                                            ? '警告'
                                            : lv === 'error'
                                            ? '错误'
                                            : '调试',
                                        )}
                                  </Text>
                                </View>
                              ))}
                            </View>

                            {filteredLogs.length === 0 ? (
                              <Empty title={t('暂无日志')} />
                            ) : (
                              filteredLogs.map((log) => (
                                <View key={log.log_id} className={styles.logItem}>
                                  <View className={styles.logHead}>
                                    <Tag type={LOG_LEVEL_TYPE[log.level]} round>
                                      <Text>{log.level.toUpperCase()}</Text>
                                    </Tag>
                                    <Text className={styles.logTime}>
                                      {formatTime(log.timestamp)}
                                    </Text>
                                  </View>
                                  <Text className={styles.logMsg}>{log.message}</Text>
                                </View>
                              ))
                            )}
                          </View>
                        )}
                      </>
                    )}
                  </View>
                )}
              </View>
            );
          })
        )}

        <View className={styles.disclaimer}>
          <Text>{t('数据与分析仅供参考，不构成投资建议。市场有风险，决策需独立判断。')}</Text>
        </View>
      </ScrollView>
    </View>
  );
};

export default AgentPage;
