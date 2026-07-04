import React, { useEffect, useMemo, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { usePullDownRefresh, useDidShow } from '@tarojs/taro';
import { Badge, Button, Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import dayjs from 'dayjs';
import { messageService } from '@/services/message';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import { buildSafeNavigationQuery } from '@/security/transport';
import styles from './index.module.scss';

/**
 * 消息类型（对接后端 /notifications 的 type 字段）
 * 与后端真实接口契约一致：trade / risk / news / agent / system
 */
type MessageType = 'trade' | 'risk' | 'news' | 'agent' | 'system';
type FilterKey = 'all' | MessageType;

/** 消息条目（对接后端真实接口返回结构） */
interface MessageEntry {
  id: string;
  type: MessageType;
  title: string;
  content: string;
  status: 'read' | 'unread';
  created_at: string;
  link?: { page: string; params: Record<string, string> };
}

/** 类型展示配置：图标首字 + i18n 标签键 + NutUI Tag 类型 */
interface TypeConfig {
  iconChar: string;
  label: string;
  tagType: 'default' | 'primary' | 'success' | 'warning' | 'danger';
}

const TYPE_CONFIG: Record<MessageType, TypeConfig> = {
  trade: { iconChar: 'T', label: '交易', tagType: 'success' },
  risk: { iconChar: '!', label: '风险', tagType: 'danger' },
  news: { iconChar: 'N', label: '新闻', tagType: 'primary' },
  agent: { iconChar: 'A', label: 'Agent', tagType: 'default' },
  system: { iconChar: 'S', label: '系统', tagType: 'default' },
};

const TYPE_ORDER: MessageType[] = ['trade', 'risk', 'news', 'agent', 'system'];

const FILTERS: Array<{ key: FilterKey; label: string }> = [
  { key: 'all', label: '全部' },
  { key: 'trade', label: '交易' },
  { key: 'risk', label: '风险' },
  { key: 'news', label: '新闻' },
  { key: 'agent', label: 'Agent' },
  { key: 'system', label: '系统' },
];

/**
 * 格式化时间：今日显示 HH:mm，本年显示 MM-DD，其它显示 YYYY-MM-DD。
 * 使用 dayjs 解析 ISO 字符串，规避小程序下 toLocaleDateString 的兼容性问题。
 */
function formatTime(iso: string): string {
  const d = dayjs(iso);
  if (!d.isValid()) return '';
  const now = dayjs();
  if (d.isSame(now, 'day')) return d.format('HH:mm');
  if (d.isSame(now, 'year')) return d.format('MM-DD');
  return d.format('YYYY-MM-DD');
}

/**
 * 消息中心页（tabBar）
 * 基于后端 /notifications 真实接口：
 * - GET  /notifications        分页查询
 * - PUT  /notifications/{id}/read   标记单条已读
 * - PUT  /notifications/read-all   标记全部已读
 */
const MessagePage: React.FC = () => {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const [messages, setMessages] = useState<MessageEntry[]>([]);
  const [filter, setFilter] = useState<FilterKey>('all');
  const [loading, setLoading] = useState(true);
  const { t } = useI18n();
  useLocalizedTitle('消息');

  const loadMessages = async (silent = false) => {
    if (!silent) setLoading(true);
    try {
      // messageService 返回后端 items，按真实接口契约断言为 MessageEntry[]
      const data = (await messageService.listMessages()) as unknown as MessageEntry[];
      setMessages(Array.isArray(data) ? data : []);
    } catch {
      if (!silent) {
        Taro.showToast({ title: t('数据开小差了，请稍后重试'), icon: 'none' });
      }
    } finally {
      if (!silent) setLoading(false);
    }
  };

  // 首次进入加载
  useEffect(() => {
    if (!isAuthenticated) return;
    loadMessages();
  }, [isAuthenticated]);

  // 页面显示时刷新（从详情页返回后更新未读数）
  useDidShow(() => {
    if (isAuthenticated) loadMessages(true);
  });

  usePullDownRefresh(async () => {
    await loadMessages(true);
    Taro.stopPullDownRefresh();
  });

  const filteredMessages = useMemo(() => {
    if (filter === 'all') return messages;
    return messages.filter((m) => m.type === filter);
  }, [messages, filter]);

  // 统计：总数 / 未读数 / 各类型未读分布
  const stats = useMemo(() => {
    const total = messages.length;
    const unread = messages.filter((m) => m.status === 'unread').length;
    const byType: Record<MessageType, number> = { trade: 0, risk: 0, news: 0, agent: 0, system: 0 };
    messages.forEach((m) => {
      if (m.status === 'unread' && byType[m.type] !== undefined) {
        byType[m.type] += 1;
      }
    });
    return { total, unread, byType };
  }, [messages]);

  const filterUnreadCount = (key: FilterKey): number =>
    key === 'all' ? stats.unread : stats.byType[key] || 0;

  const handleMessageClick = (msg: MessageEntry) => {
    // 乐观标记已读，失败回滚
    if (msg.status === 'unread') {
      setMessages((prev) => prev.map((m) => (m.id === msg.id ? { ...m, status: 'read' } : m)));
      messageService.markRead(msg.id).catch(() => {
        setMessages((prev) => prev.map((m) => (m.id === msg.id ? { ...m, status: 'unread' } : m)));
        Taro.showToast({ title: t('操作失败'), icon: 'none' });
      });
    }
    // 安全跳转（保留既有逻辑：敏感字段过滤 + 编码）
    if (msg.link) {
      let query = '';
      try {
        query = buildSafeNavigationQuery(msg.link.params);
      } catch {
        Taro.showToast({ title: t('操作失败'), icon: 'none' });
        return;
      }
      Taro.navigateTo({ url: `${msg.link.page}${query}` }).catch(() => {
        Taro.showToast({ title: t('页面不存在'), icon: 'none' });
      });
    }
  };

  const handleMarkAll = async () => {
    if (stats.unread === 0) {
      Taro.showToast({ title: t('暂无未读消息'), icon: 'none' });
      return;
    }
    try {
      await messageService.markAllRead();
      setMessages((prev) => prev.map((m) => ({ ...m, status: 'read' })));
      Taro.showToast({ title: t('已全部标记为已读'), icon: 'success' });
    } catch {
      Taro.showToast({ title: t('操作失败'), icon: 'none' });
    }
  };

  // 未登录：引导登录
  if (!isAuthenticated) {
    return (
      <View className={styles.messagePage}>
        <View className={styles.loginWrap}>
          <Empty
            title={t('未登录')}
            description={t('登录后查看消息中心')}
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

  return (
    <View className={styles.messagePage}>
      {/* 统计卡片：总消息数 + 未读数 + 各类型未读分布 */}
      <View className={styles.statsCard}>
        <View className={styles.statsHeader}>
          <View className={styles.statsUnread}>
            <Text className={styles.statsUnreadNum}>{stats.unread}</Text>
            <Text className={styles.statsUnreadLabel}>{t('未读消息')}</Text>
          </View>
          <View className={styles.statsTotal}>
            <Text className={styles.statsTotalNum}>{stats.total}</Text>
            <Text className={styles.statsTotalLabel}>{t('总消息')}</Text>
          </View>
        </View>
        <View className={styles.typeDistribution}>
          {TYPE_ORDER.map((type) => {
            const cfg = TYPE_CONFIG[type];
            const count = stats.byType[type] || 0;
            return (
              <View
                key={type}
                className={`${styles.distTagWrap} ${count === 0 ? styles.dim : ''}`}
                onClick={() => setFilter(type)}
              >
                <Tag type={cfg.tagType} round className={`${styles.distTag} ${styles[type]}`}>
                  {`${t(cfg.label)} ${count}`}
                </Tag>
              </View>
            );
          })}
        </View>
      </View>

      {/* 顶部操作栏：左筛选 Tab（横向滚动） + 右全部已读 */}
      <View className={styles.topBar}>
        <ScrollView scrollX className={styles.filterTabs}>
          {FILTERS.map((f) => {
            const unread = filterUnreadCount(f.key);
            return (
              <View
                key={f.key}
                className={`${styles.filterTab} ${filter === f.key ? styles.active : ''}`}
                onClick={() => setFilter(f.key)}
              >
                <Text>{t(f.label)}</Text>
                {unread > 0 && <Badge value={unread} max={99} className={styles.tabBadge} />}
              </View>
            );
          })}
        </ScrollView>
        <Button
          size="mini"
          shape="round"
          type="primary"
          className={styles.markAllBtn}
          onClick={handleMarkAll}
        >
          {t('全部已读')}
        </Button>
      </View>

      {/* 消息列表 */}
      <ScrollView scrollY className={styles.messageList}>
        {loading ? (
          <View className={styles.skeletonWrap}>
            <Skeleton rows={6} animated />
          </View>
        ) : filteredMessages.length === 0 ? (
          <Empty title={t('暂无消息')} description={t('新的消息会显示在这里')} />
        ) : (
          filteredMessages.map((msg) => {
            const cfg = TYPE_CONFIG[msg.type] || TYPE_CONFIG.system;
            return (
              <View
                key={msg.id}
                className={`${styles.messageItem} ${msg.status === 'unread' ? styles.unread : ''}`}
                onClick={() => handleMessageClick(msg)}
              >
                <View className={`${styles.messageIcon} ${styles[msg.type]}`}>
                  <Text>{cfg.iconChar}</Text>
                </View>
                <View className={styles.messageContent}>
                  <View className={styles.messageHeader}>
                    <Text className={styles.messageTitle}>{t(msg.title)}</Text>
                    <Text className={styles.messageTime}>{formatTime(msg.created_at)}</Text>
                  </View>
                  <Text className={styles.messageBody}>{t(msg.content)}</Text>
                </View>
                {msg.status === 'unread' && <View className={styles.unreadDot} />}
              </View>
            );
          })
        )}
      </ScrollView>
    </View>
  );
};

export default MessagePage;
