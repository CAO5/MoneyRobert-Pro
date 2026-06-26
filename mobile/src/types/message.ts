/**
 * 消息相关类型
 * 对接后端 /notifications/* 接口
 */

/** 消息类型 */
export type MessageType =
  | 'system' // 系统通知
  | 'business' // 业务告警
  | 'risk' // 风险提醒
  | 'approval' // 审批通知
  | 'promotion'; // 升级通知

/** 消息状态 */
export type MessageStatus = 'unread' | 'read';

/** 消息列表项 */
export interface MessageItem {
  id: string;
  type: MessageType;
  title: string;
  content: string;
  status: MessageStatus;
  priority: 'low' | 'medium' | 'high';
  created_at: string;
  // 点击消息后跳转的页面路径（深链）
  link?: {
    page: string;
    params?: Record<string, string>;
  };
}

/** 消息类型中文映射 */
export const MESSAGE_TYPE_LABELS: Record<MessageType, string> = {
  system: '系统通知',
  business: '业务告警',
  risk: '风险提醒',
  approval: '审批通知',
  promotion: '升级通知',
};
