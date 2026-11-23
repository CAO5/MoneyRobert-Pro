# WorkBuddy 测试交接 MCP

协调方提交用例 → WorkBuddy 领取并执行 → 回传逐条证据 → 协调方审阅并提交复测任务。

本实现使用 Python MCP SDK 的 FastMCP 和 stdio；WorkBuddy 的连接字段依据[官方连接器文档](https://open.workbuddy.cn/docs/connector)，SDK 使用[官方 v1 文档](https://py.sdk.modelcontextprotocol.io/v1/)。需要 Python 3.10+，依赖固定在 SDK v1 主版本范围。

## 安装与连接

以下命令由负责接入的用户或 WorkBuddy 执行；当前交付未执行安装、启动或连通性验证：

```powershell
cd D:\wan_workspase\MoneyRobert-Pro
python -m venv tools/workbuddy-mcp/.venv
tools/workbuddy-mcp/.venv/Scripts/python.exe -m pip install -r tools/workbuddy-mcp/requirements.txt
```

1. 在 WorkBuddy 的 MCP 配置入口合并 `workbuddy.mcp.json` 中的服务器条目，保留原有服务器配置。
2. 在协调方 MCP 客户端中配置 `coordinator.mcp.json` 描述的同一个命令和参数；JSON 是通用连接参数示例，具体配置格式按客户端要求填写。
3. 两端必须运行在同一台机器上，并使用相同的 `--db` 绝对路径。项目移动后需要同步修改配置路径。
4. 连接后协调方可见 `submit_task/cancel_task/requeue_task`；WorkBuddy 可见 `claim_task/report_results`；两端均可查询 `list_tasks/get_task`。

数据库在已被项目忽略的 `data/workbuddy-mcp/` 下；虚拟环境由本目录 `.gitignore` 忽略。数据库记录用例与证据文本，不应写入密码、令牌或未脱敏个人信息。角色区分用于工具隔离，不构成本机用户间的安全认证边界；能访问数据库或修改启动参数的本机用户拥有数据访问权。

MCP 本身不唤醒 WorkBuddy，也不让协调方执行测试。首次连接后，需要在 WorkBuddy 打开此项目并发送下方执行指令。没有待执行任务时它应返回空闲，不保证后台自动轮询；新任务可再次发送“领取下一个测试任务”。当前 Codex 会话尚未注册此 MCP，交付的配置文件也不会自行改变当前会话工具列表。两端接入完成后，才能通过工具形成完整闭环。

## 协调方用法

调用 `submit_task`，例如：

```json
{
  "title": "登录流程回归",
  "instructions": "仅在已准备好的本地测试环境执行。由 WorkBuddy 检查实际地址和测试账号；缺少条件则 blocked。不得使用真实交易账号。未授权构建、修改源码或启动数据库。",
  "cases": [
    {
      "id": "LOGIN-001",
      "title": "正确账号登录",
      "preconditions": ["用户已提供本地测试地址", "测试账号已存在，密码通过安全渠道提供"],
      "steps": ["打开登录页", "输入测试账号和密码", "提交并记录页面和网络结果"],
      "input_data": "用户指定的有效测试账号；报告不得包含明文密码",
      "expected_result": "登录成功并进入系统，页面不显示异常"
    }
  ]
}
```

上述 JSON 仅为填写示例，未创建任务。实际任务须针对代码改动提供具体前置条件、输入、步骤和预期，并写明允许的环境操作。查询 `get_task(task_id)` 获取结果：`completed` 仅表示报告收齐，应检查每条 `passed/failed/blocked`。

失败复测创建新任务，说明原任务 ID、失败用例和修复内容，保留旧报告。执行者中断后，先确认其外部操作已停止，再 `requeue_task`；旧令牌随即失效，不自动超时重派。`cancel_task` 只更改队列状态，无法终止已经运行的浏览器或系统进程，应同步通知执行者停止。

## 发给 WorkBuddy 的执行指令

```text
在 D:\wan_workspase\MoneyRobert-Pro 项目中，使用 moneyrobert-test-executor MCP：

1. 调用 claim_task，worker 填 WorkBuddy；指定任务 ID 时只领取该任务。
2. 没有任务则报告空闲并结束本轮。保存领取响应中的 task.id 和 claim_token。
3. 阅读 instructions 和全部 cases，核对目标环境及允许执行的操作。
   本次用户允许由你执行测试用例。不要自行扩大到修改测试代码、修复业务代码、
   构建或启动环境；如用例未明确授权这些操作，缺少条件时记录 blocked。
4. 逐条按 preconditions、input_data、steps 执行，比较 expected_result。
   在开始每条用例前用 get_task 确认任务仍在 running；撤销后停止后续操作。
5. 为每条保存实际结果和证据：执行步骤、必要命令、退出码、相关响应摘要，
   或截图/日志绝对路径。路径必须是实际生成的文件，敏感内容先脱敏。
6. 用 report_results 一次提交全部结果，带 task_id、claim_token 和 results。
   每条含 case_id、status（passed/failed/blocked）、actual_result、evidence 数组。
   未执行的用例必须 blocked 并写明阻塞原因；不得凭代码阅读标为 passed。
7. 总结通过、失败、阻塞数量和需协调方处理的问题。除非用户要求持续处理，
   本轮提交后停止，等待下一轮指令。
```

## 交付验收用例（由用户安排执行）

以下均为待执行用例，未验证通过。除安装和连接外，不要求启动本项目业务服务。

| 用例 | 前置条件 | 操作步骤与输入数据 | 预期结果 |
| --- | --- | --- | --- |
| 两端连接及权限 | 已安装依赖，两端指向同一数据库 | 分别连接并查看工具列表；执行方尝试调用 submit_task | 两端连接成功；工具按角色隔离，执行方不能调用提交工具 |
| 完整闭环 | 两端已接入 | 提交包含 C1、C2 的任务；领取；分别回传 passed 和 failed，填写实际结果和证据；协调方查询 | 状态 pending → running → completed；两条结果原样保留，任务完成不等于全部通过 |
| 并发领取 | 仅一项 pending 任务，两个执行方连接 | 同时 claim_task，worker 分别为 WB-A、WB-B | 仅一个领取成功，另一个返回无任务；指定相同任务 ID 时另一方收到状态错误 |
| 结果完整性 | running 任务含 C1、C2 | 分别提交缺 C2、重复 C1、多 C3、非法状态、空证据数组 | 全部拒绝，任务保持 running；补齐合法结果后可提交 |
| 无效凭据 | 有已领取任务 | 用错误 token 回报，随后用原 token 回报合法结果 | 错误 token 被拒绝；原领取者可正常回报 |
| 环境阻塞 | 已领取任务，目标测试环境不可用 | 不启动环境；每条结果填 blocked，并提供原因和检查证据 | 所有阻塞结果可保存，报告无通过记录 |
| 中断恢复 | 已领取任务，确认旧执行者已停止 | 重排后再次领取；用旧 token 回报，再用新 token 回报 | 旧 token 被拒绝；新执行者可提交；无重复已执行操作 |
| 取消与停止 | 一项 pending 和一项 running 任务 | 分别取消；尝试领取前者、回报后者，并通知执行方停止 | 队列操作被拒绝；执行方停止后续步骤；已启动外部操作由执行方处理 |
| 持久化与复测 | 已有完成任务 | 断开并重新连接两端；查询旧任务；提交引用旧任务的新复测任务 | 旧证据保留；复测使用独立任务 ID，不覆盖原报告 |
| 输入边界 | 协调方已连接 | 提交空用例、重复用例 ID；查询不存在任务；list_tasks 的 limit 填 0、101 | 均返回明确错误；limit 为 1、100 时可正常查询 |
| 现有 MCP 回归 | 客户端已有其他 MCP | 合并本服务配置后重新连接，调用原服务工具 | 原配置保留，原工具仍可用 |

仅做源代码和差异静态审查，未编译、未启动 MCP、未执行测试。实际测试证据真实性由执行者记录，桥接服务只校验字段与任务状态，不会检查截图内容或重跑命令。
