import { create } from 'zustand';
import Taro from '@tarojs/taro';
import type { LoginRequest } from '@/types/auth';
import type { CurrentUser } from '@/types/auth';
import { tokenStorage } from '@/services/request';
import { authService } from '@/services/auth';

/**
 * 认证状态管理（zustand）
 * 负责登录态管理与用户信息获取；Token 仅保存在当前进程内存。
 */
interface AuthStore {
  // 状态
  accessToken: string | null;
  refreshToken: string | null;
  user: CurrentUser | null;
  isAuthenticated: boolean;
  isLoading: boolean;

  // 动作
  restoreAuth: () => Promise<void>;
  login: (req: LoginRequest) => Promise<void>;
  logout: () => void;
  refreshUser: () => Promise<void>;
}

export const useAuthStore = create<AuthStore>((set, get) => ({
  accessToken: null,
  refreshToken: null,
  user: null,
  isAuthenticated: false,
  isLoading: false,

  /**
   * 检查当前进程内存中的登录态（应用重启后需要重新登录）
   * 在 app.tsx useEffect 中调用
   */
  async restoreAuth() {
    const accessToken = tokenStorage.getAccessToken();
    const refreshToken = tokenStorage.getRefreshToken();
    const user = tokenStorage.getUser<CurrentUser>();

    if (accessToken && user) {
      set({
        accessToken,
        refreshToken,
        user,
        isAuthenticated: true,
      });
      // 后台异步刷新用户信息
      try {
        await get().refreshUser();
      } catch {
        // 静默失败：不把认证错误对象写入终端或远程日志。
      }
    }
  },

  /** 登录 */
  async login(req: LoginRequest) {
    set({ isLoading: true });
    try {
      const auth = await authService.login(req);
      tokenStorage.setTokens(auth.access_token, auth.refresh_token);
      set({
        accessToken: auth.access_token,
        refreshToken: auth.refresh_token,
        isAuthenticated: true,
      });
      // 拉取用户信息
      await get().refreshUser();
    } finally {
      set({ isLoading: false });
    }
  },

  /** 退出登录 */
  logout() {
    tokenStorage.clearTokens();
    tokenStorage.clearUser();
    set({
      accessToken: null,
      refreshToken: null,
      user: null,
      isAuthenticated: false,
    });
    Taro.reLaunch({ url: '/pages/login/index' });
  },

  /** 刷新当前用户信息（从 /auth/me） */
  async refreshUser() {
    const user = await authService.getCurrentUser();
    tokenStorage.setUser(user);
    set({ user });
  },
}));

export default useAuthStore;
