import { create } from 'zustand';
import Taro from '@tarojs/taro';

/**
 * 应用全局状态
 * 管理网络状态、设备信息、平台差异等
 */
interface AppStore {
  // 网络状态
  networkType: string;
  isOnline: boolean;

  // 设备信息
  systemInfo: Taro.getSystemInfoSync.Result | null;
  statusBarHeight: number;

  // 业务状态
  unreadMessageCount: number;
  pendingTodoCount: number;

  // 动作
  setNetworkStatus: (networkType: string, isOnline: boolean) => void;
  initSystemInfo: () => void;
  setUnreadMessageCount: (count: number) => void;
  setPendingTodoCount: (count: number) => void;
}

export const useAppStore = create<AppStore>((set) => ({
  networkType: 'wifi',
  isOnline: true,
  systemInfo: null,
  statusBarHeight: 0,
  unreadMessageCount: 0,
  pendingTodoCount: 0,

  setNetworkStatus(networkType, isOnline) {
    set({ networkType, isOnline });
  },

  initSystemInfo() {
    try {
      const systemInfo = Taro.getSystemInfoSync();
      set({
        systemInfo,
        statusBarHeight: systemInfo.statusBarHeight || 0,
      });
    } catch {
    }
  },

  setUnreadMessageCount(count) {
    set({ unreadMessageCount: count });
  },

  setPendingTodoCount(count) {
    set({ pendingTodoCount: count });
  },
}));

export default useAppStore;
