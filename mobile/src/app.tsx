import React, { useEffect } from 'react';
import { useDidShow, useDidHide } from '@tarojs/taro';
import { useAuthStore } from '@/store/auth';
import { useLanguageStore } from '@/store/language';
import { initializePrivacyProtection } from '@/security/privacy';
// 全局样式
import './app.scss';

/**
 * 应用根组件
 * 负责应用启动时的初始化工作：
 * - 初始化内存会话与隐私防护
 * - 监听应用显示/隐藏生命周期
 */
function App(props: React.PropsWithChildren<unknown>) {
  // 从存储恢复认证状态
  const restoreAuth = useAuthStore((state) => state.restoreAuth);
  const initializeLanguage = useLanguageStore((state) => state.initialize);

  useEffect(() => {
    initializeLanguage();
    initializePrivacyProtection();
    // 只检查当前进程内存会话；应用重启不会恢复敏感凭证。
    restoreAuth();
  }, [initializeLanguage, restoreAuth]);

  // 应用进入前台
  useDidShow(() => {
    // 可在此处做被动刷新、消息角标同步等
  });

  // 应用进入后台
  useDidHide(() => {
    // 可在此处做数据持久化、定时任务暂停等
  });

  return props.children;
}

export default App;
