import React from 'react';
import { View, Text } from '@tarojs/components';
import styles from './index.module.scss';

/**
 * 空状态组件
 * 用于列表为空、加载失败、无权限等场景
 */
export interface EmptyStateProps {
  /** 标题文案，默认"暂无数据" */
  title?: string;
  /** 描述文案 */
  description?: string;
  /** 操作按钮文案（可选） */
  actionText?: string;
  onAction?: () => void;
}

const EmptyState: React.FC<EmptyStateProps> = ({
  title = '暂无数据',
  description,
  actionText,
  onAction,
}) => {
  return (
    <View className={styles.empty}>
      <View className={styles.icon}>
        <Text className={styles.iconText}>·</Text>
      </View>
      <Text className={styles.title}>{title}</Text>
      {description && <Text className={styles.description}>{description}</Text>}
      {actionText && (
        <View className={styles.action} onClick={onAction}>
          <Text className={styles.actionText}>{actionText}</Text>
        </View>
      )}
    </View>
  );
};

export default EmptyState;
