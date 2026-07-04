import { create } from 'zustand';
import Taro from '@tarojs/taro';
import type { LoginRequest } from '@/types/auth';
import type { CurrentUser } from '@/types/auth';
import { tokenStorage } from '@/services/request';
import { authService } from '@/services/auth';
import { encryptPassword } from '@/utils/passwordCrypto';

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

// store 初始化时同步从 sessionStorage/内存恢复登录态
// 这样组件 mount 时 isAuthenticated 已经是正确的值，避免 useEffect 执行顺序导致的竞态
const _initialAccessToken = tokenStorage.getAccessToken();
const _initialRefreshToken = tokenStorage.getRefreshToken();
const _initialUser = tokenStorage.getUser<CurrentUser>();
// 只要有 access token 就认为已登录（user 可由 restoreAuth 异步补拉）
// 之前要求 token AND user，会导致 refreshUser 失败后刷新丢失登录态
const _initialIsAuthenticated = !!_initialAccessToken;

export const useAuthStore = create<AuthStore>((set, get) => ({
  accessToken: _initialAccessToken,
  refreshToken: _initialRefreshToken,
  user: _initialUser,
  isAuthenticated: _initialIsAuthenticated,
  isLoading: false,

  /**
   * 恢复登录态（store 初始化时已同步恢复，此方法主要用于后台异步刷新用户信息）
   * 在 app.tsx useEffect 中调用
   */
  async restoreAuth() {
    // store 初始化时已同步恢复 isAuthenticated，这里只做后台异步刷新
    if (get().isAuthenticated) {
      try {
        await get().refreshUser();
      } catch {
        // 静默失败：token 可能已过期，请求时 401 会自动触发刷新或重定向
      }
    }
  },

  /** 登录 */
  async login(req: LoginRequest) {
    set({ isLoading: true });
    try {
      // 强制 sha256：客户端绝不发送明文密码
      const encryptedPassword = encryptPassword(req.password);
      const auth = await authService.login({ ...req, password: encryptedPassword });
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
