import React, { useEffect, useState } from 'react';
import { View, Text } from '@tarojs/components';
import Taro from '@tarojs/taro';
import styles from './index.module.scss';

interface TabItem {
  pagePath: string;
  text: string;
  icon: string;
}

const TABS: TabItem[] = [
  { pagePath: '/pages/workbench/index', text: '首页', icon: '⌂' },
  { pagePath: '/pages/market/index', text: '行情', icon: '◈' },
  { pagePath: '/pages/ai/index', text: 'AI', icon: '✦' },
  { pagePath: '/pages/agent/index', text: 'Agent', icon: '◉' },
  { pagePath: '/pages/message/index', text: '消息', icon: '✉' },
  { pagePath: '/pages/mine/index', text: '我的', icon: '○' },
];

const TAB_PATHS = new Set(TABS.map((t) => t.pagePath));

const CustomTabBar: React.FC = () => {
  const [selected, setSelected] = useState('/pages/workbench/index');
  const [visible, setVisible] = useState(false);

  const syncRoute = () => {
    try {
      const pages = Taro.getCurrentPages();
      if (pages.length > 0) {
        const route = pages[pages.length - 1].route || '';
        const current = route.startsWith('/') ? route : `/${route}`;
        setVisible(TAB_PATHS.has(current));
        const matched = TABS.find((t) => current.startsWith(t.pagePath));
        if (matched) setSelected(matched.pagePath);
      }
    } catch {
      // ignore
    }
  };

  // 首次挂载立即同步一次 + 轮询兜底
  // 注意：不能使用 useDidShow，它是页面级 hook，CustomTabBar 是 app 级子组件，
  // 在非页面组件中使用会触发 Taro 内部异常导致整个 React 树崩溃白页
  useEffect(() => {
    syncRoute();
    const timer = setInterval(syncRoute, 500);
    return () => clearInterval(timer);
  }, []);

  const handleSwitch = (path: string) => {
    setSelected(path);
    Taro.reLaunch({ url: path }).catch(() => {
      Taro.navigateTo({ url: path }).catch(() => {
        // ignore
      });
    });
  };

  if (!visible) return null;

  return (
    <View className={styles.tabBar}>
      {TABS.map((tab) => {
        const isSelected = selected === tab.pagePath;
        return (
          <View
            key={tab.pagePath}
            className={`${styles.tabItem} ${isSelected ? styles.active : ''}`}
            onClick={() => handleSwitch(tab.pagePath)}
          >
            <Text className={styles.tabIcon}>{tab.icon}</Text>
            <Text className={styles.tabText}>{tab.text}</Text>
          </View>
        );
      })}
    </View>
  );
};

export default CustomTabBar;
