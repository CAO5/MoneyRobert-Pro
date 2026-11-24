"""Render the VET distribution into a read-only feasibility report."""

import argparse
import json
from pathlib import Path


def pct(value):
    return '—' if value is None else f'{value:.2%}'


def num(value):
    return '—' if value is None else f'{value:.4g}'


def cuts(values):
    return ' / '.join(num(values.get(name)) for name in ('p50', 'p90', 'p99'))


def rows(source):
    for market, row in source.get('markets', {}).items():
        yield market.removesuffix('-USDT-SWAP'), row


def report(data):
    sources = data['sources']
    historical = sources.get('historical', {})
    direct = sources.get('direct_all_sealed', sources.get('direct_first_two_hours', {}))
    primary = list(rows(historical)) if historical.get('markets') else list(rows(direct))
    counts = [(asset, row) for asset, row in primary if row['joint_valid']]
    lines = [
        '# VET v0.1 数据可辨识性预检',
        '',
        '研究对象为 OKX BTC、ETH、SOL USDT 永续。结论只涉及变量能否区分市场状态。',
        '',
        '## 数据与口径',
        '',
        '- BTC、ETH 复用已有的 Tardis 原始档案，SOL 从同一供应商补齐同一历史窗口。历史最优价从序列连续的 `books` 重建。档案保留供应商接收时间、OKX 事件时间和聚合成交的 `tradeId/count`。',
        '- 三币直连采集复用项目现有录制器，订阅 `bbo-tbt` 与 `trades`，附本机 UTC 和单调接收时钟。历史 `books` 与直连 `bbo-tbt` 的更新频率不同，两组分布各自报告。',
        '- 本次启动时 OKX 公共合约接口返回报价单位为 BTC 0.1、ETH 0.01、SOL 0.01 USDT。历史切片使用这组单位并检查报价是否落在对应网格。历史挂牌规格仍以当时交易所元数据为准。',
        '- 每 10 秒在交易所时间网格上计算。A 为最近 60 秒的聚合成交消息数，除以紧邻此前 10 分钟的每分钟均值，再取正向对数。L 为最近 30 秒与紧邻此前 10 分钟的时间加权中位价差之比的正向对数。价差用 tick 表示，同一 tick 规格下与绝对报价差的比值相同。VET 等于 A × L。',
        '- 报价段按有效持续毫秒加权，价差 bp 为报价差除以中间价乘 10,000，统计精度为 0.001 bp。健康连接内延续最近报价。断线、异常接收时钟、乱序、缺序列和无法确认的报价段记缺失。每个 10 秒桶只接收截至桶末后 5 秒的记录。迟到事件标记缺口，缺口跨越任一必要回看窗口则跳过相应变量。',
        '- 波动率为有效 BBO 中间价的 1 秒对数收益平方和开根号，以 bp 表示。比较同一市场、同一来源下的全部有效联合时点、VET 非零时点和非零 VET 的最高十分位（包含并列）。过去波动率的 P90 只用于报告诊断。有效时点高度重叠。',
        '',
        '接口来源 [OKX 聚合成交和最优报价](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-trades-channel) · [Tardis 原始数据接口](https://docs.tardis.dev/api/http-api-reference#data-feeds-exchange)',
        '',
    ]
    if len(counts) == 3 and all(row['vet_nonzero_n'] == 0 for _, row in counts):
        lines[4:4] = ['**初步结论是暂缓进入前向预测。** 三个合约的成交事件强度有变化，但所有有效联合时点的 VET 均为 0。瓶颈是最近 30 秒的时间加权中位价差未出现正向扩张。', '']
    for title, source in (('历史切片', historical), ('直连已封存数据', direct)):
        lines += [f'## {title}', '']
        if not source.get('window_seconds'):
            lines += ['当前没有可复核的完整封存窗口。', '']
            continue
        window = source['window_seconds']
        live_status = source.get('capture_stop_reason') or '仍在采集'
        lines += [f'UTC {source["utc_start"]} 至 {source["utc_end"]}，已分析 {window / 3600:.3f} 小时。接收时钟为 {source["receiver"]}。', '']
        if title.startswith('直连'):
            lines += [f'采集状态为「{live_status}」。本节只读取已封存分片。2 小时和 24 小时检查点以原始清单覆盖为准。', '']
        lines += [
            '| 合约 | 1 tick 时间占比 | Spread bp P50 / P90 / P99 | 60 秒成交事件数 P50 / P90 / P99 | A > 0 | L > 0 | VET > 0 | 联合有效点 | VET 非零 P50 / P90 / P99 |',
            '|---|---:|---|---|---:|---:|---:|---:|---|',
        ]
        for asset, row in rows(source):
            lines += [f'| {asset} | {pct(row["one_tick_fraction"])} | {cuts(row["spread_bp"])} | '
                      f'{cuts(row["trade_events_per_60s"])} | {pct(row["a_positive_fraction"])} | '
                      f'{pct(row["l_positive_fraction"])} | {pct(row["vet_positive_fraction"])} | '
                      f'{row["joint_valid"]} | {cuts(row["vet_nonzero"])} |']
        lines += ['', '| 合约 | 聚合消息条数 | `count` 合计 | `count > 1` 消息占比 | 平均每条底层成交数 | 报价缺失桶 | 成交缺失桶 | 报价延迟 ms P50 / P90 / P99 | 成交延迟 ms P50 / P90 / P99 |',
                  '|---|---:|---:|---:|---:|---:|---:|---|---|']
        for asset, row in rows(source):
            total = row['expected_10s_bins']
            lines += [f'| {asset} | {row["trade_messages"]:,} | {row["trade_count_sum"]:,} | '
                      f'{pct(row["aggregated_message_fraction"])} | {num(row["mean_fills_per_message"])} | '
                      f'{row["quote_missing_10s_bins"]}/{total} | {row["trade_missing_10s_bins"]}/{total} | '
                      f'{cuts(row["delay_ms"]["quotes"])} | {cuts(row["delay_ms"]["trades"])} |']
        lines += ['', '完整的 tick 持续时间分布、延迟极值与断线及缺序列计数见 `distribution.json`。', '',
                  '| 合约 | 过去波动率窗口 | 全部联合点 RV bp P50 / P90 / P99 | VET 非零 RV bp P50 / P90 / P99 | VET 高位 RV bp P50 / P90 / P99 | 高位组 n | 高位组超过基准 P90 |',
                  '|---|---:|---|---|---|---:|---:|']
        for asset, row in rows(source):
            for minutes, vol in row['past_realized_volatility'].items():
                positive, high = vol['vet_positive'], vol['vet_top_nonzero_decile']
                lines += [f'| {asset} | {minutes} 分钟 | {cuts(vol["baseline_percentiles_bp"])} | '
                          f'{cuts(positive["percentiles_bp"])} | {cuts(high["percentiles_bp"])} | '
                          f'{high["n"]} | {pct(high["above_baseline_p90_fraction"])} |']
        lines.append('')
    lines += ['## 六个可行性问题', '']
    if not counts:
        lines += ['尚无联合有效窗口。六项结论等待首批覆盖至少完整回看窗口的有效数据。']
        return '\n'.join(lines).rstrip('\n') + '\n'
    names = '、'.join(asset for asset, _ in counts)
    scope = '历史' if historical.get('markets') else '直连'
    lines += [f'以下判断基于{scope}切片中有联合有效点的 {names}。', '']
    lines += [
        '1. **成交事件强度是否有变化。** ' + '。'.join(
            f'{asset} 的 A 为正占 {pct(row["a_positive_fraction"])}，60 秒事件数 P50/P90/P99 为 {cuts(row["trade_events_per_60s"])}'
            for asset, row in counts) + '。',
        '2. **最优价差是否有变化。** ' + '。'.join(
            f'{asset} 的 1 tick 占 {pct(row["one_tick_fraction"])}，L 为正占 {pct(row["l_positive_fraction"])}'
            for asset, row in counts) + '。应分别看瞬时扩价和 30 秒中位数是否扩张。',
        '3. **A 与 L 是否经常共同升高。** ' + '。'.join(
            f'{asset} 同升占 {pct(row["vet_positive_fraction"])}，同一样本上边际比例乘积为 '
            f'{pct(row["joint_a_positive_fraction"] * row["joint_l_positive_fraction"])}'
            for asset, row in counts) + '。边际乘积仅提供共现频度参照。',
        '4. **VET 是否稀疏。** ' + '。'.join(
            f'{asset} 非零 {row["vet_nonzero_n"]}/{row["joint_valid"]}，零值占 {pct(1 - row["vet_positive_fraction"])}'
            for asset, row in counts) + '。这些 10 秒观测共享回看窗口。',
    ]
    vol_details = []
    for asset, row in counts:
        horizons = row['past_realized_volatility']
        vol_details.append(asset + ' 在 1/5/30 分钟的高位组超过既往波动率基准 P90 的比例分别为 ' +
                           ' / '.join(pct(horizons[str(m)]['vet_top_nonzero_decile']['above_baseline_p90_fraction'])
                                      for m in (1, 5, 30)))
    if all(row['vet_nonzero_n'] == 0 for _, row in counts):
        lines += ['5. **是否主要出现在既往剧烈波动后。** 无法判断。该切片没有 VET 非零点，高位组为空。上表给出过去 1、5、30 分钟的既往波动率基准分布。']
    else:
        lines += ['5. **是否主要出现在既往剧烈波动后。** ' + '。'.join(vol_details)
                  + '。各组样本量见上表，少量高位点或缺失窗口只支持描述性判断。']
    coverage_ready = direct.get('capture_stop_reason') == 'deadline' and direct.get('window_seconds', 0) >= 86_380
    if coverage_ready:
        decision = '保留乘法公式作为候选，前向实验前按直连样本的共现频度和跨时段持续性决定是否立项。'
    else:
        decision = '保留乘法公式供后续复核。当前阶段暂缓进入前向预测，先完成三币同口径直连全天的分布复核。'
    lines += ['6. **是否进入前向预测。** ' + decision, '', '## 复跑', '',
              '使用与现有历史回放相同的 Python 环境，包含 `aiohttp` 和 `pyarrow`。分析仅读取已封存的直连分片。', '',
              '```powershell',
              'python research/vet_preflight/analyze.py --archive F:/temp/vet-preflight-20261123-230914 --historical F:/temp/okx-tardis-native-20261101-audit-complete --sol-historical F:/temp/vet-tardis-sol-20261101 --output research/vet_preflight/distribution.json',
              'python research/vet_preflight/render_report.py research/vet_preflight/distribution.json research/vet_preflight/report.md',
              '```', '']
    return '\n'.join(lines).rstrip('\n') + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('distribution', type=Path)
    parser.add_argument('report', type=Path)
    args = parser.parse_args()
    result = report(json.loads(args.distribution.read_text(encoding='utf-8')))
    temporary = args.report.with_suffix('.tmp')
    temporary.write_text(result, encoding='utf-8')
    temporary.replace(args.report)
