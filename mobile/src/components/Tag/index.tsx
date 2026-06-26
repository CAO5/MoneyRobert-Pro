import React from 'react';
import { View, Text } from '@tarojs/components';
import classnames from 'classnames';
import styles from './index.module.scss';

/**
 * 标签/徽章组件
 * 用于状态标识、可信等级、优先级等
 */
export type TagVariant =
  | 'default'
  | 'primary'
  | 'success'
  | 'warning'
  | 'error'
  | 'risk'
  | 'premium';

export interface TagProps {
  variant?: TagVariant;
  /** 自定义颜色（覆盖 variant） */
  color?: string;
  /** 自定义背景色（覆盖 variant） */
  bgColor?: string;
  size?: 'sm' | 'md';
  children?: React.ReactNode;
  className?: string;
}

const Tag: React.FC<TagProps> = ({
  variant = 'default',
  color,
  bgColor,
  size = 'sm',
  children,
  className,
}) => {
  const rootClass = classnames(
    styles.tag,
    styles[`variant_${variant}`],
    styles[`size_${size}`],
    className,
  );

  const customStyle: React.CSSProperties = {};
  if (color || bgColor) {
    if (color) customStyle.color = color;
    if (bgColor) customStyle.backgroundColor = bgColor;
  }

  return (
    <View className={rootClass} style={customStyle}>
      <Text className={styles.text}>{children}</Text>
    </View>
  );
};

export default Tag;
