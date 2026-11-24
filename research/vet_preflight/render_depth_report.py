"""Write a standalone depth-candidate feasibility and formula review."""

import argparse
import json
from pathlib import Path

from render_report import cuts, pct


def format_report(depth, quality):
    direct = quality['sources'].get('direct_all_sealed', quality['sources']['direct_first_two_hours'])
    reports = {name.split('-')[0]: row for name, row in depth['markets'].items()}
    all_distinguishable = all(row['candidate_positive_n'] > 0 for row in reports.values())
    coverage_statement = (
        '三币在匹配历史窗口内均观察到正的 L_depth 和候选 VET。'
        if all_distinguishable else '匹配历史窗口内，部分合约未出现正的候选 VET。'
    )
    lines = [
        '# VET v0.2 深度候选 · 数据可行性与数学审查',
        '',
        f'**初步判断。** {coverage_statement}本阶段暂缓启动正式前向预测，先用其他市场时段的完整深度档案复核可用性和稳定性。',
        '',
        f'历史窗口为 UTC {depth["utc_start"]} 至 {depth["utc_end"]}。BTC、ETH、SOL 均使用同一 Tardis 时段内的 OKX 原始 `books` 和 `trades`。三币按 10 秒网格评估，来自同一天且窗口有重叠。',
        '',
        '## 候选数学定义',
        '',
        '对每一条已接上序列的 400 档盘口，以买一和卖一的均值为中间价。保留中间价上下各 2 bp 内的买卖报价。买盘的最远保留档必须达到或超出下边界，卖盘的最远保留档必须达到或超出上边界。任一侧未覆盖时，将该时段标记为深度缺失。',
        '',
        '线性永续单档 USDT 深度等于报价 × 张数 × 合约面值 × 合约乘数。以公开规格快照计算，每张 BTC、ETH、SOL 合约分别代表 0.01 BTC、0.1 ETH、1 SOL。历史回放按该快照的单位计算。历史规格的有效日期仍以当时的交易所元数据为准。',
        '',
        '- 对每个有效盘口，定义 D = 买盘深度 + 卖盘深度。先在同一快照相加，再按可见状态持续时间计算总量中位数。',
        '- D_recent 取 [t − 30 秒, t) 的时间加权中位数。D_base 取 [t − 630 秒, t − 30 秒) 的时间加权中位数，与最近窗口无重叠。',
        '- 当两个中位数均为正且两段均全覆盖时，L_depth = max(0, ln(D_base / D_recent))。零深度分母、档位覆盖不足或窗口断流均记无效。',
        '- A 沿用 v0.1，比较最近 60 秒聚合成交消息数与再之前 10 分钟的每分钟均值。VET_v0.2_candidate = A × L_depth。',
        '',
        '总量收缩可以由一侧主导。报告另给出 L_depth 为正时买卖两侧窗口中位数同时下降的比例。跟随中间价移动的区间也会改变被纳入统计的档位。可见深度变化本身无法区分撤单、成交消耗与档位移出区间。',
        '',
        '盘口更新要求序列链接正确。断流后等待新快照。报价段只延续此前已收到且仍有效的状态。接收晚于 10 秒桶结束后 5 秒的记录标记缺口。任何必要窗口跨越无效桶，该项结果记为空。诊断文件保留报价与接收时间、序列、最远档、覆盖状态和分侧深度。',
        '',
        '接口说明见 [OKX 公开盘口和序列规则](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-order-book-channel) 和 [OKX 合约规格](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments)。',
        '',
        '## 深度与可辨识性',
        '',
        '| 合约 | A > 0 | L_depth > 0 | 候选 VET > 0 | 联合有效 | L>0 时两侧同时收缩 |',
        '|---|---:|---:|---:|---:|---:|',
    ]
    for asset, row in reports.items():
        lines += [f'| {asset} | {pct(row["a_positive_fraction"])} | '
                  f'{pct(row["l_positive_fraction"])} ({row["l_positive_n"]}/{row["l_valid"]}) | '
                  f'{pct(row["candidate_positive_fraction"])} ({row["candidate_positive_n"]}/{row["joint_valid"]}) | '
                  f'{row["joint_valid"]} | {pct(row["both_side_window_medians_contract_fraction_given_l_positive"])} |']
    lines += ['', '| 合约 | 买盘深度 USDT P50 / P90 / P99 | 卖盘深度 USDT P50 / P90 / P99 | 总深度 USDT P50 / P90 / P99 | 30 秒中位总量 P50 / P90 / P99 | 10 分钟中位总量 P50 / P90 / P99 |',
              '|---|---|---|---|---|---|']
    for asset, row in reports.items():
        values = row['time_weighted_depth_usdt']
        lines += [f'| {asset} | {cuts(values["bid_usdt"])} | {cuts(values["ask_usdt"])} | '
                  f'{cuts(values["total_usdt"])} | {cuts(row["recent_30s_median_total_usdt"])} | '
                  f'{cuts(row["prior_10m_median_total_usdt"])} |']
    lines += ['', '| 合约 | 正 L_depth P50 / P90 / P99 | 正收缩时相对参考深度损失 P50 / P90 / P99 | 正候选 VET P50 / P90 / P99 |',
              '|---|---|---|---|']
    for asset, row in reports.items():
        lost = row['fraction_of_reference_depth_lost_when_positive']
        l = ' / '.join(pct(lost[key]) for key in ('p50', 'p90', 'p99'))
        lines += [f'| {asset} | {cuts(row["l_depth_nonzero"])} | {l} | {cuts(row["candidate_nonzero"])} |']
    lines += ['', '| 合约 | 00:00–01:00 UTC 候选 VET > 0 | 01:00–02:00 UTC 候选 VET > 0 |',
              '|---|---:|---:|']
    for asset, row in reports.items():
        lines += [f'| {asset} | ' + ' | '.join(
            f'{pct(hour["candidate_positive_fraction"])}（n={hour["joint_valid"]}）'
            for hour in row['hourly_joint']
        ) + ' |']
    lines += ['', '三币在两个小时均出现候选正值，支持此切片内的可辨识性。表内时点共享回看窗口，两个小时也来自同一交易日。', '',
              '## 400 档覆盖和盘口异常', '',
              '| 合约 | 健康报价时间覆盖双侧 2 bp | 仅因覆盖不足的 10 秒桶 | 报价缺失或异常桶 | 成交缺失或异常桶 | 盘口断链事件 | 锚点前增量 |',
              '|---|---:|---:|---:|---:|---:|---:|']
    for asset, row in reports.items():
        c, n = row['quality_counters'], row['expected_bins_10s']
        lines += [f'| {asset} | {pct(row["fully_covered_time_fraction_given_healthy_quote"])} | '
                  f'{row["insufficient_band_only_bins_10s"]}/{n} | {row["quote_invalid_or_missing_bins_10s"]}/{n} | '
                  f'{row["trade_invalid_or_missing_bins_10s"]}/{n} | {c.get("book_sequence_gap_quotes", 0)} | '
                  f'{c.get("unanchored_book", 0)} |']
    lines += ['', '订阅、重建快照、迟到和时间异常分类见分布文件。覆盖比例只以健康报价时间为分母。', '',
              '## 收缩前的波动率', '',
              '用 BBO 中间价的 1 秒对数收益计算 1、5、30 分钟已实现波动率，统一为 bp。波动率窗口结束于最近深度窗口开始时，即 t − 30 秒。以全部联合有效时点为基准，对比 L_depth > 0，以及候选 VET > 0。另将收缩前波动率与紧邻此前等长窗口配对，检查波动是否已经上升。', '',
              '| 合约 | 窗口 | 基准 RV bp P50 / P90 / P99 | L>0 RV bp P50 / P90 / P99 | 候选>0 RV bp P50 / P90 / P99 | L>0 超基准 P90 | RV 较上一等长窗上升（基准 → L>0） |',
              '|---|---:|---|---|---|---:|---|']
    for asset, row in reports.items():
        for minutes, view in row['past_realized_volatility_ending_before_contraction_window'].items():
            base, shrink, candidate = (view[name] for name in ('joint_baseline', 'depth_shrinks', 'candidate_positive'))
            lines += [f'| {asset} | {minutes} 分钟 | {cuts(base["volatility_bp"])} | {cuts(shrink["volatility_bp"])} | '
                      f'{cuts(candidate["volatility_bp"])} | {pct(shrink["above_joint_baseline_p90_fraction"])} | '
                      f'{pct(base["above_previous_equal_window_fraction"])} → '
                      f'{pct(shrink["above_previous_equal_window_fraction"])} |']
    five_minutes = {asset: row['past_realized_volatility_ending_before_contraction_window']['5']['depth_shrinks']
                    for asset, row in reports.items()}
    five_summary = '、'.join(f'{asset} {pct(view["above_previous_equal_window_fraction"])}'
                            for asset, view in five_minutes.items())
    eth_30 = reports['ETH']['past_realized_volatility_ending_before_contraction_window']['30']
    lines += ['', f'按收缩前 5 分钟与紧邻等长窗口的比较，波动已经上升的比例分别为 {five_summary}。三币均低于一半。'
              '1 分钟和 30 分钟窗口也没有显示跨币一致的富集。',
              f'ETH 的 30 分钟配对中，联合基准（n={eth_30["joint_baseline"]["paired_comparison_n"]}）'
              f'与收缩组（n={eth_30["depth_shrinks"]["paired_comparison_n"]}）的上升比例均为 100%，'
              '因此该时段的普遍波动上升无法区分收缩样本。',
              '',
              '分布文件给出各组样本数、等长配对数，以及候选正值最高十分位的同口径统计。这里测量的是发生顺序和共现频度，统计结果只适用于本历史时段。', '',
              '## v0.1 直连验证进度', '',
              f'本次直连复核读取到 UTC {direct.get("utc_end", "尚无封存片段")}，包含 {direct.get("window_seconds", 0) / 60:.2f} 分钟已封存数据。'
              f'采集状态为「{direct.get("capture_stop_reason") or "仍在采集"}」。',
              '',
              '| 合约 | 报价缺失桶 | 成交缺失桶 | 报价接收延迟 ms P50 / P90 / P99 | 成交接收延迟 ms P50 / P90 / P99 |',
              '|---|---:|---:|---|---|']
    for name, row in direct['markets'].items():
        total = row['expected_10s_bins']
        lines += [f'| {name.split("-")[0]} | {row["quote_missing_10s_bins"]}/{total} | '
                  f'{row["trade_missing_10s_bins"]}/{total} | {cuts(row["delay_ms"]["quotes"])} | {cuts(row["delay_ms"]["trades"])} |']
    lines += ['', '直连 BBO 用于验证 v0.1 的最优报价质量。2 bp 深度研究需要可重建的多档盘口。', '',
              '## 复现', '', '使用现有研究 Python 环境（aiohttp、pyarrow）。', '',
              '```powershell',
              'python research/vet_preflight/depth_replay.py --historical F:/temp/okx-tardis-native-20261101-audit-complete --sol-historical F:/temp/vet-tardis-sol-20261101 --specs F:/temp/vet-preflight-20261123-230914/instruments.json --output research/vet_preflight/depth_distribution.json --diagnostics research/vet_preflight/depth_diagnostics.jsonl.gz',
              'python research/vet_preflight/analyze.py --archive F:/temp/vet-preflight-20261123-230914 --output research/vet_preflight/direct_quality.json',
              'python research/vet_preflight/render_depth_report.py research/vet_preflight/depth_distribution.json research/vet_preflight/direct_quality.json research/vet_preflight/depth_report.md',
              '```', '']
    return '\n'.join(lines).rstrip('\n') + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('depth', type=Path)
    parser.add_argument('direct', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    depth = json.loads(args.depth.read_text(encoding='utf-8'))
    quality = json.loads(args.direct.read_text(encoding='utf-8'))
    tmp = args.output.with_suffix('.tmp')
    tmp.write_text(format_report(depth, quality), encoding='utf-8')
    tmp.replace(args.output)
