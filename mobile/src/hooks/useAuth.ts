import { useEffect } from 'react';
import { useAuthStore } from '@/store/auth';
import Taro from '@tarojs/taro';

/**
 * 鉴权 Hook
 * 封装登录态检查、未登录跳转、加载状态
 */
export function useAuth() {
  const { isAuthenticated, user, isLoading, login, logout } = useAuthStore();

  /**
   * 检查登录态，未登录则跳转登录页
   * 在需要登录的页面 useEffect 中调用
   */
  const requireAuth = () => {
    useEffect(() => {
      if (!isAuthenticated && !isLoading) {
        Taro.reLaunch({ url: '/pages/login/index' });
      }
    }, [isAuthenticated, isLoading]);
  };

  return {
    isAuthenticated,
    user,
    isLoading,
    login,
    logout,
    requireAuth,
  };
}

export default useAuth;
