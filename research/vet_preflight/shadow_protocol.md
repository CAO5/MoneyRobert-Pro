# VET v0.2 影子前向观察

独立 Rust 进程用 OKX 公共 `books` 和聚合 `trades` 观察 BTC、ETH、SOL 永续。使用 MoneyRobert PostgreSQL 中的 `vet_shadow_*` 研究表。

## 冻结口径

- 每 10 秒在交易所时钟上封一桶。最近 60 秒聚合成交消息数与此前 10 分钟每分钟均值比较得到 A。底层 `count` 合计另存。盘口在中间价两侧各 2 bp 内，以单档价格乘合约张数、面值和乘数得到 USDT 深度。两侧均达到边界才有完整覆盖。
- 用两侧深度合计的时长加权中位数计算 `D_recent` [t−30s,t) 和 `D_base` [t−630s,t−30s)。两者均为正时计算 `L_depth=max(0,ln(D_base/D_recent))` 和 `VET=A×L_depth`。每条记录保存最近窗的三种深度中位数，同时保存买卖两侧和最后报价的诊断。
- 每桶接收截止为桶末加 5 秒。盘口从快照和连续增量构造。断线、错序、缺档和迟到均显式处理。对已落库时点发现的缺口通过 `INPUT_REVOKED` 状态排除。研究样本要求 A、L_depth 和过去 1、5、30 分钟 RV 均有效。
- 中间价使用每个整秒之前的最近一笔健康盘口，连续健康期间可沿用无变化的报价。RV 等于窗口内全部 1 秒中间价对数收益平方和的平方根，实现方差保存平方和。起点为记录提交且 `available_at` 已过后的首个 UTC 整秒，以同一中间价规则取价。缺失起点或未来网格时，相应标签记空并保留状态。到期窗口封桶后才落库，数据库保证 `origin_ts>available_at` 和 `completed_at>=target_ts`。
- 诊断按资产和 UTC 小时统计。连续事件簇是同一资产中相邻 10 秒、有效且 VET > 0 的连续区间。报告非零点的同期 30 秒绝对中间价移动、买卖分侧收缩，以及 2 bp 区间中心移动与总深度对数比的相关性。24 小时检查点只统计本轮前 24 小时因子记录，等待最后一批 15 分钟标签封桶后持久化。

## B0–B4 样本外协议

| 对照 | 输入 |
|---|---|
| B0 | 历史 1、5、30 分钟 RV |
| B1 | B0 + A |
| B2 | B0 + L_depth |
| B3 | B0 + A + L_depth |
| B4 | B0 + A + L_depth + VET |

评价目标为未来 5 分钟实现方差。只使用完整标签和有效样本。同一资产先按起点顺序保留不重叠的未来收益区间，再按已结束的 UTC 日分块，每日最低 30 条。每折训练标签须在测试日开始前完成。每资产需至少 7 个合格训练日和 300 条训练样本，并达到至少 3 个合格测试日。所有资产均通过后，才允许显式执行评价命令。

五组使用相同折和简单的对数方差线性拟合。B0 的历史 RV 先平方成方差后取对数。取对数的方差和预测下限统一为 1e−12。主指标为 QLIKE，即 log(预测方差) + 实现方差 / 预测方差。B4 相对 B3 的增量为同一测试块上 B3 减 B4 的 QLIKE。报告按 UTC 日保留配对差值。

## 运行与读取

在项目根目录设置 `VET_DATABASE_URL` 后运行。

```powershell
cargo build --manifest-path backend/Cargo.toml --locked --bin vet-shadow
./backend/target/debug/vet-shadow.exe run data/vet-shadow
./backend/target/debug/vet-shadow.exe report data/vet-shadow
```

状态命令保存 `data/vet-shadow/live_status.json`。完整 24 小时报告写入 `vet_shadow_checkpoints` 和 `data/vet-shadow/<run_id>-24h.json`。若采集中断并重新启动，旧档仍可查询，新运行重新建立回看窗口。

通过 PostgreSQL 执行 `shadow_five_minute.sql` 导出 CSV，然后运行 `python research/vet_preflight/shadow_evaluate.py <CSV>`。默认只检查评价门槛。正式评价使用显式的 `--evaluate`。
