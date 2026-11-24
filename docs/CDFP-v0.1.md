# AI Fund OS · Research Session 002

## CDFP v0.1 候选规格

研究假设是在非极端行情中，现货与永续持续同向的主动成交压力与同向的未来 5 分钟收益有关，并且在样本外控制价格动量和单市场订单流后仍有增量预测表现。

### 仓库检查

- [OKX 录制器](../research/okx_behavior_tape.py)已有 WebSocket 订阅、重连、接收时钟和断线记录。默认采集 BTC、ETH、DOGE 永续的聚合成交及盘口。聚合成交审计可借助 `tradeId` 和 `count` 检查缺口，但默认市场组合和频道均不满足本次研究口径。
- [归档器](../research/forward_tape.py)已有分片压缩、接收时间和 SHA-256 清单。仓库检查时未发现可直接复用的 BTC、ETH、SOL 现货和永续六市场逐笔归档。旧研究输出保留在各自的文件和流程中。
- CDFP 复用上述录制器和归档器，通过六条独立连接订阅 OKX 公开 `trades-all`，以现货逐笔成交提供价格快照。订阅字段和成交单位参见 [OKX 逐笔成交通道](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-all-trades-channel)。

### 计算和时间口径

1. 用 OKX 成交时间把事件归入 UTC 左闭右开的 10 秒桶。每桶结束后等待 5 秒接收迟到数据。现货名义金额为成交价乘基础币数量，USDT 线性永续再按合约面值和乘数把张数转换成基础币数量。每次运行获取并归档合约规格。普通成交和增强流动性计划成交均计入。 [OKX 合约规格接口](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments)
2. 每个市场的失衡为（买入名义额 − 卖出名义额）÷ 总名义额。六个桶分别用两侧同号时保留符号的较小绝对值，其他有效情况取 0。六个贡献的算术平均为 CDFP。`spot_imbalance` 和 `perp_imbalance` 保存各自六桶的等权平均，单桶数值和买卖名义额写入 `bins`。`same_direction_bins` 统计两市场的失衡均非零且同号的桶数。
3. 健康桶要求该市场在桶开始时已订阅且在桶结束后收到成交或心跳响应。健康且无成交返回 0。断线、成交 ID 缺口、无效数据或超出等待时间的成交使对应桶无效。六个桶任一无效时 CDFP 为空。后续发现缺口时将受影响记录标记为 `GAP`，同时保留首次发出的得分 `issued_cdfp_value`、原始桶和已经采集的价格标签供审计。
4. 使用每个健康现货桶内最后一笔成交作为边界价格。动量是过去 60 秒的对数收益。15 分钟实现波动率为相邻 10 秒价格对数变化的平方和开根号，要求区间两端都有效且至少有 81 个有效相邻变化。参考分布只有当前窗口之前滚动 24 小时的观测，每项至少有 90% 的 10 秒观察点才可使用。当前绝对 1 分钟收益严格低于 P99、实现波动率严格低于 P95 时为 `NORMAL`，否则为 `EXTREME`。缺少必要数据或未满 24 小时时为 `WARMUP`。
5. 成交额门槛按资产及市场分别取此前 24 小时健康、正成交额桶的 P10，使用相同的最低参考覆盖率。六桶内任意市场低于自己的门槛时记录 `LOW`，无门槛时为 `UNKNOWN`。两者均保留原始分数。
6. `factor_time` 是观察区间结束时刻，`available_time` 是计算完成后实际可用的时刻。价格起点使用两种时钟都达到 `available_time` 后收到的第一笔合格现货成交，以其接收时刻为预测起点。1、5、15 分钟目标以该接收时刻计算。起点与各目标等待第一笔合格成交最多 10 秒，保存价格事件时间、接收时间和标签完成时间。标签只看价格是否可得，后续出现极端行情仍保留收益。

### 运行

MoneyRobert 的 Rust 后端可随服务启动 CDFP。数据库自动应用 `034_cdfp_research.sql`，由一个持有 PostgreSQL 会话锁的采集任务运行六路 OKX 公开连接。后端将合约规格、六桶成交额、分数和收益标签写入 `cdfp_runs` 和 `cdfp_samples`。后端重启会开启新一轮预热，之前的运行和样本保留在 PostgreSQL 中。采集任务需要网络能直连 OKX 公共 REST 与 WebSocket 地址。

开发环境启用方式

```powershell
$env:CDFP_ENABLED = 'true'
docker compose up -d --build backend
```

本地 Cargo 启动时也可设置同一变量。登录后从「数据监控 → CDFP 研究」进入页面。登录态 API 为 `GET /api/v1/research/cdfp/status` 和 `GET /api/v1/research/cdfp/report`，默认显示最近一轮运行。

### Python 研究校验

在仓库根目录运行，Python 需要安装 `aiohttp`。研究校验工具可以保存逐笔原始归档和单独的 SQLite 样本。以下命令分别运行采集和读取报告。使用新建的输出目录，持续时长取决于磁盘预算和成交密度。

```powershell
python research/cdfp.py data/cdfp-session-002 --seconds 604800 --max-gib 8
python research/cdfp_report.py data/cdfp-session-002/cdfp.sqlite
python -m unittest discover -s research -p test_cdfp.py
```

`cdfp.sqlite` 存储样本和收益标签。`raw` 存储逐笔行情及校验清单，`instruments.json` 保存此次运行的合约规格。可在 `raw` 目录创建 `STOP` 文件结束采集。现有录制器会在到达归档上限或剩余磁盘空间低于 10 GiB 时停止，并把原因写入 `raw/summary.json`。每次启动独立预热，当前原型没有跨进程续跑。

### 研究报告

报告按 BTC、ETH、SOL 输出原始分数分布、缺口和各类排除比例、固定强度组的 5 分钟条件收益，以及按 UTC 日分块的样本外表现。排除比例各自以全量样本为分母，同一记录可能触发多个质量条件。

主分析仅纳入当前处于 `NORMAL`、订单流质量和成交活跃度均通过且 5 分钟已结标的样本。对每个资产按时间只保留价格区间不重叠的观测。每个测试日使用此前可用、且在测试日开始前完成标签的训练样本，最低训练量为 30 条。对照为过去 1 分钟动量、单独的现货流、单独的永续流，以及三者联合的线性模型。比较联合模型加入 CDFP 前后的测试均方误差，并报告各测试日分数与收益的相关性和样本外分组收益。MSE 降幅为正表示该测试集的预测误差下降。样本不足时报告 `AWAITING_SAMPLE`。

七天是首次阶段性检查点。跨日方向稳定性、分组条件收益的单调性以及加入 CDFP 后的增量表现需要真实前向样本验证。
