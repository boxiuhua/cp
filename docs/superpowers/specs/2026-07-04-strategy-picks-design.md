# 策略选号演示(仅教学)— 设计文档

日期:2026-07-04
项目:`lottery_stats`(Rust,Cargo 零外部依赖)
承接:网页页面(serve)+ 真实数据引擎。

## 背景与目标

页面新增「策略选号(仅演示)」:对当前彩种按冷号/热号/机选三种策略各生成**一注真实号码**,并解释"为什么这么选"。**关键框定**:这是诚实的教学演示,不是荐号——顶部醒目标注"中奖概率与任意号码完全相同",并用页面已有的预测回测数据作"打脸对照",证明这三注策略并不优于机选。与工具"彩票不可预测"的宗旨一致。

已确认决策:采用"C"——策略选号 + 诚实标注 + 复用回测对照。

## ① 数据模型与算法

`src/realdata.rs` 新增:
```rust
pub(crate) struct StrategyPick {
    pub strategy: String,       // "冷号" / "热号" / "机选"
    pub why: String,            // 为什么这么选(含"实际无效")
    pub ticket: Vec<Vec<u32>>,  // 每组件一段号码(如双色球:[[6红], [1蓝]])
}
pub(crate) fn strategy_picks(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng) -> Vec<StrategyPick>;
```
- **冷号**:各组件选**全历史**最少出现的号(池型 `pick_pool(counts,size,pick,false)`;数字型每位 `pick_digit(counts,false)`)。
- **热号**:同上,取最多(`want_hot=true`)。
- **机选**:池型 `rng.sample(size,pick)`;数字型每位 `rng.below(base)`。
- 复用已有 `pool_counts`/`digit_counts`/`pick_pool`/`pick_digit`(同模块私有/pub(crate),直接调用)。
- 冷/热平局按号码/数字升序(pick_pool/pick_digit 已保证),可复现。

`GameAnalysis` 增加字段 `pub picks: Vec<StrategyPick>`;`analyze_game` 里 `picks: strategy_picks(spec, draws, rng)`(在 prediction 之后调用,rng 状态延续,固定种子 → 可复现)。

**CLI 第 7 章不变**:picks 仅进结构体供网页用,`run_game_report`/`format_*` 不打印它 → 终端输出仍逐字不变。

## ② JSON 序列化

`src/server.rs` 的 `analysis_to_json` 增加 `picks` 数组:
```json
"picks":[{"strategy":"冷号","why":"...","ticket":[[7,8,9,10,11,12],[1]]}, ...]
```
`strategy`/`why` 经 `jesc` 转义;`ticket` 为嵌套整数数组。

## ③ 页面渲染

`index.html`(INDEX_HTML)在"分析结果"卡片**上方**新增「策略选号(仅演示)」卡片:
- 顶部醒目标注(warn 色):`⚠ 这三注中奖概率与任意号码完全相同(1/1772万),本演示只为展示"策略"长什么样。`
- 三注:每注显示策略名 + 号码(池型分段展示,如"红 07 08 09 10 11 12 | 蓝 01")+ "为什么:{why}"。
- 一句指向回测:`下方「预测打脸实验」显示这三策略历史平均命中数都≈理论值——并不比机选更优。`
- 切换彩种时随分析一起刷新(用同一份 `/api/analysis` 返回的 `picks`)。

## ④ 测试

- `strategy_picks`:构造"红球恒为 [1..6]"的数据 → 热号红段=`[1,2,3,4,5,6]`,冷号红段=`[7,8,9,10,11,12]`;ticket 含 2 组件(红+蓝);三注 strategy 名正确。
- `analysis_to_json`:输出含 `"picks":[`,含 `"strategy":"冷号"`。
- 页面:`index_page_served` 增断言 `策略选号`;渲染逻辑靠手动 serve 验证。
- 回归:第 7 章 CLI 输出仍逐字不变(picks 不打印)。

## 非目标(YAGNI)

- 不做"多注""复式";不做真正的荐号(明确拒绝);不改 CLI 输出;机选不追求密码学随机(演示用固定种子即可)。
