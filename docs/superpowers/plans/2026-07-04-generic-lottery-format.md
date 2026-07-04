# 通用多彩种真实数据引擎(B 阶段)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把双色球专用的真实数据链路抽象成覆盖全部 8 种彩票的通用引擎:一套 `GameSpec`(Pool/Digits 组件)描述开奖结构,一套分析引擎在其上泛化,双色球成为其中一个配置。

**Architecture:** 新增 `src/game_spec.rs`(结构描述)与 `src/realdata.rs`(通用解析 + 分析 + 预测 + 编排),删除双色球专用的 `src/ssq.rs`。分析复用 A 阶段抽出的纯数学函数(`chi2_from_freq`/`runs_z`/`chi2_pvalue`/`normal_two_sided_p`/`format_int`)。main 第 1–6 章不变,第 7 章改为遍历 8 种彩票。

**Tech Stack:** Rust 2021,零外部依赖(仅 std,含 std::fs),内置 `#[cfg(test)]` + `cargo test`。

## Global Constraints

- **零外部依赖**:`Cargo.toml` 的 `[dependencies]` 保持为空,只用 std。
- **离线可复现**:所有随机用固定种子 `Rng`(main 中 `Rng::new(20260703)`);策略平局按号码/数字升序取,保证可复现。
- **Pool 组件规则**:抽 `pick` 个,值 ∈ [1,size],互异,存储排序。**Digits 组件规则**:第 i 位 ∈ [0,bases[i]),允许重复,不排序。
- **命令**:`cargo test`、`cargo run --release`。
- **Git**:仓库已初始化(remote origin 存在)。每个任务末尾按步骤 commit。
- **复用而非重写**:卡方/游程/p 值一律调用 `crate::` 下 A 阶段的 `pub(crate)` 函数,不得重新实现。
- **8 种彩票配置(逐字)**:ssq 双色球 data/ssq.csv [Pool 红球{33,6}+Pool 蓝球{16,1}];dlt 超级大乐透 data/dlt.csv [Pool 前区{35,5}+Pool 后区{12,2}];d3 福彩3D data/d3.csv [Digits 直选 [10,10,10]];pl3 排列3 data/pl3.csv [Digits 直选 [10,10,10]];pl5 排列5 data/pl5.csv [Digits 直选 [10,10,10,10,10]];qxc 7星彩 data/qxc.csv [Digits 直选 [10,10,10,10,10,10,15]];qlc 7乐彩 data/qlc.csv [Pool 号码{30,7}];kl8 快乐8 data/kl8.csv [Pool 号码{80,20}]。

---

### Task 1: `src/game_spec.rs` — 结构模型与 8 种彩票配置

**Files:**
- Create: `src/game_spec.rs`
- Modify: `src/main.rs`(顶部加 `mod game_spec;`)

**Interfaces:**
- Produces:
  - `pub(crate) enum Component { Pool { label: &'static str, size: u32, pick: u32 }, Digits { label: &'static str, bases: Vec<u32> } }`
  - `impl Component { pub fn width(&self) -> usize }`
  - `pub(crate) struct GameSpec { pub key, pub name, pub file: &'static str, pub components: Vec<Component> }`
  - `impl GameSpec { pub fn field_count(&self) -> usize }`
  - `pub(crate) fn real_data_games() -> Vec<GameSpec>`

- [ ] **Step 1: 建文件并写失败测试**

Create `src/game_spec.rs`:

```rust
// 彩票开奖结构描述:每种彩票 = 若干组件。仅用于真实数据的随机性分析,
// 与第 1-2 章的头奖概率/返奖率 Game 是不同关注点,故独立。

pub(crate) enum Component {
    // 无放回抽 pick 个,值 ∈ [1,size],互异
    Pool { label: &'static str, size: u32, pick: u32 },
    // 逐位独立,第 i 位 ∈ [0,bases[i]),允许重复
    Digits { label: &'static str, bases: Vec<u32> },
}

impl Component {
    // 该组件在一行数据里占用多少个号码字段
    pub fn width(&self) -> usize {
        match self {
            Component::Pool { pick, .. } => *pick as usize,
            Component::Digits { bases, .. } => bases.len(),
        }
    }
}

pub(crate) struct GameSpec {
    pub key: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub components: Vec<Component>,
}

impl GameSpec {
    // 一行 CSV 的期望字段数 = 期号 + 日期 + 全部号码
    pub fn field_count(&self) -> usize {
        2 + self.components.iter().map(|c| c.width()).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_counts_are_correct() {
        let games = real_data_games();
        let by = |k: &str| games.iter().find(|g| g.key == k).unwrap().field_count();
        assert_eq!(by("ssq"), 9); // 期号+日期+6红+1蓝
        assert_eq!(by("dlt"), 9); // +5+2
        assert_eq!(by("d3"), 5);  // +3
        assert_eq!(by("pl5"), 7); // +5
        assert_eq!(by("qxc"), 9); // +7
        assert_eq!(by("kl8"), 22); // +20
    }

    #[test]
    fn all_eight_games_present() {
        assert_eq!(real_data_games().len(), 8);
    }
}
```

Modify `src/main.rs`: add near the top (before `mod ssq;` if present, or before `struct Rng`):

```rust
mod game_spec;
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function real_data_games`.

- [ ] **Step 3: 实现 real_data_games**（加到 `src/game_spec.rs`,`impl GameSpec` 之后、`#[cfg(test)]` 之前)

```rust
pub(crate) fn real_data_games() -> Vec<GameSpec> {
    vec![
        GameSpec {
            key: "ssq", name: "双色球", file: "data/ssq.csv",
            components: vec![
                Component::Pool { label: "红球", size: 33, pick: 6 },
                Component::Pool { label: "蓝球", size: 16, pick: 1 },
            ],
        },
        GameSpec {
            key: "dlt", name: "超级大乐透", file: "data/dlt.csv",
            components: vec![
                Component::Pool { label: "前区", size: 35, pick: 5 },
                Component::Pool { label: "后区", size: 12, pick: 2 },
            ],
        },
        GameSpec {
            key: "d3", name: "福彩3D", file: "data/d3.csv",
            components: vec![Component::Digits { label: "直选", bases: vec![10, 10, 10] }],
        },
        GameSpec {
            key: "pl3", name: "排列3", file: "data/pl3.csv",
            components: vec![Component::Digits { label: "直选", bases: vec![10, 10, 10] }],
        },
        GameSpec {
            key: "pl5", name: "排列5", file: "data/pl5.csv",
            components: vec![Component::Digits { label: "直选", bases: vec![10, 10, 10, 10, 10] }],
        },
        GameSpec {
            key: "qxc", name: "7星彩", file: "data/qxc.csv",
            components: vec![Component::Digits { label: "直选", bases: vec![10, 10, 10, 10, 10, 10, 15] }],
        },
        GameSpec {
            key: "qlc", name: "7乐彩", file: "data/qlc.csv",
            components: vec![Component::Pool { label: "号码", size: 30, pick: 7 }],
        },
        GameSpec {
            key: "kl8", name: "快乐8", file: "data/kl8.csv",
            components: vec![Component::Pool { label: "号码", size: 80, pick: 20 }],
        },
    ]
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: `field_counts_are_correct` 与 `all_eight_games_present` PASS(其余既有测试仍 PASS)。dead-code 警告可接受。

- [ ] **Step 5: Commit**

```bash
git add src/game_spec.rs src/main.rs
git commit -m "feat: add GameSpec model with 8 lottery configs"
```

---

### Task 2: `src/realdata.rs` — 通用解析器

**Files:**
- Create: `src/realdata.rs`
- Modify: `src/main.rs`(加 `mod realdata;`)

**Interfaces:**
- Consumes: `crate::game_spec::{Component, GameSpec}`.
- Produces:
  - `pub(crate) struct DrawRecord { pub issue: String, pub date: String, pub components: Vec<Vec<u32>> }`
  - `pub(crate) struct SkipInfo { pub line: usize, pub reason: String }`
  - `pub(crate) fn parse_record(spec: &GameSpec, fields: &[&str]) -> Result<DrawRecord, String>`
  - `pub(crate) fn parse_lines(spec: &GameSpec, content: &str) -> (Vec<DrawRecord>, Vec<SkipInfo>)`
  - `pub(crate) fn load_game(spec: &GameSpec) -> Result<(Vec<DrawRecord>, Vec<SkipInfo>), String>`
  - test helper `fn rec(components: Vec<Vec<u32>>) -> DrawRecord` (in `mod tests`, reused by later tasks)

- [ ] **Step 1: 建文件并写失败测试**

Create `src/realdata.rs`:

```rust
// 通用真实数据引擎:由 GameSpec 驱动的解析与分析。
use crate::game_spec::{Component, GameSpec};

pub(crate) struct DrawRecord {
    pub issue: String,
    pub date: String,
    pub components: Vec<Vec<u32>>, // 按 spec 组件顺序,每组件一段号码
}

pub(crate) struct SkipInfo {
    pub line: usize,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_spec::real_data_games;

    // 测试用:直接构造一条记录
    fn rec(components: Vec<Vec<u32>>) -> DrawRecord {
        DrawRecord { issue: "x".into(), date: "d".into(), components }
    }

    fn ssq() -> GameSpec {
        real_data_games().into_iter().find(|g| g.key == "ssq").unwrap()
    }
    fn d3() -> GameSpec {
        real_data_games().into_iter().find(|g| g.key == "d3").unwrap()
    }

    #[test]
    fn parses_pool_game_row() {
        let d = parse_record(&ssq(), &["2024001", "2024-01-02", "7", "1", "33", "15", "22", "28", "9"]).unwrap();
        assert_eq!(d.components[0], vec![1, 7, 15, 22, 28, 33]); // 红球排序
        assert_eq!(d.components[1], vec![9]); // 蓝球
    }

    #[test]
    fn parses_digit_game_row_keeps_order_and_repeats() {
        let d = parse_record(&d3(), &["2024001", "2024-01-02", "7", "7", "2"]).unwrap();
        assert_eq!(d.components[0], vec![7, 7, 2]); // 允许重复,不排序
    }

    #[test]
    fn rejects_wrong_field_count() {
        assert!(parse_record(&ssq(), &["2024001", "2024-01-02", "1", "2", "3"]).is_err());
    }

    #[test]
    fn rejects_pool_out_of_range_and_dupes() {
        assert!(parse_record(&ssq(), &["i", "d", "0", "7", "15", "22", "28", "33", "9"]).is_err()); // 红 0
        assert!(parse_record(&ssq(), &["i", "d", "7", "7", "15", "22", "28", "33", "9"]).is_err()); // 红重复
        assert!(parse_record(&ssq(), &["i", "d", "1", "7", "15", "22", "28", "33", "17"]).is_err()); // 蓝 17
    }

    #[test]
    fn rejects_digit_out_of_range() {
        assert!(parse_record(&d3(), &["i", "d", "7", "7", "10"]).is_err()); // 位值 10 >= base 10
    }

    #[test]
    fn parse_lines_skips_comment_blank_header_and_bad() {
        let content = "\
# 注释
期号,日期,红,红,红,红,红,红,蓝

2024001,2024-01-02,1,7,15,22,28,33,9
2024002,2024-01-04,3,5,11,19,26,31,99
2024003,2024-01-06,2,4,6,8,10,12,3
";
        let (draws, skips) = parse_lines(&ssq(), content);
        assert_eq!(draws.len(), 2);
        assert_eq!(skips.len(), 1);
    }
}
```

Modify `src/main.rs`: add `mod realdata;` next to `mod game_spec;`.

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function parse_record` / `parse_lines`.

- [ ] **Step 3: 实现解析器**（加到 `src/realdata.rs`,`SkipInfo` 之后、`#[cfg(test)]` 之前)

```rust
// 按 spec 解析一行的全部字段为一条记录。任何不合规返回 Err(原因)。
pub(crate) fn parse_record(spec: &GameSpec, fields: &[&str]) -> Result<DrawRecord, String> {
    let want = spec.field_count();
    if fields.len() != want {
        return Err(format!("字段数应为 {},实际 {}", want, fields.len()));
    }
    let mut idx = 2usize;
    let mut components: Vec<Vec<u32>> = Vec::with_capacity(spec.components.len());
    for comp in &spec.components {
        let w = comp.width();
        let mut seg: Vec<u32> = Vec::with_capacity(w);
        for k in 0..w {
            let raw = fields[idx + k].trim();
            let v: u32 = raw.parse().map_err(|_| format!("'{}' 非法整数", raw))?;
            seg.push(v);
        }
        match comp {
            Component::Pool { size, .. } => {
                for &v in &seg {
                    if v < 1 || v > *size {
                        return Err(format!("号 {} 超出 1-{}", v, size));
                    }
                }
                for i in 0..seg.len() {
                    for j in (i + 1)..seg.len() {
                        if seg[i] == seg[j] {
                            return Err(format!("号重复:{}", seg[i]));
                        }
                    }
                }
                seg.sort_unstable();
            }
            Component::Digits { bases, .. } => {
                for (pos, &v) in seg.iter().enumerate() {
                    if v >= bases[pos] {
                        return Err(format!("第 {} 位值 {} 超出 0-{}", pos + 1, v, bases[pos] - 1));
                    }
                }
                // 不排序,保留位置
            }
        }
        components.push(seg);
        idx += w;
    }
    Ok(DrawRecord {
        issue: fields[0].trim().to_string(),
        date: fields[1].trim().to_string(),
        components,
    })
}

// 逐行解析:跳过空行、# 注释、表头(首字段非纯数字)。坏行记 SkipInfo。
pub(crate) fn parse_lines(spec: &GameSpec, content: &str) -> (Vec<DrawRecord>, Vec<SkipInfo>) {
    let mut draws = Vec::new();
    let mut skips = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if fields[0].is_empty() || !fields[0].chars().all(|c| c.is_ascii_digit()) {
            continue; // 表头/非数据行
        }
        match parse_record(spec, &fields) {
            Ok(d) => draws.push(d),
            Err(reason) => skips.push(SkipInfo { line: line_no, reason }),
        }
    }
    (draws, skips)
}

// 从 spec.file 加载。文件不存在/不可读返回 Err。
pub(crate) fn load_game(spec: &GameSpec) -> Result<(Vec<DrawRecord>, Vec<SkipInfo>), String> {
    let content = std::fs::read_to_string(spec.file)
        .map_err(|e| format!("无法读取 '{}': {}", spec.file, e))?;
    Ok(parse_lines(spec, &content))
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: 6 个新 realdata 解析测试 + 既有测试全部 PASS。dead-code 警告可接受。

- [ ] **Step 5: Commit**

```bash
git add src/realdata.rs src/main.rs
git commit -m "feat: generic spec-driven CSV parser in realdata module"
```

---

### Task 3: 通用均匀性分析

**Files:**
- Modify: `src/realdata.rs`

**Interfaces:**
- Consumes: `crate::chi2_from_freq`, `crate::chi2_pvalue`, `crate::format_int`.
- Produces:
  - `pub(crate) fn pool_counts(draws: &[DrawRecord], comp_idx: usize, size: u32) -> Vec<u64>` (长度 size+1,下标 1..=size)
  - `pub(crate) fn digit_counts(draws: &[DrawRecord], comp_idx: usize, pos: usize, base: u32) -> Vec<u64>` (长度 base,下标 0..base)
  - `pub(crate) fn analyze_uniformity(spec: &GameSpec, draws: &[DrawRecord])`

- [ ] **Step 1: 写失败测试**（加到 `src/realdata.rs` 的 `mod tests`)

```rust
    #[test]
    fn pool_counts_tally() {
        let draws = vec![rec(vec![vec![1, 2, 3, 4, 5, 6], vec![9]]),
                         rec(vec![vec![1, 2, 3, 4, 5, 7], vec![3]])];
        let c = pool_counts(&draws, 0, 33);
        assert_eq!(c[1], 2);
        assert_eq!(c[6], 1);
        assert_eq!(c[7], 1);
        assert_eq!(c[8], 0);
    }

    #[test]
    fn digit_counts_tally() {
        let draws = vec![rec(vec![vec![7, 7, 2]]), rec(vec![vec![7, 0, 2]])];
        let pos0 = digit_counts(&draws, 0, 0, 10);
        assert_eq!(pos0[7], 2);
        assert_eq!(pos0[0], 0);
        let pos1 = digit_counts(&draws, 0, 1, 10);
        assert_eq!(pos1[7], 1);
        assert_eq!(pos1[0], 1);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function pool_counts`.

- [ ] **Step 3: 实现**（加到 `src/realdata.rs`,`load_game` 之后)

```rust
// 某 Pool 组件各号出现次数,下标 1..=size。
pub(crate) fn pool_counts(draws: &[DrawRecord], comp_idx: usize, size: u32) -> Vec<u64> {
    let mut c = vec![0u64; size as usize + 1];
    for d in draws {
        for &b in &d.components[comp_idx] {
            c[b as usize] += 1;
        }
    }
    c
}

// 某 Digits 组件第 pos 位各数字出现次数,下标 0..base。
pub(crate) fn digit_counts(draws: &[DrawRecord], comp_idx: usize, pos: usize, base: u32) -> Vec<u64> {
    let mut c = vec![0u64; base as usize];
    for d in draws {
        c[d.components[comp_idx][pos] as usize] += 1;
    }
    c
}

// [真实] 均匀性:Pool 按号池、Digits 逐位分别做卡方。
pub(crate) fn analyze_uniformity(spec: &GameSpec, draws: &[DrawRecord]) {
    println!("\n-- [真实] 卡方均匀性检验 --");
    for (ci, comp) in spec.components.iter().enumerate() {
        match comp {
            Component::Pool { label, size, pick } => {
                let counts = pool_counts(draws, ci, *size);
                let expected = draws.len() as f64 * *pick as f64 / *size as f64;
                let chi2 = crate::chi2_from_freq(&counts[1..=*size as usize], expected);
                let df = (*size - 1) as f64;
                let p = crate::chi2_pvalue(chi2, df);
                let sd = expected.sqrt();
                let mut idx: Vec<usize> = (1..=*size as usize).collect();
                idx.sort_by_key(|&i| counts[i]);
                let (cold, hot) = (idx[0], idx[*size as usize - 1]);
                println!(
                    "[{} {}/{}] 期望频次 {:.1}  χ²={:.2} df={} p={:.4}  =>{}",
                    label, pick, size, expected, chi2, df as u32, p,
                    if p > 0.05 { "均匀" } else { "本样本偏离(小样本功效低)" }
                );
                println!(
                    "  最冷 {:02}({}次) vs 最热 {:02}({}次),差 ≈{:.1}σ,属随机涨落。",
                    cold, counts[cold], hot, counts[hot],
                    (counts[hot] - counts[cold]) as f64 / sd.max(1e-9)
                );
            }
            Component::Digits { label, bases } => {
                for (pos, &base) in bases.iter().enumerate() {
                    let counts = digit_counts(draws, ci, pos, base);
                    let expected = draws.len() as f64 / base as f64;
                    let chi2 = crate::chi2_from_freq(&counts, expected);
                    let df = (base - 1) as f64;
                    let p = crate::chi2_pvalue(chi2, df);
                    println!(
                        "[{} 第{}位 0-{}] 期望频次 {:.1}  χ²={:.2} df={} p={:.4}  =>{}",
                        label, pos + 1, base - 1, expected, chi2, df as u32, p,
                        if p > 0.05 { "均匀" } else { "本样本偏离(小样本功效低)" }
                    );
                }
            }
        }
    }
    let _ = crate::format_int; // 保持与其他分析一致的可用性(供覆盖行使用)
}
```

（注:`let _ = crate::format_int;` 只是占位说明 format_int 可用;若产生 clippy/编译问题,删掉该行即可,它非必需。实现者可直接省略此行。)

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: `pool_counts_tally`、`digit_counts_tally` PASS,全部测试 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/realdata.rs
git commit -m "feat: generic chi-square uniformity for pool and digit components"
```

---

### Task 4: 赌徒谬误 + 游程(泛化)

**Files:**
- Modify: `src/realdata.rs`

**Interfaces:**
- Consumes: `crate::runs_z`, `crate::normal_two_sided_p`.
- Produces:
  - `pub(crate) enum TargetKind { PoolBall(u32), DigitAt { pos: usize, digit: u32 } }`
  - `pub(crate) fn default_target(comp: &Component) -> TargetKind`
  - `pub(crate) fn target_hit(rec: &DrawRecord, comp_idx: usize, target: &TargetKind) -> bool`
  - `pub(crate) fn analyze_gamblers_fallacy(spec: &GameSpec, draws: &[DrawRecord])`
  - `pub(crate) fn analyze_runs(spec: &GameSpec, draws: &[DrawRecord])`

- [ ] **Step 1: 写失败测试**（加到 `mod tests`)

```rust
    #[test]
    fn target_hit_pool_and_digit() {
        let pool_rec = rec(vec![vec![1, 7, 15, 22, 28, 33], vec![9]]);
        assert!(target_hit(&pool_rec, 0, &TargetKind::PoolBall(7)));
        assert!(!target_hit(&pool_rec, 0, &TargetKind::PoolBall(8)));
        let digit_rec = rec(vec![vec![7, 0, 2]]);
        assert!(target_hit(&digit_rec, 0, &TargetKind::DigitAt { pos: 0, digit: 7 }));
        assert!(!target_hit(&digit_rec, 0, &TargetKind::DigitAt { pos: 0, digit: 3 }));
    }

    #[test]
    fn default_target_values() {
        match default_target(&Component::Pool { label: "x", size: 33, pick: 6 }) {
            TargetKind::PoolBall(b) => assert_eq!(b, 7),
            _ => panic!("expected PoolBall"),
        }
        match default_target(&Component::Digits { label: "x", bases: vec![10, 10, 10] }) {
            TargetKind::DigitAt { pos, digit } => { assert_eq!(pos, 0); assert_eq!(digit, 7); }
            _ => panic!("expected DigitAt"),
        }
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find ... TargetKind` / `default_target`.

- [ ] **Step 3: 实现**（加到 `src/realdata.rs`,均匀性函数之后)

```rust
pub(crate) enum TargetKind {
    PoolBall(u32),
    DigitAt { pos: usize, digit: u32 },
}

// 每个组件的固定代表 target(可复现)。
pub(crate) fn default_target(comp: &Component) -> TargetKind {
    match comp {
        Component::Pool { size, .. } => TargetKind::PoolBall((*size).min(7)),
        Component::Digits { bases, .. } => TargetKind::DigitAt { pos: 0, digit: (bases[0] - 1).min(7) },
    }
}

// 一期记录里,该 target 是否"出现"。
pub(crate) fn target_hit(rec: &DrawRecord, comp_idx: usize, target: &TargetKind) -> bool {
    let seg = &rec.components[comp_idx];
    match target {
        TargetKind::PoolBall(b) => seg.contains(b),
        TargetKind::DigitAt { pos, digit } => seg[*pos] == *digit,
    }
}

// 该 target 的无条件出现概率。
fn base_prob(comp: &Component, target: &TargetKind) -> f64 {
    match (comp, target) {
        (Component::Pool { size, pick, .. }, _) => *pick as f64 / *size as f64,
        (Component::Digits { bases, .. }, TargetKind::DigitAt { pos, .. }) => 1.0 / bases[*pos] as f64,
        _ => 0.0,
    }
}

// [真实] 赌徒谬误:每组件挑代表 target,验条件概率 ≈ 无条件。
pub(crate) fn analyze_gamblers_fallacy(spec: &GameSpec, draws: &[DrawRecord]) {
    println!("\n-- [真实] 赌徒谬误检验 --");
    for (ci, comp) in spec.components.iter().enumerate() {
        let target = default_target(comp);
        let (mut gap_hit, mut gap_miss) = (0u64, 0u64);
        let mut prev_absent = false;
        for d in draws {
            let hit = target_hit(d, ci, &target);
            if prev_absent {
                if hit { gap_hit += 1; } else { gap_miss += 1; }
            }
            prev_absent = !hit;
        }
        let denom = gap_hit + gap_miss;
        let base = base_prob(comp, &target);
        let label = describe_target(comp, &target);
        if denom == 0 {
            println!("[{}] 样本不足,跳过。", label);
            continue;
        }
        let cond = gap_hit as f64 / denom as f64;
        println!(
            "[{}] 无条件 P={:.4}  条件 P(出|上期没出)={:.4}(样本{}次)  差={:.4} =>历史遗漏无影响。",
            label, base, cond, denom, (cond - base).abs()
        );
    }
}

// [真实] 游程检验:代表 target 的逐期出现序列是否独立。
pub(crate) fn analyze_runs(spec: &GameSpec, draws: &[DrawRecord]) {
    println!("\n-- [真实] 游程检验 --");
    for (ci, comp) in spec.components.iter().enumerate() {
        let target = default_target(comp);
        let seq: Vec<bool> = draws.iter().map(|d| target_hit(d, ci, &target)).collect();
        let (runs, mu, z) = crate::runs_z(&seq);
        let p = crate::normal_two_sided_p(z);
        let label = describe_target(comp, &target);
        println!(
            "[{}] 出现 {} 次  R={:.0} μ={:.1} Z={:.3} 双尾p={:.4} =>{}",
            label, seq.iter().filter(|&&b| b).count(), runs, mu, z, p,
            if p > 0.05 { "序列独立" } else { "偶然显著(小样本)" }
        );
    }
}

// 生成 target 的人类可读标签。
fn describe_target(comp: &Component, target: &TargetKind) -> String {
    match (comp, target) {
        (Component::Pool { label, .. }, TargetKind::PoolBall(b)) => format!("{} 号{:02}", label, b),
        (Component::Digits { label, .. }, TargetKind::DigitAt { pos, digit }) => {
            format!("{} 第{}位=数字{}", label, pos + 1, digit)
        }
        _ => "?".into(),
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: `target_hit_pool_and_digit`、`default_target_values` PASS,全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/realdata.rs
git commit -m "feat: generic gambler's-fallacy and runs tests over target kinds"
```

---

### Task 5: 预测"打脸"实验(泛化)

**Files:**
- Modify: `src/realdata.rs`

**Interfaces:**
- Consumes: `crate::Rng` (`below`, `sample`).
- Produces:
  - `pub(crate) struct CompPred { pub label: String, pub cold: f64, pub hot: f64, pub random: f64, pub expected: f64 }`
  - `pub(crate) fn prediction_stats(spec: &GameSpec, draws: &[DrawRecord], window: usize, rng: &mut crate::Rng) -> (usize, Vec<CompPred>)`
  - `pub(crate) fn print_prediction(n: usize, preds: &[CompPred])`

- [ ] **Step 1: 写失败测试**（加到 `mod tests`)

```rust
    #[test]
    fn pool_hot_wins_on_rigged_data() {
        let ssq = real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        let rows: Vec<DrawRecord> = (0..40).map(|_| rec(vec![vec![1, 2, 3, 4, 5, 6], vec![9]])).collect();
        let mut rng = crate::Rng::new(1);
        let (n, preds) = prediction_stats(&ssq, &rows, 30, &mut rng);
        assert!(n > 0);
        // 组件 0 = 红球:热策略每期命中 6,冷策略 0
        assert!((preds[0].hot - 6.0).abs() < 1e-9, "hot={}", preds[0].hot);
        assert!(preds[0].cold.abs() < 1e-9, "cold={}", preds[0].cold);
    }

    #[test]
    fn digit_hot_wins_on_rigged_data() {
        let d3 = real_data_games().into_iter().find(|g| g.key == "d3").unwrap();
        let rows: Vec<DrawRecord> = (0..40).map(|_| rec(vec![vec![7, 7, 7]])).collect();
        let mut rng = crate::Rng::new(2);
        let (n, preds) = prediction_stats(&d3, &rows, 30, &mut rng);
        assert!(n > 0);
        // 3 位都固定为 7:热策略每期命中 3 位,冷策略 0
        assert!((preds[0].hot - 3.0).abs() < 1e-9, "hot={}", preds[0].hot);
        assert!(preds[0].cold.abs() < 1e-9, "cold={}", preds[0].cold);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function prediction_stats`.

- [ ] **Step 3: 实现**（加到 `src/realdata.rs`,游程函数之后)

```rust
pub(crate) struct CompPred {
    pub label: String,
    pub cold: f64,
    pub hot: f64,
    pub random: f64,
    pub expected: f64,
}

// 从窗口计数选号:Pool 取 pick 个(热=最多/冷=最少),平局按号升序。
fn pick_pool(wc: &[u64], size: u32, pick: u32, want_hot: bool) -> Vec<u32> {
    let mut idx: Vec<u32> = (1..=size).collect();
    idx.sort_by(|&a, &b| {
        let ord = wc[a as usize].cmp(&wc[b as usize]);
        let ord = if want_hot { ord.reverse() } else { ord };
        ord.then(a.cmp(&b))
    });
    idx.into_iter().take(pick as usize).collect()
}

// 从窗口计数选一个数字:热=最多/冷=最少,平局按数字升序。
fn pick_digit(wc: &[u64], want_hot: bool) -> u32 {
    let base = wc.len() as u32;
    let mut idx: Vec<u32> = (0..base).collect();
    idx.sort_by(|&a, &b| {
        let ord = wc[a as usize].cmp(&wc[b as usize]);
        let ord = if want_hot { ord.reverse() } else { ord };
        ord.then(a.cmp(&b))
    });
    idx[0]
}

fn overlap(pred: &[u32], actual: &[u32]) -> u64 {
    pred.iter().filter(|n| actual.contains(n)).count() as u64
}

// 遍历历史,从第 window 期起用前 window 期走势预测下一期,冷/热/随机三策略,按组件回测。
pub(crate) fn prediction_stats(
    spec: &GameSpec,
    draws: &[DrawRecord],
    window: usize,
    rng: &mut crate::Rng,
) -> (usize, Vec<CompPred>) {
    if draws.len() <= window {
        return (0, vec![]);
    }
    let ncomp = spec.components.len();
    let mut sums = vec![[0u64; 3]; ncomp]; // [cold, hot, random]
    let mut n = 0u64;
    for i in window..draws.len() {
        for (ci, comp) in spec.components.iter().enumerate() {
            match comp {
                Component::Pool { size, pick, .. } => {
                    let mut wc = vec![0u64; *size as usize + 1];
                    for d in &draws[i - window..i] {
                        for &b in &d.components[ci] {
                            wc[b as usize] += 1;
                        }
                    }
                    let cold = pick_pool(&wc, *size, *pick, false);
                    let hot = pick_pool(&wc, *size, *pick, true);
                    let rnd = rng.sample(*size, *pick);
                    let actual = &draws[i].components[ci];
                    sums[ci][0] += overlap(&cold, actual);
                    sums[ci][1] += overlap(&hot, actual);
                    sums[ci][2] += overlap(&rnd, actual);
                }
                Component::Digits { bases, .. } => {
                    let actual = &draws[i].components[ci];
                    for (pos, &base) in bases.iter().enumerate() {
                        let mut wc = vec![0u64; base as usize];
                        for d in &draws[i - window..i] {
                            wc[d.components[ci][pos] as usize] += 1;
                        }
                        let cold = pick_digit(&wc, false);
                        let hot = pick_digit(&wc, true);
                        let rnd = rng.below(base as u64) as u32;
                        if cold == actual[pos] { sums[ci][0] += 1; }
                        if hot == actual[pos] { sums[ci][1] += 1; }
                        if rnd == actual[pos] { sums[ci][2] += 1; }
                    }
                }
            }
        }
        n += 1;
    }
    let nf = n as f64;
    let preds = spec
        .components
        .iter()
        .enumerate()
        .map(|(ci, comp)| {
            let expected = match comp {
                Component::Pool { size, pick, .. } => *pick as f64 * *pick as f64 / *size as f64,
                Component::Digits { bases, .. } => bases.iter().map(|&b| 1.0 / b as f64).sum(),
            };
            let label = match comp {
                Component::Pool { label, .. } => label.to_string(),
                Component::Digits { label, .. } => label.to_string(),
            };
            CompPred {
                label,
                cold: sums[ci][0] as f64 / nf,
                hot: sums[ci][1] as f64 / nf,
                random: sums[ci][2] as f64 / nf,
                expected,
            }
        })
        .collect();
    (n as usize, preds)
}

pub(crate) fn print_prediction(n: usize, preds: &[CompPred]) {
    println!("\n-- [真实] 预测'打脸'实验(冷/热/随机)--");
    if n == 0 {
        println!("真实期数不足(需 > 窗口期),跳过。");
        return;
    }
    println!("回测 {} 期,各组件平均命中数:", n);
    for p in preds {
        let spread = [p.cold, p.hot, p.random]
            .iter()
            .map(|v| (v - p.expected).abs())
            .fold(0.0f64, f64::max);
        let verdict = if spread < 0.15 {
            "三策略贴近理论,无策略优于随机"
        } else {
            "样本涨落,长期仍无优势"
        };
        println!(
            "  [{}] 冷 {:.4} / 热 {:.4} / 随机 {:.4}  理论期望 {:.4}  => {}",
            p.label, p.cold, p.hot, p.random, p.expected, verdict
        );
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: `pool_hot_wins_on_rigged_data`(hot=6.0,cold=0.0)、`digit_hot_wins_on_rigged_data`(hot=3.0,cold=0.0)PASS,全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/realdata.rs
git commit -m "feat: generic prediction experiment for pool and digit games"
```

---

### Task 6: 编排 + main 接入 + 删除 ssq + 数据/README

**Files:**
- Modify: `src/realdata.rs`(加编排函数)
- Modify: `src/main.rs`(移除 `mod ssq;` 与旧第 7 章,改调 `realdata::run_all_real_data`)
- Delete: `src/ssq.rs`
- Create: `data/d3.csv`
- Modify: `data/README.md`

**Interfaces:**
- Consumes: 上述所有 realdata 分析/预测函数、`game_spec::real_data_games`。
- Produces:
  - `pub(crate) fn run_game_report(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng)`
  - `pub(crate) fn run_all_real_data(rng: &mut crate::Rng)`

- [ ] **Step 1: 实现编排**（加到 `src/realdata.rs`,`print_prediction` 之后、`#[cfg(test)]` 之前)

```rust
use crate::game_spec::real_data_games;

// 单彩种报告:覆盖行 + 四项分析 + 预测实验。
pub(crate) fn run_game_report(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng) {
    let first = &draws[0];
    let last = &draws[draws.len() - 1];
    println!(
        "数据覆盖:{}({}) → {}({}),共 {} 期。最新一期号码 {:?}",
        first.issue, first.date, last.issue, last.date, draws.len(), last.components
    );
    analyze_uniformity(spec, draws);
    analyze_gamblers_fallacy(spec, draws);
    analyze_runs(spec, draws);
    let (n, preds) = prediction_stats(spec, draws, 30, rng);
    print_prediction(n, &preds);
}

// 第 7 章总编排:遍历 8 种彩票,有数据文件的跑完整分析,缺失则一行跳过。
pub(crate) fn run_all_real_data(rng: &mut crate::Rng) {
    println!("\n========== 7. 真实历史数据篇(全彩种)==========");
    for spec in real_data_games() {
        match load_game(&spec) {
            Ok((draws, skips)) => {
                println!(
                    "\n【{}】{}:解析 {} 期,跳过 {} 行。",
                    spec.name, spec.file, draws.len(), skips.len()
                );
                for s in skips.iter().take(5) {
                    println!("  [跳过] 第 {} 行:{}", s.line, s.reason);
                }
                if draws.len() < 2 {
                    println!("  有效数据不足(<2 期),跳过分析。");
                } else {
                    run_game_report(&spec, &draws, rng);
                }
            }
            Err(_) => {
                println!("\n【{}】未找到 {},跳过(填入真实数据即可启用)。", spec.name, spec.file);
            }
        }
    }
}
```

- [ ] **Step 2: 改 main:移除旧 ssq 第 7 章,改调通用编排**

在 `src/main.rs`:
1. 删除 `mod ssq;` 一行(保留 `mod game_spec;` 与 `mod realdata;`)。
2. 找到 `main()` 中以 `println!("\n========== 7. 真实历史数据篇(双色球)==========");` 开头、到调用 `ssq::run_real_data_report` 及其 `match ssq::load_ssq(...)` 结束的整段(A 阶段插入的第 7 章代码块),**整段替换**为:

```rust
    realdata::run_all_real_data(&mut rng);
```

（保留其后的"总结"打印。第 7 条总结行文案可保持不变。)

- [ ] **Step 3: 删除 src/ssq.rs**

```bash
git rm src/ssq.rs
```

- [ ] **Step 4: 创建数字型样例数据 data/d3.csv**

Create `data/d3.csv`:

```
# 福彩3D 历史开奖  期号,日期,百,十,个(每位 0-9)
# ⚠ 占位示例,请替换为真实开奖。详见 data/README.md
2024001,2024-01-02,3,8,1
2024002,2024-01-03,0,5,9
2024003,2024-01-04,7,7,2
2024004,2024-01-05,1,4,6
2024005,2024-01-06,9,0,3
```

- [ ] **Step 5: 更新 data/README.md 覆盖全部 8 种**

Overwrite `data/README.md` with:

```markdown
# 彩票历史开奖数据

程序第 7 章会遍历 8 种彩票,分别读取下列文件(缺失则跳过该彩种)。
每行一期,逗号分隔:`期号,日期,号码...`。`#` 注释、空行忽略,首行可为表头。
不合规的行会被跳过并报告行号与原因。号码越多、期数越多(建议数百期),统计越有意义。
预测实验需超过 30 期才会运行。

| 文件 | 彩种 | 号码字段(顺序) | 规则 |
|---|---|---|---|
| ssq.csv | 双色球 | 红1..红6,蓝 | 红 6 个 1-33 互异;蓝 1 个 1-16 |
| dlt.csv | 超级大乐透 | 前1..前5,后1,后2 | 前 5 个 1-35 互异;后 2 个 1-12 互异 |
| d3.csv | 福彩3D | 百,十,个 | 各 0-9,可重复 |
| pl3.csv | 排列3 | 百,十,个 | 各 0-9,可重复 |
| pl5.csv | 排列5 | 万,千,百,十,个 | 各 0-9,可重复 |
| qxc.csv | 7星彩 | 第1..第6位,末位 | 前 6 位 0-9;末位 0-14 |
| qlc.csv | 7乐彩 | 号1..号7 | 7 个 1-30 互异 |
| kl8.csv | 快乐8 | 号1..号20 | 20 个 1-80 互异 |

当前仓库内的 `ssq.csv` 与 `d3.csv` 为占位示例,请替换为真实开奖数据。

## 数据来源

可从中国福利彩票 / 体育彩票官方网站或公开数据集获取历史开奖号码,整理为上述格式。
```

- [ ] **Step 6: 运行测试与程序,确认全彩种输出**

Run: `cargo test`
Expected: 全部 PASS,且现在应无 dead-code 警告(所有函数已被 `run_all_real_data` 串起)。
Run: `cargo run --release`
Expected: 第 7 章遍历 8 种彩票;ssq 与 d3 打印覆盖行 + 四项分析(ssq 红/蓝两组件、d3 三位),预测实验因样例期数少打印"真实期数不足…跳过";其余 6 种打印"未找到 … 跳过"。

- [ ] **Step 7: 手动验证降级路径**

临时将 `data/ssq.csv` 重命名(PowerShell `Rename-Item data\ssq.csv ssq.bak` 或 Bash `mv data/ssq.csv data/ssq.bak`),`cargo run --release` 确认 ssq 行显示"未找到 … 跳过",随后改回。

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: wire generic multi-game real-data report; remove ssq module"
```

---

## 自查(计划 vs spec)

- **① 核心模型**:Task 1 `Component`/`GameSpec`/`real_data_games`/`field_count`/`width`,8 配置逐字匹配 Global Constraints。✅
- **② 文件格式与解析**:Task 2 `parse_record`/`parse_lines`/`load_game`,Pool 互异+排序、Digits 允许重复+不排序、字段数由 spec 算。✅
- **③ 通用分析引擎**:Task 3 均匀性(Pool 号池 + Digits 逐位),Task 4 `TargetKind`/`target_hit`/`default_target` + 赌徒谬误 + 游程。✅
- **④ 预测实验泛化**:Task 5 Pool 重叠(pick·pick/size)+ Digits 逐位(Σ1/base),平局升序,数据不足返回 n=0。✅
- **⑤ 模块结构/重构/测试/数据**:Task 6 删 ssq、改 main、d3 样例、README 全彩种;各任务含 TDD 测试,Task 5 含 Pool+Digits 双 rigged 反向验证,Task 3 含 pool_counts 回归。✅
- **占位符扫描**:无 TBD/TODO,每个代码步骤含完整代码(Task 3 的 `let _ = crate::format_int;` 已注明可省略)。✅
- **类型一致性**:`DrawRecord.components: Vec<Vec<u32>>`、`Component::{Pool,Digits}` 字段名、`prediction_stats -> (usize, Vec<CompPred>)`、`CompPred` 字段、`analyze_*(spec, draws)` 签名跨任务一致。✅
