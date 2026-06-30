import React, { useEffect, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { usePullDownRefresh } from '@tarojs/taro';
import { todoService } from '@/services/todo';
import type { TodoItem } from '@/types/todo';
import { TODO_TYPE_LABELS, TODO_PRIORITY_LABELS } from '@/types/todo';
import Tag from '@/components/Tag';
import EmptyState from '@/components/EmptyState';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

type FilterKey = 'all' | 'risk_confirmation' | 'exception_review' | 'promotion_approval' | 'alert_acknowledgement';

/**
 * 待办中心页（tabBar）
 * 风险确认、异常审核、人工复核、升级审批、告警确认
 */
const TodoPage: React.FC = () => {
  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [filter, setFilter] = useState<FilterKey>('all');
  const [loading, setLoading] = useState(false);
  const { t, locale } = useI18n();
  useLocalizedTitle('策略');

  const loadTodos = async () => {
    setLoading(true);
    try {
      const data = await todoService.listTodos();
      setTodos(data);
    } catch {
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadTodos();
  }, []);

  usePullDownRefresh(async () => {
    await loadTodos();
    Taro.stopPullDownRefresh();
  });

  // 应用筛选
  const filteredTodos = filter === 'all' ? todos : todos.filter((t) => t.type === filter);

  // 跳转待办详情
  const handleTodoClick = (id: string) => {
    Taro.navigateTo({ url: `/pages/todo-detail/index?id=${id}` });
  };

  const filters: Array<{ key: FilterKey; label: string }> = [
    { key: 'all', label: t('全部') },
    { key: 'risk_confirmation', label: t('风险确认') },
    { key: 'exception_review', label: t('异常审核') },
    { key: 'promotion_approval', label: t('升级审批') },
    { key: 'alert_acknowledgement', label: t('告警确认') },
  ];

  // 优先级颜色
  const getPriorityVariant = (priority: string) => {
    if (priority === 'critical') return 'error';
    if (priority === 'high') return 'warning';
    if (priority === 'medium') return 'default';
    return 'default';
  };

  return (
    <View className={styles.todoPage}>
      <View className={styles.filterBar}>
        {filters.map((f) => (
          <View
            key={f.key}
            className={`${styles.filterItem} ${filter === f.key ? styles.active : ''}`}
            onClick={() => setFilter(f.key)}
          >
            <Text>{f.label}</Text>
          </View>
        ))}
      </View>

      <ScrollView scrollY className={styles.todoList}>
        {loading && <EmptyState title={t('加载中...')} />}
        {!loading && filteredTodos.length === 0 ? (
          <EmptyState title={t('暂无待办')} description={t('所有任务已处理完成')} />
        ) : (
          filteredTodos.map((todo) => (
            <View
              key={todo.id}
              className={styles.todoItem}
              onClick={() => handleTodoClick(todo.id)}
            >
              <View className={styles.todoHeader}>
                <View className={`${styles.todoPriorityDot} ${styles[todo.priority]}`} />
                <Text className={styles.todoTitle}>{t(todo.title)}</Text>
                <Tag variant={getPriorityVariant(todo.priority)}>
                  {TODO_PRIORITY_LABELS[todo.priority]}
                </Tag>
              </View>
              <Text className={styles.todoDesc}>{t(todo.description)}</Text>
              <View className={styles.todoFooter}>
                <Text className={styles.todoMeta}>
                  {t(TODO_TYPE_LABELS[todo.type])}
                  {todo.due_at && ` · ${t('截止 {date}', { date: new Date(todo.due_at).toLocaleDateString(locale) })}`}
                </Text>
                <Text className={styles.todoAction}>{t('处理 ›')}</Text>
              </View>
            </View>
          ))
        )}
      </ScrollView>
    </View>
  );
};

export default TodoPage;
