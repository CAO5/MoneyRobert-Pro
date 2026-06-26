/**
 * 待办相关类型
 * 用于风险确认、异常处理、审批记录
 */

/** 待办类型 */
export type TodoType =
  | 'risk_confirmation' // 风险确认
  | 'exception_review' // 异常审核
  | 'manual_review' // 人工复核
  | 'promotion_approval' // 升级审批
  | 'alert_acknowledgement'; // 告警确认

/** 待办状态 */
export type TodoStatus = 'pending' | 'approved' | 'rejected' | 'deferred';

/** 待办优先级 */
export type TodoPriority = 'low' | 'medium' | 'high' | 'critical';

/** 待办列表项 */
export interface TodoItem {
  id: string;
  type: TodoType;
  title: string;
  description: string;
  status: TodoStatus;
  priority: TodoPriority;
  symbol?: string; // 关联标的
  job_id?: string; // 关联回测任务
  created_at: string;
  due_at?: string; // 截止时间
  assignee?: string;
}

/** 待办详情 */
export interface TodoDetail extends TodoItem {
  context: {
    risk_level?: string;
    max_acceptable_loss?: number;
    current_loss?: number;
    requirements_met?: boolean;
    missing_requirements?: string[];
  };
  history: Array<{
    action: string;
    operator: string;
    comment?: string;
    timestamp: string;
  }>;
}

/** 待办处理请求 */
export interface ProcessTodoRequest {
  action: 'approve' | 'reject' | 'defer';
  comment?: string;
}

/** 待办类型中文映射 */
export const TODO_TYPE_LABELS: Record<TodoType, string> = {
  risk_confirmation: '风险确认',
  exception_review: '异常审核',
  manual_review: '人工复核',
  promotion_approval: '升级审批',
  alert_acknowledgement: '告警确认',
};

/** 待办状态中文映射 */
export const TODO_STATUS_LABELS: Record<TodoStatus, string> = {
  pending: '待处理',
  approved: '已通过',
  rejected: '已驳回',
  deferred: '已延后',
};

/** 待办优先级中文映射 */
export const TODO_PRIORITY_LABELS: Record<TodoPriority, string> = {
  low: '低',
  medium: '中',
  high: '高',
  critical: '紧急',
};
