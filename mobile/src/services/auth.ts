import { http, MOCK_ENABLED } from './request';
import type {
  LoginRequest,
  RegisterRequest,
  RefreshRequest,
  AuthResponse,
  CurrentUser,
} from '@/types/auth';
import { mockLogin, mockGetCurrentUser, mockRefresh } from '@/data/auth';

/**
 * 认证服务
 * 对接后端 /auth/* 接口
 * H5 预览（MOCK_ENABLED）时走 mock 数据，小程序/真机走真实后端
 */
export const authService = {
  /** 登录 */
  async login(req: LoginRequest): Promise<AuthResponse> {
    if (MOCK_ENABLED) {
      return mockLogin(req);
    }
    return http.post<AuthResponse>('/auth/login', req, { auth: false });
  },

  /** 注册 */
  async register(req: RegisterRequest): Promise<{ message: string }> {
    if (MOCK_ENABLED) {
      return { message: '注册成功（mock）' };
    }
    return http.post<{ message: string }>('/auth/register', req, { auth: false });
  },

  /** 刷新 Token */
  async refresh(req: RefreshRequest): Promise<AuthResponse> {
    if (MOCK_ENABLED) {
      return mockRefresh();
    }
    return http.post<AuthResponse>('/auth/refresh', req, { auth: false });
  },

  /** 获取当前用户信息 */
  async getCurrentUser(): Promise<CurrentUser> {
    if (MOCK_ENABLED) {
      return mockGetCurrentUser();
    }
    return http.get<CurrentUser>('/auth/me');
  },
};

export default authService;
