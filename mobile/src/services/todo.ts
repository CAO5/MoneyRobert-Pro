import { MOCK_ENABLED } from './request';
import type { TodoItem, TodoDetail, ProcessTodoRequest } from '@/types/todo';
import { mockTodos, mockTodoDetail, mockProcessTodo } from '@/data/todo';

/**
 * 待办服务
 *
 * 后端接口现状：
 * - /tasks 是定时同步任务（数据采集/聚合调度），语义与"待办审批"不同
 * - 待办审批模块（/todos/*）暂未实现 → 全部 mock 兜底
 *
 * TODO: 后端补齐待办审批模块后，切换为真实接口
 *   - GET  /todos          查询待办列表
 *   - GET  /todos/{id}     查询待办详情
 *   - POST /todos/{id}/process  处理待办（通过/驳回/延后）
 */
export const todoService = {
  /** 查询待办列表 */
  async listTodos(params?: { status?: string; type?: string }): Promise<TodoItem[]> {
    if (MOCK_ENABLED) {
      return mockTodos(params);
    }
    // 待办审批模块暂未实现，mock 兜底
    return mockTodos(params);
  },

  /** 查询待办详情 */
  async getTodo(id: string): Promise<TodoDetail> {
    if (MOCK_ENABLED) {
      return mockTodoDetail(id);
    }
    // 待办审批模块暂未实现，mock 兜底
    return mockTodoDetail(id);
  },

  /** 处理待办（通过/驳回/延后） */
  async processTodo(id: string, req: ProcessTodoRequest): Promise<TodoItem> {
    if (MOCK_ENABLED) {
      return mockProcessTodo(id, req);
    }
    // 待办审批模块暂未实现，mock 兜底
    return mockProcessTodo(id, req);
  },
};

export default todoService;
