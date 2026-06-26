/**
 * 认证相关类型定义
 * 对接后端 /auth/register、/auth/login、/auth/refresh、/auth/me 接口
 */

/** 登录请求 */
export interface LoginRequest {
  username: string;
  password: string;
}

/** 注册请求 */
export interface RegisterRequest {
  username: string;
  email: string;
  password: string;
}

/** Token 刷新请求 */
export interface RefreshRequest {
  refresh_token: string;
}

/** 认证响应（登录/注册/刷新统一格式） */
export interface AuthResponse {
  access_token: string;
  refresh_token: string;
  token_type: string; // "bearer"
  expires_in: number; // 秒
}

/** 当前用户信息（来自 /auth/me） */
export interface CurrentUser {
  id: number;
  username: string;
  email: string;
  role: string; // NORMAL / ADMIN ...
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

/** 本地存储的认证状态 */
export interface AuthState {
  accessToken: string | null;
  refreshToken: string | null;
  user: CurrentUser | null;
  isAuthenticated: boolean;
}
