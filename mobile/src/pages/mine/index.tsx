import React from 'react';
import { View, Text, Button } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { useAuthStore } from '@/store/auth';
import EmptyState from '@/components/EmptyState';
import styles from './index.module.scss';

/**
 * 我的页（tabBar）
 * 展示用户信息、统计、设置入口、退出登录
 */
const MinePage: React.FC = () => {
  const { user, isAuthenticated, logout } = useAuthStore();

  // 未登录显示登录引导
  if (!isAuthenticated) {
    return (
      <View className={styles.minePage}>
        <EmptyState
          title="未登录"
          description="登录后可查看个人信息与设置"
          actionText="去登录"
          onAction={() => Taro.reLaunch({ url: '/pages/login/index' })}
        />
      </View>
    );
  }

  // 跳转设置
  const handleNavigate = (url: string) => {
    Taro.navigateTo({ url }).catch(() => {
      Taro.showToast({ title: '页面不存在', icon: 'none' });
    });
  };

  // 退出登录二次确认
  const handleLogout = () => {
    Taro.showModal({
      title: '退出登录',
      content: '确定退出当前账号吗？',
      confirmText: '退出',
      confirmColor: '#f53f3f',
      success: (res) => {
        if (res.confirm) {
          logout();
        }
      },
    });
  };

  // 用户首字头像
  const avatarChar = (user?.username || 'U').slice(0, 1).toUpperCase();

  const menuGroups: Array<{
    items: Array<{ icon: string; label: string; route?: string; onClick?: () => void }>;
  }> = [
    {
      items: [
        { icon: '设', label: '设置', route: '/pages/settings/index' },
        { icon: '设', label: '设备与安全', route: '/pages/settings/index?type=security' },
      ],
    },
    {
      items: [
        { icon: '告', label: '通知设置', route: '/pages/settings/index?type=notification' },
        { icon: '显', label: '显示设置', route: '/pages/settings/index?type=display' },
      ],
    },
    {
      items: [
        { icon: '帮', label: '帮助与反馈', onClick: () => Taro.showToast({ title: '请前往桌面端查看', icon: 'none' }) },
        { icon: '关', label: '关于 MoneyRobert', onClick: () => Taro.showToast({ title: 'v1.0.0', icon: 'none' }) },
      ],
    },
  ];

  return (
    <View className={styles.minePage}>
      {/* 用户信息 */}
      <View className={styles.userHeader}>
        <View className={styles.userInfo}>
          <View className={styles.avatar}>
            <Text>{avatarChar}</Text>
          </View>
          <View className={styles.userDetail}>
            <Text className={styles.username}>{user?.username || '未登录'}</Text>
            {user?.email && <Text className={styles.userEmail}>{user.email}</Text>}
            {user?.role && <Text className={styles.roleTag}>{user.role}</Text>}
          </View>
        </View>
      </View>

      {/* 统计卡 */}
      <View className={styles.statsRow}>
        <View className={styles.statItem}>
          <Text className={styles.statValue}>12</Text>
          <Text className={styles.statLabel}>本月操作</Text>
        </View>
        <View className={styles.statItem}>
          <Text className={styles.statValue}>3</Text>
          <Text className={styles.statLabel}>待办任务</Text>
        </View>
        <View className={styles.statItem}>
          <Text className={styles.statValue}>5</Text>
          <Text className={styles.statLabel}>未读消息</Text>
        </View>
      </View>

      {/* 菜单分组 */}
      {menuGroups.map((group, gIdx) => (
        <View key={gIdx} className={styles.menuGroup}>
          {group.items.map((item, idx) => (
            <View
              key={idx}
              className={styles.menuItem}
              onClick={() => (item.onClick ? item.onClick() : handleNavigate(item.route || ''))}
            >
              <View className={styles.menuIcon}>
                <Text>{item.icon}</Text>
              </View>
              <Text className={styles.menuLabel}>{item.label}</Text>
              <Text className={styles.menuArrow}>›</Text>
            </View>
          ))}
        </View>
      ))}

      <Button className={styles.logoutButton} onClick={handleLogout}>
        退出登录
      </Button>
    </View>
  );
};

export default MinePage;
