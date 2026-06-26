import React, { useEffect, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { usePullDownRefresh } from '@tarojs/taro';
import { messageService } from '@/services/message';
import type { MessageItem, MessageType } from '@/types/message';
import { MESSAGE_TYPE_LABELS } from '@/types/message';
import EmptyState from '@/components/EmptyState';
import styles from './index.module.scss';

type FilterKey = 'all' | MessageType;

/**
 * 消息中心页（tabBar）
 * 系统通知、业务告警、风险提醒、审批通知、升级通知
 */
const MessagePage: React.FC = () => {
  const [messages, setMessages] = useState<MessageItem[]>([]);
  const [filter, setFilter] = useState<FilterKey>('all');
  const [loading, setLoading] = useState(false);

  const loadMessages = async () => {
    setLoading(true);
    try {
      const data = await messageService.listMessages();
      setMessages(data);
    } catch (err) {
      console.error('[Message] load failed:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadMessages();
  }, []);

  usePullDownRefresh(async () => {
    await loadMessages();
    Taro.stopPullDownRefresh();
  });

  // 应用筛选
  const filteredMessages = filter === 'all' ? messages : messages.filter((m) => m.type === filter);

  // 标记单条已读并跳转
  const handleMessageClick = async (msg: MessageItem) => {
    if (msg.status === 'unread') {
      try {
        await messageService.markRead(msg.id);
        setMessages((prev) =>
          prev.map((m) => (m.id === msg.id ? { ...m, status: 'read' } : m)),
        );
      } catch (err) {
        console.error('[Message] markRead failed:', err);
      }
    }
    if (msg.link) {
      const query = msg.link.params
        ? `?${Object.entries(msg.link.params).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&')}`
        : '';
      Taro.navigateTo({ url: `${msg.link.page}${query}` }).catch(() => {
        Taro.showToast({ title: '页面不存在', icon: 'none' });
      });
    }
  };

  // 标记全部已读
  const handleMarkAll = async () => {
    try {
      await messageService.markAllRead();
      setMessages((prev) => prev.map((m) => ({ ...m, status: 'read' })));
      Taro.showToast({ title: '已全部标记为已读', icon: 'success' });
    } catch (err) {
      Taro.showToast({ title: '操作失败', icon: 'none' });
    }
  };

  const filters: Array<{ key: FilterKey; label: string }> = [
    { key: 'all', label: '全部' },
    { key: 'risk', label: '风险' },
    { key: 'business', label: '业务' },
    { key: 'approval', label: '审批' },
    { key: 'system', label: '系统' },
  ];

  // 图标首字
  const getIconChar = (type: MessageType) => {
    return MESSAGE_TYPE_LABELS[type].slice(0, 1);
  };

  return (
    <View className={styles.messagePage}>
      <View className={styles.topBar}>
        <View className={styles.filterTabs}>
          {filters.map((f) => (
            <View
              key={f.key}
              className={`${styles.filterTab} ${filter === f.key ? styles.active : ''}`}
              onClick={() => setFilter(f.key)}
            >
              <Text>{f.label}</Text>
            </View>
          ))}
        </View>
        <Text className={styles.markAll} onClick={handleMarkAll}>全部已读</Text>
      </View>

      <ScrollView scrollY className={styles.messageList}>
        {loading && <EmptyState title="加载中..." />}
        {!loading && filteredMessages.length === 0 ? (
          <EmptyState title="暂无消息" description="新的消息会显示在这里" />
        ) : (
          filteredMessages.map((msg) => (
            <View
              key={msg.id}
              className={`${styles.messageItem} ${msg.status === 'unread' ? styles.unread : ''}`}
              onClick={() => handleMessageClick(msg)}
            >
              <View className={`${styles.messageIcon} ${styles[msg.type]}`}>
                <Text>{getIconChar(msg.type)}</Text>
              </View>
              <View className={styles.messageContent}>
                <View className={styles.messageHeader}>
                  <Text className={styles.messageTitle}>{msg.title}</Text>
                  <Text className={styles.messageTime}>
                    {new Date(msg.created_at).toLocaleDateString()}
                  </Text>
                </View>
                <Text className={styles.messageBody}>{msg.content}</Text>
              </View>
              {msg.status === 'unread' && <View className={styles.unreadDot} />}
            </View>
          ))
        )}
      </ScrollView>
    </View>
  );
};

export default MessagePage;
