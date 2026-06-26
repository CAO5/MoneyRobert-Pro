import { http, MOCK_ENABLED } from './request';
import type { WorkbenchData } from '@/types/workbench';
import type { TodoItem } from '@/types/todo';
import type { MessageItem } from '@/types/message';
import { mockWorkbench, mockRecentTodos, mockRecentMessages } from '@/data/workbench';

/**
 * 工作台聚合服务（Mobile BFF 模式）
 * 一次返回首屏需要的所有数据，避免多接口并发
 *
 * 后端接口现状：
 * - /dashboard/workbench 聚合接口暂未实现 → mock 兜底（H5 预览）/ 真机走后端时会 404
 * - /tasks/recent 待办审批模块暂未实现 → mock 兜底
 * - /notifications 已有，getRecentMessages 复用该接口取最近 N 条
 */
export const workbenchService = {
  /** 获取工作台聚合数据 */
  async getWorkbench(): Promise<WorkbenchData> {
    if (MOCK_ENABLED) {
      return mockWorkbench();
    }
    return http.get<WorkbenchData>('/dashboard/workbench');
  },

  /** 获取最近待办（工作台卡片用） */
  async getRecentTodos(limit = 5): Promise<TodoItem[]> {
    if (MOCK_ENABLED) {
      return mockRecentTodos(limit);
    }
    // 后端 /tasks 是定时同步任务（非待办审批），待办审批模块暂未实现，暂走 mock 兜底
    return mockRecentTodos(limit);
  },

  /** 获取最近消息 */
  async getRecentMessages(limit = 5): Promise<MessageItem[]> {
    if (MOCK_ENABLED) {
      return mockRecentMessages(limit);
    }
    // 复用 /notifications 接口（后端返回分页对象，取 items）
    const res = await http.get<NotificationListResponse>('/notifications', {
      page_size: limit,
    });
    return res.items;
  },
};

/** 通知列表响应（后端 /notifications 返回的分页对象） */
interface NotificationListResponse {
  items: MessageItem[];
  total: number;
  page: number;
  page_size: number;
  unread_count: number;
}

export default workbenchService;
