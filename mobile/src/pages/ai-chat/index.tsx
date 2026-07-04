import React, { useCallback, useEffect, useState } from 'react';
import { View, Text, ScrollView, Input } from '@tarojs/components';
import Taro, { usePullDownRefresh } from '@tarojs/taro';
import { Button, Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { aiService } from '@/services/ai';
import type { ChatSession, ChatMessage } from '@/types/ai';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * AI 对话页（二级页面）
 * 调用 aiService.listChatSessions(), getChatMessages(), sendChatMessage()
 * 展示：对话列表 + 消息列表 + 输入框
 */
const AiChatPage: React.FC = () => {
  const { t, locale } = useI18n();
  useLocalizedTitle('AI 对话');

  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [activeSession, setActiveSession] = useState<ChatSession | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(true);
  const [msgLoading, setMsgLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState(false);

  const loadSessions = useCallback(async () => {
    setLoading(true);
    setError(false);
    try {
      const list = await aiService.listChatSessions(50);
      setSessions(list || []);
      if (list.length > 0 && !activeSession) {
        setActiveSession(list[0]);
      }
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, [activeSession]);

  const loadMessages = useCallback(async (sessionId: string) => {
    setMsgLoading(true);
    try {
      const list = await aiService.getChatMessages(sessionId, { limit: 100 });
      setMessages(list || []);
    } catch {
      Taro.showToast({ title: t('加载失败'), icon: 'none' });
    } finally {
      setMsgLoading(false);
    }
  }, [t]);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  useEffect(() => {
    if (activeSession) {
      loadMessages(activeSession.session_id);
    } else {
      setMessages([]);
    }
  }, [activeSession, loadMessages]);

  usePullDownRefresh(async () => {
    await loadSessions();
    Taro.stopPullDownRefresh();
  });

  const handleSelectSession = (session: ChatSession) => {
    setActiveSession(session);
  };

  const handleSend = async () => {
    const content = input.trim();
    if (!content || !activeSession || sending) return;
    setSending(true);
    setInput('');
    try {
      const msg = await aiService.sendChatMessage(activeSession.session_id, content);
      setMessages((prev) => [...prev, msg]);
    } catch {
      Taro.showToast({ title: t('操作失败'), icon: 'none' });
    } finally {
      setSending(false);
    }
  };

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/ai/index' });
    });
  };

  if (loading) {
    return (
      <View className={styles.page}>
        <Skeleton rows={6} animated />
      </View>
    );
  }

  if (error) {
    return (
      <View className={styles.page}>
        <Empty
          title={t('加载失败')}
          description={t('数据开小差了，请稍后重试')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  if (sessions.length === 0) {
    return (
      <View className={styles.page}>
        <Empty
          title={t('暂无对话记录')}
          description={t('登录后可使用 AI 分析能力')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  return (
    <View className={styles.page}>
      <View className={styles.sessionBar}>
        <ScrollView scrollX className={styles.sessionScroll}>
          {sessions.map((s) => (
            <View
              key={s.session_id}
              className={`${styles.sessionTag} ${activeSession?.session_id === s.session_id ? styles.active : ''}`}
              onClick={() => handleSelectSession(s)}
            >
              <Text>{s.title || t('新对话')}</Text>
            </View>
          ))}
        </ScrollView>
      </View>

      <ScrollView scrollY className={styles.messageList}>
        {msgLoading ? (
          <View className={styles.msgSkeleton}>
            <Skeleton rows={4} animated />
          </View>
        ) : messages.length === 0 ? (
          <Empty title={t('暂无消息')} description={t('发送消息开始对话')} />
        ) : (
          messages.map((msg) => (
            <View
              key={msg.message_id}
              className={`${styles.messageItem} ${msg.role === 'user' ? styles.user : styles.assistant}`}
            >
              <Tag type={msg.role === 'user' ? 'primary' : 'default'}>
                {msg.role === 'user' ? t('用户') : t('助手')}
              </Tag>
              <Text className={styles.messageContent}>{msg.content}</Text>
              <Text className={styles.messageTime}>
                {new Date(msg.created_at).toLocaleString(locale)}
              </Text>
            </View>
          ))
        )}
      </ScrollView>

      <View className={styles.inputBar}>
        <Input
          className={styles.input}
          placeholder={t('请输入消息')}
          value={input}
          onInput={(e) => setInput(e.detail.value)}
          onConfirm={handleSend}
        />
        <Button
          type="primary"
          loading={sending}
          disabled={sending || !input.trim()}
          onClick={handleSend}
        >
          {t('发送')}
        </Button>
      </View>
    </View>
  );
};

export default AiChatPage;
