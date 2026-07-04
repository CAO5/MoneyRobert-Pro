import React, { useEffect } from 'react';
import { useAuthStore } from '@/store/auth';
import { useLanguageStore } from '@/store/language';
import { initializePrivacyProtection } from '@/security/privacy';
import CustomTabBar from '@/custom-tab-bar';
import ErrorBoundary from '@/components/ErrorBoundary';
// 全局样式
import './app.scss';

/**
 * 应用根组件
 * 负责应用启动时的初始化工作：
 * - 初始化内存会话与隐私防护
 * - 渲染自定义底部导航（绕过 Taro H5 默认 tabBar 的 stencil 组件 bug）
 *
 * 注意：不能在此使用 useDidShow/useDidHide，它们是页面级生命周期 hook，
 * 在 App 根组件中使用会触发 Taro 内部异常导致整个应用白页崩溃。
 */
function App(props: React.PropsWithChildren<unknown>) {
  const restoreAuth = useAuthStore((state) => state.restoreAuth);
  const initializeLanguage = useLanguageStore((state) => state.initialize);

  useEffect(() => {
    initializeLanguage();
    initializePrivacyProtection();
    // 同步检查 sessionStorage 中是否有 token（H5 端刷新后可恢复）
    // restoreAuth 内部会异步刷新用户信息，但不阻塞渲染
    restoreAuth();
  }, [initializeLanguage, restoreAuth]);

  return (
    <ErrorBoundary>
      {props.children}
      <CustomTabBar />
    </ErrorBoundary>
  );
}

export default App;
