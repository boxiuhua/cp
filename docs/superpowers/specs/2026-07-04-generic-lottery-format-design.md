# 通用多彩种真实数据引擎(B 阶段)— 设计文档

日期:2026-07-04
项目:`lottery_stats`(彩票随机性验证工具,Rust,零外部依赖)
承接:A 阶段(双色球真实数据接入,`docs/superpowers/specs/2026-07-04-ssq-realdata-design.md`)

## 背景与目标

A 阶段为双色球实现了"读文件 → 真实数据检验 → 预测打脸"的完整链路,但格式与代码是双色球专用的。B 阶段把它抽象成**覆盖全部 8 种彩票的通用引擎**:一套 `GameSpec` 描述开奖抽取结构,一套分析引擎在其上泛化;双色球重构为其中一个配置,只保留一条代码路径。

零外部依赖 / 离线可复现 / 固定种子的原则不变。

决策前提(已确认):
- 每种彩票一个数据文件(`data/<key>.csv`)。
- 数字型彩票也做全套 5 项分析,预测实验改为"逐位猜数字、统计平均猜中位数"。
- 双色球重构进通用引擎(取代 `src/ssq.rs`),只有一条分析代码路径。
- 建模采用"组件枚举"(Pool / Digits)。

## ① 核心模型

```rust
// src/game_spec.rs
pub enum Component {
    Pool   { label: &'static str, size: u32, pick: u32 },  // 无放回抽 pick 个,值 ∈ [1,size],互异
    Digits { label: &'static str, bases: Vec<u32> },       // 逐位独立,第 i 位 ∈ [0,bases[i])
}

pub struct GameSpec {
    pub key: &'static str,        // "ssq" 等
    pub name: &'static str,       // "双色球"
    pub file: &'static str,       // "data/ssq.csv"
    pub components: Vec<Component>,
}

pub fn real_data_games() -> Vec<GameSpec>;
```

8 种彩票配置:

| key | 名称 | 文件 | 组件 | 每行号码数 |
|---|---|---|---|---|
| ssq | 双色球 | data/ssq.csv | Pool 红球{33,6} + Pool 蓝球{16,1} | 7 |
| dlt | 超级大乐透 | data/dlt.csv | Pool 前区{35,5} + Pool 后区{12,2} | 7 |
| d3 | 福彩3D | data/d3.csv | Digits{[10,10,10]} | 3 |
| pl3 | 排列3 | data/pl3.csv | Digits{[10,10,10]} | 3 |
| pl5 | 排列5 | data/pl5.csv | Digits{[10,10,10,10,10]} | 5 |
| qxc | 7星彩 | data/qxc.csv | Digits{[10,10,10,10,10,10,15]} | 7 |
| qlc | 7乐彩 | data/qlc.csv | Pool 号码{30,7} | 7 |
| kl8 | 快乐8 | data/kl8.csv | Pool 号码{80,20} | 20 |

**关键决定**:`GameSpec` 只描述开奖抽取结构(供随机性分析),**不合并**现有第 1–2 章的 `Game`(头奖组合数/返奖率)。原因:头奖概率取决于投注玩法(如快乐8"选10中10"的中奖概率并非简单 C(80,20)),与开奖结构是不同关注点。两者保持独立。

## ② 文件格式与解析

每种彩票一个文件,格式统一为 `期号,日期,号码...`,号码个数 = 各组件所需数量之和。示例:

```
# data/dlt.csv  超级大乐透  期号,日期,前1..前5,后1,后2
2024001,2024-01-02,3,11,18,27,34,4,10

# data/d3.csv  福彩3D  期号,日期,百,十,个
2024001,2024-01-02,3,8,1
```

通用解析器(由 `GameSpec` 驱动):

```rust
pub struct DrawRecord {
    pub issue: String,
    pub date: String,
    pub components: Vec<Vec<u32>>,   // 按 spec 组件顺序,每组件一段号码
}
pub struct SkipInfo { pub line: usize, pub reason: String }

pub fn parse_lines(spec: &GameSpec, content: &str) -> (Vec<DrawRecord>, Vec<SkipInfo>);
pub fn load_game(spec: &GameSpec) -> Result<(Vec<DrawRecord>, Vec<SkipInfo>), String>;
```

- 期望字段数 = `2 + Σ(Pool.pick 或 Digits.bases.len())`,由 spec 计算。
- 逐组件校验:
  - `Pool{size,pick}`:该段恰好 pick 个,均 ∈ [1,size],互异;存储时排序。
  - `Digits{bases}`:该段恰好 bases.len() 个,第 i 个 ∈ [0,bases[i]);允许重复,不排序(保留位置)。
- `#` 注释行、空行忽略;首字段非纯数字视为表头跳过;坏行记 `SkipInfo{行号,原因}`,不中断解析。
- `load_game` 文件缺失/不可读返回 `Err`(上层转成"跳过该彩种")。

## ③ 通用分析引擎

把每个组件展开为若干"类别频次轨道",四项分析在轨道上跑,复用 A 阶段的 `crate::chi2_from_freq` / `runs_z` / `chi2_pvalue` / `normal_two_sided_p` / `format_int`。

**① 均匀性(卡方)**,按组件分别检验:
- `Pool{size,pick}`:统计 1..=size 各号频次,`expected = draws*pick/size`,`df = size-1`,算 χ²/p,列最冷/最热号(带理论标准差 √expected 参照)。
- `Digits{bases}`:逐位检验第 i 位数字在 0..bases[i] 上是否均匀,`expected = draws/bases[i]`,`df = bases[i]-1`,各位分别打印。

**② 赌徒谬误**,每个组件自动挑一个固定代表 target,验证条件概率 ≈ 无条件:
- Pool:号 `min(7, size)`,predicate = 该期号池含 target;无条件 P = pick/size。
- Digits:第 0 位、数字 `min(7, base-1)`,predicate = 该位 == target;无条件 P = 1/base。

**③ 游程检验**:用②的同一 target predicate 生成逐期 0/1 序列,过 `runs_z` → 双尾 p,判独立性。

target 固定挑选(非随机),保证可复现;每组件只测一个 target,输出量可控。

统一 predicate 抽象:
```rust
pub enum TargetKind { PoolBall(u32), DigitAt { pos: usize, digit: u32 } }
fn target_hit(rec: &DrawRecord, comp_idx: usize, target: &TargetKind) -> bool;
```

## ④ 预测"打脸"实验(泛化)

按时间序遍历,从第 `window`(默认 30)期起,用前 window 期走势预测下一期,冷/热/随机三策略同台;**按组件分别回测并报告**。

- **Pool{size,pick}**:预测 pick 个不同号(冷=窗口最少见的 pick 个,热=最多见,随机=随机 pick 个;平局按号码升序)。命中数 = 预测 ∩ 实际,∈ 0..pick。理论期望 = `pick*pick/size`。
- **Digits{bases}**:逐位各猜一个数字(冷=该位窗口最少见,热=最多见,随机=均匀)。命中数 = 猜中位数,∈ 0..位数。理论期望 = `Σ 1/bases[i]`。

输出每组件一段:冷/热/随机平均命中数 + 理论期望 + "是否在噪声内贴近理论"的简单判断。

```rust
pub struct CompPred { pub label: String, pub cold: f64, pub hot: f64, pub random: f64, pub expected: f64 }
pub fn prediction_stats(spec: &GameSpec, draws: &[DrawRecord], window: usize, rng: &mut crate::Rng)
    -> (usize, Vec<CompPred>);   // 返回 (回测期数 n, 各组件结果);n==0 表示数据不足(draws <= window)
```

## ⑤ 模块结构、重构与测试

**模块划分:**
- `src/game_spec.rs`(新):`Component`、`GameSpec`、`real_data_games()`。
- `src/realdata.rs`(新,取代 `ssq.rs`):`DrawRecord`、`SkipInfo`、`TargetKind`、`parse_lines`、`load_game`、`red-`/digit- 频次辅助、四项分析、`prediction_stats`、`print_*`、`run_game_report(spec, draws, rng)`、`run_all_real_data(rng)`。
- `src/ssq.rs`:删除。
- `src/main.rs`:第 1–6 章与纯数学/`Rng` 不变;`mod ssq;` 换成 `mod game_spec; mod realdata;`;第 7 章改为调用 `realdata::run_all_real_data(&mut rng)`。

**`run_all_real_data`**:遍历 `real_data_games()`,对每种彩票尝试 `load_game`;文件缺失打印一行"未找到 data/<key>.csv,跳过";加载成功且有效期数 ≥ 2 则打印加载摘要 + 数据覆盖行 + 四项分析 + 预测实验,否则打印"数据不足,跳过"。

**重构影响:** 第 7 章输出从"仅双色球"变为"遍历 8 种彩票";双色球走通用引擎,数学与 A 阶段同源、结论一致。

**数据文件:** 保留 `data/ssq.csv`(占位);新增 `data/d3.csv`(几行占位,让数字型链路在 demo 中真跑);其余 6 种默认无文件 → 优雅跳过。`data/README.md` 更新为覆盖全部 8 种的格式说明。

**测试(`#[cfg(test)]`):**
- `game_spec`:期望字段数计算(kl8=22、d3=5、ssq=9)。
- 通用解析:Pool 段(互异/越界/个数),Digits 段(越界/个数/允许重复),表头/注释/空行跳过,坏行记录。
- 均匀性:Pool 完全均匀频次 → χ²≈0;Digits 逐位均匀 → χ²≈0。
- 赌徒谬误/游程:池型与数字型 target predicate 各验一次。
- 预测:①Pool 造"固定 6 号必出"→ 热满命中、冷=0;②Digits 造"某位固定数字"→ 热该位必中。
- 回归:一小组双色球数据过通用引擎,核对红球频次/χ² 与手算一致。

**非目标(YAGNI):** 不合并第 1–2 章 `Game`;不做跨彩种对比汇总;不加命令行选彩种(默认全跑);仍为终端文本输出。
