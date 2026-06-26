import { http, MOCK_ENABLED } from './request';
import type { MessageItem } from '@/types/message';
import { mockMessages, mockMarkRead } from '@/data/message';

/**
 * 消息服务
 * 对接后端 /notifications/* 接口
 *
 * 后端实际路由（见 backend/src/routes/notifications.rs）：
 * - GET  /notifications            分页查询，返回 {items, total, page, page_size, unread_count}
 * - PUT  /notifications/{id}/read  标记单条已读（注意是 PUT，非 POST）
 * - PUT  /notifications/read-all   标记全部已读（注意是 PUT，非 POST）
 */
export const messageService = {
  /** 查询消息列表 */
  async listMessages(params?: { type?: string; status?: string }): Promise<MessageItem[]> {
    if (MOCK_ENABLED) {
      return mockMessages(params);
    }
    // 后端返回分页对象 {items, total, ...}，这里取 items 适配 mobile 的数组期望
    const res = await http.get<NotificationListResponse>('/notifications', {
      type: params?.type,
      // status=unread 映射为后端 is_read=false 过滤
      is_read: params?.status === 'unread' ? false : undefined,
      page_size: 50,
    });
    return res.items;
  },

  /** 标记消息为已读 */
  async markRead(id: string): Promise<void> {
    if (MOCK_ENABLED) {
      return mockMarkRead(id);
    }
    // 后端为 PUT 方法（非 POST），与 notifications.rs 路由定义一致
    return http.put<void>(`/notifications/${id}/read`);
  },

  /** 标记全部已读 */
  async markAllRead(): Promise<void> {
    if (MOCK_ENABLED) {
      return mockMarkRead('all');
    }
    // 后端为 PUT 方法（非 POST）
    return http.put<void>('/notifications/read-all');
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

export default messageService;
