import React, { useEffect, useState } from 'react';
import { View, Text, Textarea, Button } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import { todoService } from '@/services/todo';
import type { TodoDetail } from '@/types/todo';
import { TODO_TYPE_LABELS, TODO_PRIORITY_LABELS, TODO_STATUS_LABELS } from '@/types/todo';
import Tag from '@/components/Tag';
import EmptyState from '@/components/EmptyState';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * 待办详情页（二级页面）
 * 风险确认/异常审核/审批操作
 * 对应深度研究报告"待办详情"原型
 */
const TodoDetailPage: React.FC = () => {
  const router = useRouter();
  const todoId = router.params.id;
  const { t, locale } = useI18n();
  useLocalizedTitle('策略');

  const [todo, setTodo] = useState<TodoDetail | null>(null);
  const [comment, setComment] = useState('');
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    const load = async () => {
      if (!todoId) {
        setLoading(false);
        return;
      }
      try {
        const data = await todoService.getTodo(todoId);
        setTodo(data);
      } catch {
        Taro.showToast({ title: t('加载失败'), icon: 'none' });
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [todoId]);

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/todo/index' });
    });
  };

  // 处理审批动作
  const handleProcess = async (action: 'approve' | 'reject' | 'defer') => {
    if (!todo) return;
    const actionText = action === 'approve' ? '通过' : action === 'reject' ? '驳回' : '延后';
    setSubmitting(true);
    try {
      await todoService.processTodo(todo.id, { action, comment: comment.trim() || undefined });
      Taro.showToast({ title: t('已{action}', { action: t(actionText) }), icon: 'success' });
      setTimeout(() => handleBack(), 800);
    } catch {
      Taro.showToast({ title: t('操作失败'), icon: 'none' });
    } finally {
      setSubmitting(false);
    }
  };

  if (loading) {
    return (
      <View className={styles.detailPage}>
        <EmptyState title={t('加载中...')} />
      </View>
    );
  }

  if (!todo) {
    return (
      <View className={styles.detailPage}>
        <EmptyState title={t('待办不存在')} actionText={t('返回')} onAction={handleBack} />
      </View>
    );
  }

  const isProcessed = todo.status !== 'pending';
  const priorityVariant = todo.priority === 'critical' ? 'error' : todo.priority === 'high' ? 'warning' : 'default';

  return (
    <View className={styles.detailPage}>
      {/* 头部 */}
      <View className={styles.header}>
        <View className={styles.headerTop}>
          <View className={styles.backButton} onClick={handleBack}>
            <Text className={styles.backIcon}>‹</Text>
          </View>
        </View>
        <Text className={styles.todoTitle}>{t(todo.title)}</Text>
        <Text className={styles.todoDesc}>{t(todo.description)}</Text>
        <View className={styles.metaRow}>
          <Tag variant={priorityVariant}>{t(TODO_PRIORITY_LABELS[todo.priority])}</Tag>
          <Tag variant={todo.status === 'pending' ? 'primary' : 'default'}>
            {t(TODO_STATUS_LABELS[todo.status])}
          </Tag>
          <Tag variant="default">{t(TODO_TYPE_LABELS[todo.type])}</Tag>
          {todo.symbol && <Tag variant="primary">{todo.symbol}</Tag>}
        </View>
      </View>

      <View className={styles.content}>
        {/* 上下文信息 */}
        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('上下文信息')}</Text>
          {todo.context.risk_level && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('风险等级')}</Text>
              <Text className={styles.contextValue}>{todo.context.risk_level}</Text>
            </View>
          )}
          {todo.context.max_acceptable_loss !== undefined && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('最大可接受损失')}</Text>
              <Text className={`${styles.contextValue} ${styles.negative}`}>
                {todo.context.max_acceptable_loss.toLocaleString()}
              </Text>
            </View>
          )}
          {todo.context.current_loss !== undefined && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('当前损失')}</Text>
              <Text className={`${styles.contextValue} ${styles.negative}`}>
                {todo.context.current_loss.toLocaleString()}
              </Text>
            </View>
          )}
          {todo.context.requirements_met !== undefined && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('前置条件')}</Text>
              <Text className={styles.contextValue}>
                {t(todo.context.requirements_met ? '已满足' : '未满足')}
              </Text>
            </View>
          )}
          {todo.context.missing_requirements && todo.context.missing_requirements.length > 0 && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('缺失项')}</Text>
              <Text className={styles.contextValue}>
                {todo.context.missing_requirements.join('；')}
              </Text>
            </View>
          )}
        </View>

        {/* 操作历史 */}
        {todo.history.length > 0 && (
          <View className={styles.contentCard}>
            <Text className={styles.cardTitle}>{t('操作历史')}</Text>
            {todo.history.map((h, idx) => (
              <View key={idx} className={styles.historyItem}>
                <Text className={styles.historyAction}>{t(h.action)}</Text>
                <Text className={styles.historyMeta}>
                  {h.operator} · {new Date(h.timestamp).toLocaleString(locale)}
                </Text>
              </View>
            ))}
          </View>
        )}
      </View>

      {/* 底部审批操作区（仅未处理时显示） */}
      {!isProcessed && (
        <View className={styles.actionBar}>
          <Textarea
            className={styles.commentInput}
            placeholder={t('备注（可选）')}
            value={comment}
            onInput={(e) => setComment(e.detail.value)}
            maxlength={200}
          />
          <View className={styles.actionButtons}>
            <Button
              className={`${styles.actionButton} ${styles.reject}`}
              disabled={submitting}
              onClick={() => handleProcess('reject')}
            >
              {t('驳回')}
            </Button>
            <Button
              className={`${styles.actionButton} ${styles.defer}`}
              disabled={submitting}
              onClick={() => handleProcess('defer')}
            >
              {t('延后')}
            </Button>
            <Button
              className={`${styles.actionButton} ${styles.approve}`}
              loading={submitting}
              disabled={submitting}
              onClick={() => handleProcess('approve')}
            >
              {t('通过')}
            </Button>
          </View>
        </View>
      )}
    </View>
  );
};

export default TodoDetailPage;
