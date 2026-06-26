import React from 'react';
import { View } from '@tarojs/components';
import classnames from 'classnames';
import styles from './index.module.scss';

/**
 * 通用卡片组件
 * 遵循深度研究报告"卡片式设计 + 12-16rpx 圆角 + 阴影"规范
 */
export interface CardProps {
  /** 卡片标题（可选） */
  title?: string;
  /** 标题右侧附加内容 */
  extra?: React.ReactNode;
  /** 是否使用渐变背景（用于关键指标卡） */
  gradient?: boolean;
  /** 是否可点击（添加 active 反馈） */
  clickable?: boolean;
  /** 内边距尺寸：默认 32rpx，紧凑 24rpx，宽松 48rpx */
  padding?: 'sm' | 'md' | 'lg';
  children?: React.ReactNode;
  className?: string;
  onClick?: () => void;
}

const Card: React.FC<CardProps> = ({
  title,
  extra,
  gradient = false,
  clickable = false,
  padding = 'md',
  children,
  className,
  onClick,
}) => {
  const rootClass = classnames(
    styles.card,
    styles[`padding_${padding}`],
    gradient && styles.gradient,
    clickable && styles.clickable,
    className,
  );

  return (
    <View className={rootClass} onClick={onClick}>
      {(title || extra) && (
        <View className={styles.header}>
          {title && <View className={styles.title}>{title}</View>}
          {extra && <View className={styles.extra}>{extra}</View>}
        </View>
      )}
      <View className={styles.body}>{children}</View>
    </View>
  );
};

export default Card;
