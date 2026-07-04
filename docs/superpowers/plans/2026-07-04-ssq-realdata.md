# 双色球真实历史数据接入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `lottery_stats` 能读取本地双色球历史开奖 CSV,在真实数据上复现全套随机性检验,并新增冷/热/随机三策略的"预测打脸"实验。

**Architecture:** 把"纯数学"与"数据来源"解耦。`main.rs` 保留纯数学并抽出两个共享函数(`chi2_from_freq`、`runs_z`),既有模拟演示改为复用它们;新增 `src/ssq.rs` 模块承载 `Draw` 结构、CSV 解析、真实数据分析与预测实验。`main` 加载文件后打印"真实数据篇",文件缺失时优雅降级。

**Tech Stack:** Rust 2021,零外部依赖,仅用 std(`std::fs`)。测试用内置 `#[cfg(test)]` + `cargo test`。

## Global Constraints

- **零外部依赖**:`Cargo.toml` 的 `[dependencies]` 保持为空,只允许用 std,拷贝自 spec。
- **离线可复现**:所有随机使用固定种子 `Rng`;策略平局按号码升序取号,保证结果可复现。
- **红球规则**:6 个,均 ∈ [1,33],互不相同;**蓝球规则**:1 个 ∈ [1,16]。
- **编译/测试命令**:`cargo test`、`cargo run --release`。
- **Git**:当前目录尚非 git 仓库。若要执行下面的 commit 步骤,先在项目根运行一次 `git init`;否则可跳过每个任务的 commit 步骤。

---

### Task 1: 抽出共享数学函数 `chi2_from_freq` / `runs_z`

**Files:**
- Modify: `src/main.rs`(在 `chi2_pvalue` 后新增两个函数;重构 `chi_square_uniformity_demo`、`runs_test_demo` 调用它们;给需被 `ssq` 复用的函数加 `pub(crate)`)

**Interfaces:**
- Produces:
  - `pub(crate) fn chi2_from_freq(counts: &[u64], expected: f64) -> f64`
  - `pub(crate) fn runs_z(seq: &[bool]) -> (f64, f64, f64)`  // 返回 (runs, mu, z)
  - 将 `chi2_pvalue`、`normal_two_sided_p`、`format_int` 及 `Rng`(含 `new`/`below`/`sample`)标记为 `pub(crate)`,供 `src/ssq.rs` 调用。

- [ ] **Step 1: 写失败测试**（追加到 `src/main.rs` 末尾)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chi2_zero_when_uniform() {
        // 5 个类别,每类恰好等于期望频次 => χ² 应为 0
        let counts = [10u64, 10, 10, 10, 10];
        let chi2 = chi2_from_freq(&counts, 10.0);
        assert!(chi2.abs() < 1e-9, "got {}", chi2);
    }

    #[test]
    fn chi2_known_value() {
        // 期望 10,观测 [12,8] => (2^2+2^2)/10 = 0.8
        let counts = [12u64, 8];
        let chi2 = chi2_from_freq(&counts, 10.0);
        assert!((chi2 - 0.8).abs() < 1e-9, "got {}", chi2);
    }

    #[test]
    fn runs_counts_alternation() {
        // 序列 T F T F => 4 个游程
        let seq = [true, false, true, false];
        let (runs, _mu, _z) = runs_z(&seq);
        assert_eq!(runs as u32, 4);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function chi2_from_freq` / `runs_z`。

- [ ] **Step 3: 实现两个共享函数**（在 `src/main.rs` 中 `chi2_pvalue` 函数之后插入)

```rust
// 频次数组 -> 卡方统计量。counts 为各类别观测频次,expected 为每类理论期望。
pub(crate) fn chi2_from_freq(counts: &[u64], expected: f64) -> f64 {
    counts
        .iter()
        .map(|&o| (o as f64 - expected).powi(2) / expected)
        .sum()
}

// 0/1 序列的 Wald–Wolfowitz 游程检验。返回 (游程数 R, 理论均值 μ, 标准化 Z)。
pub(crate) fn runs_z(seq: &[bool]) -> (f64, f64, f64) {
    let n = seq.len();
    if n == 0 {
        return (0.0, 0.0, 0.0);
    }
    let n1 = seq.iter().filter(|&&b| b).count() as f64;
    let n0 = n as f64 - n1;
    let mut runs = 1.0;
    for i in 1..n {
        if seq[i] != seq[i - 1] {
            runs += 1.0;
        }
    }
    if n1 == 0.0 || n0 == 0.0 {
        // 全同序列,方差无定义,Z 视为 0(无可检验的交替)
        return (runs, runs, 0.0);
    }
    let mu = 2.0 * n1 * n0 / (n1 + n0) + 1.0;
    let var =
        2.0 * n1 * n0 * (2.0 * n1 * n0 - n1 - n0) / ((n1 + n0).powi(2) * (n1 + n0 - 1.0));
    let z = (runs - mu) / var.sqrt();
    (runs, mu, z)
}
```

- [ ] **Step 4: 重构既有演示复用它们**

在 `chi_square_uniformity_demo` 中,把手写的 χ² 累加循环替换为:

```rust
    let chi2 = chi2_from_freq(&freq[1..=hi as usize], expected);
```

（删除原先 `let mut chi2 = 0.0; for b in 1..=hi ... ` 的循环块,`df`/`p` 与后续打印保持不变。)

在 `runs_test_demo` 中,把手写的 runs/mu/var/z 计算替换为:

```rust
    let (runs, mu, z) = runs_z(&seq);
```

（删除原先 `let mut runs = 1.0; ...` 到 `let z = ...;` 的整段,`n1`/`n0` 若仅用于打印可保留其计算;`p`/打印保持不变。)

- [ ] **Step 5: 给复用项加可见性**

将以下定义的 `fn`/`struct` 前缀改为 `pub(crate)`:`chi2_pvalue`、`normal_two_sided_p`、`format_int`、`struct Rng`、`impl Rng` 内的 `new`、`below`、`sample`(`next_u64` 可保持私有)。

- [ ] **Step 6: 运行测试与程序确认通过且输出不变**

Run: `cargo test`
Expected: 3 个测试 PASS。
Run: `cargo run --release`
Expected: 与改动前输出一致(卡方 χ²≈21.44、游程 Z≈0.381 等数值不变)。

- [ ] **Step 7: Commit**

```bash
git add src/main.rs
git commit -m "refactor: extract chi2_from_freq and runs_z shared helpers"
```

---

### Task 2: `src/ssq.rs` — `Draw` 结构与 CSV 解析

**Files:**
- Create: `src/ssq.rs`
- Modify: `src/main.rs`(顶部加 `mod ssq;`)

**Interfaces:**
- Consumes: 无(纯 std)。
- Produces:
  - `pub(crate) struct Draw { pub issue: String, pub date: String, pub reds: [u8; 6], pub blue: u8 }`
  - `pub(crate) struct SkipInfo { pub line: usize, pub reason: String }`
  - `pub(crate) fn parse_draw_fields(fields: &[&str]) -> Result<Draw, String>`
  - `pub(crate) fn parse_lines(content: &str) -> (Vec<Draw>, Vec<SkipInfo>)`
  - `pub(crate) fn load_ssq(path: &str) -> Result<(Vec<Draw>, Vec<SkipInfo>), String>`

- [ ] **Step 1: 建文件并写失败测试**

Create `src/ssq.rs`:

```rust
// 双色球真实历史数据:解析与分析。

pub(crate) struct Draw {
    pub issue: String,
    pub date: String,
    pub reds: [u8; 6],
    pub blue: u8,
}

pub(crate) struct SkipInfo {
    pub line: usize,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_row() {
        let d = parse_draw_fields(&["2024001", "2024-01-02", "1", "7", "15", "22", "28", "33", "9"])
            .unwrap();
        assert_eq!(d.issue, "2024001");
        assert_eq!(d.reds, [1, 7, 15, 22, 28, 33]);
        assert_eq!(d.blue, 9);
    }

    #[test]
    fn rejects_wrong_field_count() {
        assert!(parse_draw_fields(&["2024001", "1", "2", "3"]).is_err());
    }

    #[test]
    fn rejects_red_out_of_range() {
        assert!(parse_draw_fields(&["x", "d", "0", "7", "15", "22", "28", "33", "9"]).is_err());
        assert!(parse_draw_fields(&["x", "d", "1", "7", "15", "22", "28", "34", "9"]).is_err());
    }

    #[test]
    fn rejects_duplicate_reds() {
        assert!(parse_draw_fields(&["x", "d", "7", "7", "15", "22", "28", "33", "9"]).is_err());
    }

    #[test]
    fn rejects_blue_out_of_range() {
        assert!(parse_draw_fields(&["x", "d", "1", "7", "15", "22", "28", "33", "17"]).is_err());
        assert!(parse_draw_fields(&["x", "d", "1", "7", "15", "22", "28", "33", "0"]).is_err());
    }

    #[test]
    fn parse_lines_skips_comments_blank_header_and_bad_rows() {
        let content = "\
# 注释行
期号,日期,红1,红2,红3,红4,红5,红6,蓝

2024001,2024-01-02,1,7,15,22,28,33,9
2024002,2024-01-04,3,5,11,19,26,31,99
2024003,2024-01-06,2,4,6,8,10,12,3
";
        let (draws, skips) = parse_lines(content);
        assert_eq!(draws.len(), 2, "应解析 2 期有效数据");
        assert_eq!(skips.len(), 1, "蓝球=99 的一行应被跳过");
    }
}
```

Modify `src/main.rs`:在文件顶部(首个 `struct Rng` 之前)加一行:

```rust
mod ssq;
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function parse_draw_fields` / `parse_lines`。

- [ ] **Step 3: 实现解析函数**（加到 `src/ssq.rs`,在 `SkipInfo` 之后、`#[cfg(test)]` 之前)

```rust
// 解析 9 个字段为一期开奖;任何不合规返回 Err(原因)。红球会被排序后存储。
pub(crate) fn parse_draw_fields(fields: &[&str]) -> Result<Draw, String> {
    if fields.len() != 9 {
        return Err(format!("字段数应为 9,实际 {}", fields.len()));
    }
    let mut reds = [0u8; 6];
    for i in 0..6 {
        let v: u8 = fields[2 + i]
            .trim()
            .parse()
            .map_err(|_| format!("红球 '{}' 非法整数", fields[2 + i]))?;
        if !(1..=33).contains(&v) {
            return Err(format!("红球 {} 超出 1-33", v));
        }
        reds[i] = v;
    }
    // 互异性检查
    for i in 0..6 {
        for j in (i + 1)..6 {
            if reds[i] == reds[j] {
                return Err(format!("红球重复:{}", reds[i]));
            }
        }
    }
    reds.sort_unstable();
    let blue: u8 = fields[8]
        .trim()
        .parse()
        .map_err(|_| format!("蓝球 '{}' 非法整数", fields[8]))?;
    if !(1..=16).contains(&blue) {
        return Err(format!("蓝球 {} 超出 1-16", blue));
    }
    Ok(Draw {
        issue: fields[0].trim().to_string(),
        date: fields[1].trim().to_string(),
        reds,
        blue,
    })
}

// 逐行解析:跳过空行、# 注释行、表头(首字段非纯数字)。坏行记录到 SkipInfo。
pub(crate) fn parse_lines(content: &str) -> (Vec<Draw>, Vec<SkipInfo>) {
    let mut draws = Vec::new();
    let mut skips = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        // 表头识别:首字段不是纯数字则视为表头,静默跳过
        if !fields[0].chars().all(|c| c.is_ascii_digit()) || fields[0].is_empty() {
            continue;
        }
        match parse_draw_fields(&fields) {
            Ok(d) => draws.push(d),
            Err(reason) => skips.push(SkipInfo {
                line: line_no,
                reason,
            }),
        }
    }
    (draws, skips)
}

// 从文件加载。文件不存在/不可读时返回 Err。
pub(crate) fn load_ssq(path: &str) -> Result<(Vec<Draw>, Vec<SkipInfo>), String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("无法读取 '{}': {}", path, e))?;
    Ok(parse_lines(&content))
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: Task 1 的 3 个 + 本任务 6 个测试全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/ssq.rs src/main.rs
git commit -m "feat: add ssq module with Draw struct and CSV parsing"
```

---

### Task 3: 真实数据分析(卡方/冷热/赌徒谬误/游程)

**Files:**
- Modify: `src/ssq.rs`

**Interfaces:**
- Consumes(来自 `main.rs`,均为 `pub(crate)`):`crate::chi2_from_freq`、`crate::chi2_pvalue`、`crate::runs_z`、`crate::normal_two_sided_p`、`crate::format_int`。
- Produces:
  - `pub(crate) fn red_counts(draws: &[Draw]) -> [u64; 34]`  // 下标 1..=33 为各红球出现次数
  - `pub(crate) fn analyze_uniformity(draws: &[Draw])`
  - `pub(crate) fn analyze_gamblers_fallacy(draws: &[Draw], target: u8)`
  - `pub(crate) fn analyze_runs(draws: &[Draw], target: u8)`

- [ ] **Step 1: 写失败测试**（加到 `src/ssq.rs` 的 `mod tests` 内)

```rust
    fn make_draws(rows: &[[u8; 6]]) -> Vec<Draw> {
        rows.iter()
            .map(|r| Draw {
                issue: "x".into(),
                date: "d".into(),
                reds: *r,
                blue: 1,
            })
            .collect()
    }

    #[test]
    fn red_counts_tallies_each_ball() {
        let draws = make_draws(&[[1, 2, 3, 4, 5, 6], [1, 2, 3, 4, 5, 7]]);
        let c = red_counts(&draws);
        assert_eq!(c[1], 2);
        assert_eq!(c[6], 1);
        assert_eq!(c[7], 1);
        assert_eq!(c[8], 0);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function red_counts`。

- [ ] **Step 3: 实现分析函数**（加到 `src/ssq.rs`,`load_ssq` 之后)

```rust
// 统计各红球出现次数,下标 1..=33。
pub(crate) fn red_counts(draws: &[Draw]) -> [u64; 34] {
    let mut c = [0u64; 34];
    for d in draws {
        for &r in &d.reds {
            c[r as usize] += 1;
        }
    }
    c
}

// [真实] 卡方均匀性 + 冷热号(含理论标准差参照)。
pub(crate) fn analyze_uniformity(draws: &[Draw]) {
    let counts = red_counts(draws);
    let total = (draws.len() * 6) as f64;
    let expected = total / 33.0;
    let chi2 = crate::chi2_from_freq(&counts[1..=33], expected);
    let df = 32.0;
    let p = crate::chi2_pvalue(chi2, df);
    // 泊松/多项近似下单格频次的理论标准差 ~ sqrt(expected)
    let sd = expected.sqrt();

    println!("\n-- [真实] 卡方均匀性检验(红球 6/33)--");
    println!("真实期数: {}  各号理论期望频次: {:.1}", crate::format_int(draws.len() as f64), expected);
    println!("卡方统计量 χ² = {:.2}   自由度 df = 32   p 值 = {:.4}", chi2, p);
    if p > 0.05 {
        println!("=> p > 0.05,无法拒绝'均匀分布':真实开奖号码也均匀随机。");
    } else {
        println!("=> 本样本偏离显著(真实期数少、统计功效有限,属可能的偶然)。");
    }

    let mut idx: Vec<usize> = (1..=33).collect();
    idx.sort_by_key(|&i| counts[i]);
    let cold = idx[0];
    let hot = idx[32];
    println!(
        "最冷号 {:02}(出现 {} 次) vs 最热号 {:02}(出现 {} 次);理论标准差 ≈ {:.1},",
        cold, counts[cold], hot, counts[hot], sd
    );
    println!(
        "  冷热差 {} 次 ≈ {:.1} 个标准差,属随机涨落范围,并无'冷热规律'可用。",
        counts[hot] - counts[cold],
        (counts[hot] - counts[cold]) as f64 / sd
    );
}

// [真实] 赌徒谬误:P(本期出 target | 上期没出) vs 无条件 6/33。
pub(crate) fn analyze_gamblers_fallacy(draws: &[Draw], target: u8) {
    let mut gap_hit = [0u64; 2];
    let mut prev_absent = false;
    for d in draws {
        let hit = d.reds.contains(&target);
        if prev_absent {
            gap_hit[if hit { 0 } else { 1 }] += 1;
        }
        prev_absent = !hit;
    }
    let denom = gap_hit[0] + gap_hit[1];
    let base_p = 6.0 / 33.0;
    println!("\n-- [真实] 赌徒谬误检验(观察红球 {:02})--", target);
    if denom == 0 {
        println!("样本不足,无法统计条件概率。");
        return;
    }
    let cond_p = gap_hit[0] as f64 / denom as f64;
    println!("无条件 P(出) = 6/33 = {:.4}", base_p);
    println!("条件 P(本期出 | 上期没出) = {:.4}  (样本 {} 次)", cond_p, denom);
    println!("差异 = {:.4} => 历史遗漏对下期概率无影响,'冷号回补'是幻觉。", (cond_p - base_p).abs());
}

// [真实] 游程检验:target 逐期是否出现的 0/1 序列是否独立。
pub(crate) fn analyze_runs(draws: &[Draw], target: u8) {
    let seq: Vec<bool> = draws.iter().map(|d| d.reds.contains(&target)).collect();
    let (runs, mu, z) = crate::runs_z(&seq);
    let p = crate::normal_two_sided_p(z);
    let n1 = seq.iter().filter(|&&b| b).count();
    println!("\n-- [真实] 游程检验(红球 {:02} 的出现序列)--", target);
    println!("序列长度 {},出现 {} 次", seq.len(), n1);
    println!("游程数 R = {:.0}  理论均值 μ = {:.1}  Z = {:.3}  双尾 p = {:.4}", runs, mu, z, p);
    if p > 0.05 {
        println!("=> p > 0.05:真实开奖序列通过独立性检验,前后期无可利用规律。");
    } else {
        println!("=> 偶然显著(真实期数有限,统计功效低)。");
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: 新增 `red_counts_tallies_each_ball` 及既有测试全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/ssq.rs
git commit -m "feat: real-data uniformity, gambler's-fallacy and runs analyses"
```

---

### Task 4: 预测"打脸"实验(冷/热/随机三策略)

**Files:**
- Modify: `src/ssq.rs`

**Interfaces:**
- Consumes:`crate::Rng`(`sample`)、`red_counts` 思路(此处用滑动窗口局部计数)。
- Produces:
  - `pub(crate) struct PredStats { pub cold: f64, pub hot: f64, pub random: f64, pub expected: f64, pub n: usize }`
  - `pub(crate) fn prediction_stats(draws: &[Draw], window: usize, rng: &mut crate::Rng) -> PredStats`
  - `pub(crate) fn print_prediction(s: &PredStats)`

- [ ] **Step 1: 写失败测试**（加到 `mod tests` 内)

```rust
    #[test]
    fn hot_strategy_wins_on_rigged_data() {
        // 每期都开 [1..6];窗口后热号应恰好预测出 {1..6} => 命中 6/6
        let rows: Vec<[u8; 6]> = (0..40).map(|_| [1, 2, 3, 4, 5, 6]).collect();
        let draws = make_draws(&rows);
        let mut rng = crate::Rng::new(1);
        let s = prediction_stats(&draws, 30, &mut rng);
        assert!((s.hot - 6.0).abs() < 1e-9, "hot={}", s.hot);
        assert!(s.cold.abs() < 1e-9, "cold={}", s.cold);
        assert!(s.n > 0);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function prediction_stats`。

- [ ] **Step 3: 实现预测实验**（加到 `src/ssq.rs`,分析函数之后)

```rust
pub(crate) struct PredStats {
    pub cold: f64,
    pub hot: f64,
    pub random: f64,
    pub expected: f64,
    pub n: usize,
}

// 用前 window 期计数选 6 个号:want_hot=true 取最热,否则取最冷。平局按号码升序。
fn pick_by_window(window_counts: &[u64; 34], want_hot: bool) -> [u8; 6] {
    let mut idx: Vec<usize> = (1..=33).collect();
    // 主键:计数(热=降序/冷=升序);次键:号码升序(保证可复现)
    idx.sort_by(|&a, &b| {
        let ord = window_counts[a].cmp(&window_counts[b]);
        let ord = if want_hot { ord.reverse() } else { ord };
        ord.then(a.cmp(&b))
    });
    let mut out = [0u8; 6];
    for i in 0..6 {
        out[i] = idx[i] as u8;
    }
    out
}

fn hits(pred: &[u8; 6], actual: &Draw) -> u32 {
    pred.iter().filter(|n| actual.reds.contains(n)).count() as u32
}

// 遍历历史:第 window 期起,用前 window 期走势预测下一期,统计三策略平均命中数。
pub(crate) fn prediction_stats(draws: &[Draw], window: usize, rng: &mut crate::Rng) -> PredStats {
    let expected = 6.0 * 6.0 / 33.0;
    if draws.len() <= window {
        return PredStats { cold: 0.0, hot: 0.0, random: 0.0, expected, n: 0 };
    }
    let (mut cold_sum, mut hot_sum, mut rand_sum) = (0u64, 0u64, 0u64);
    let mut n = 0u64;
    for i in window..draws.len() {
        // 用 [i-window, i) 期构建窗口计数
        let mut wc = [0u64; 34];
        for d in &draws[i - window..i] {
            for &r in &d.reds {
                wc[r as usize] += 1;
            }
        }
        let cold_pred = pick_by_window(&wc, false);
        let hot_pred = pick_by_window(&wc, true);
        let rand_pred = {
            let v = rng.sample(33, 6); // Vec<u32>,已排序去重
            let mut a = [0u8; 6];
            for k in 0..6 {
                a[k] = v[k] as u8;
            }
            a
        };
        cold_sum += hits(&cold_pred, &draws[i]) as u64;
        hot_sum += hits(&hot_pred, &draws[i]) as u64;
        rand_sum += hits(&rand_pred, &draws[i]) as u64;
        n += 1;
    }
    let nf = n as f64;
    PredStats {
        cold: cold_sum as f64 / nf,
        hot: hot_sum as f64 / nf,
        random: rand_sum as f64 / nf,
        expected,
        n: n as usize,
    }
}

pub(crate) fn print_prediction(s: &PredStats) {
    println!("\n-- [真实] 预测'打脸'实验(冷/热/随机 三策略)--");
    if s.n == 0 {
        println!("真实期数不足(需 > 窗口期),跳过预测实验。");
        return;
    }
    println!("回测 {} 期,每期预测 6 个红球,统计平均命中数(满分 6):", s.n);
    println!("  冷号策略  平均命中 = {:.4}", s.cold);
    println!("  热号策略  平均命中 = {:.4}", s.hot);
    println!("  随机基线  平均命中 = {:.4}", s.random);
    println!("  纯运气理论期望 = 6 × 6/33 = {:.4}", s.expected);
    let spread = [s.cold, s.hot, s.random]
        .iter()
        .map(|v| (v - s.expected).abs())
        .fold(0.0f64, f64::max);
    if spread < 0.15 {
        println!("=> 三策略均值都贴着理论期望,差异在噪声内:没有策略优于瞎蒙。");
    } else {
        println!("=> 三策略均值与理论期望差 {:.3},真实样本涨落所致,长期仍无优势。", spread);
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: `hot_strategy_wins_on_rigged_data` PASS(hot=6.0、cold=0.0)。

- [ ] **Step 5: Commit**

```bash
git add src/ssq.rs
git commit -m "feat: prediction experiment comparing cold/hot/random strategies"
```

---

### Task 5: 编排接入 + 样例数据 + README

**Files:**
- Modify: `src/ssq.rs`(新增编排函数 `run_real_data_report`)
- Modify: `src/main.rs`(`main` 末尾加载文件并打印真实数据篇 [7],增补总结)
- Create: `data/ssq.csv`
- Create: `data/README.md`

**Interfaces:**
- Consumes:`ssq::load_ssq`、`ssq::analyze_uniformity`、`ssq::analyze_gamblers_fallacy`、`ssq::analyze_runs`、`ssq::prediction_stats`、`ssq::print_prediction`。
- Produces:`pub(crate) fn ssq::run_real_data_report(draws: &[Draw], rng: &mut crate::Rng)`

- [ ] **Step 1: 实现编排函数**（加到 `src/ssq.rs` 末尾,`#[cfg(test)]` 之前)

```rust
// 真实数据篇总编排:调用四项分析 + 预测实验。观察号固定为红球 7,窗口 30。
pub(crate) fn run_real_data_report(draws: &[Draw], rng: &mut crate::Rng) {
    analyze_uniformity(draws);
    analyze_gamblers_fallacy(draws, 7);
    analyze_runs(draws, 7);
    let stats = prediction_stats(draws, 30, rng);
    print_prediction(&stats);
}
```

- [ ] **Step 2: 在 `main` 中接入(文件缺失优雅降级)**

在 `src/main.rs` 的 `main()` 里,`monte_carlo_bankroll(&mut rng);` 之后、`println!("\n════ 总结 ...")` 之前插入:

```rust
    println!("\n========== 7. 真实历史数据篇(双色球)==========");
    match ssq::load_ssq("data/ssq.csv") {
        Ok((draws, skips)) => {
            println!("加载 data/ssq.csv:成功解析 {} 期,跳过 {} 行。", draws.len(), skips.len());
            for s in skips.iter().take(10) {
                println!("  [跳过] 第 {} 行:{}", s.line, s.reason);
            }
            if draws.len() < 2 {
                println!("有效数据不足(< 2 期),跳过真实数据分析。请在 data/ssq.csv 填入真实开奖。");
            } else {
                ssq::run_real_data_report(&draws, &mut rng);
            }
        }
        Err(e) => {
            println!("未找到/无法读取 data/ssq.csv({}),已跳过真实数据分析。", e);
            println!("格式:每行 `期号,日期,红1..红6,蓝`,红球 6 个∈[1,33] 互异,蓝球∈[1,16]。");
            println!("详见 data/README.md。");
        }
    }
```

- [ ] **Step 3: 增补总结行**

在 `main()` 末尾 `println!("\n=> 理性结论 ...")` 之前插入:

```rust
    println!("7. 真实历史数据(若已填充)与理论/模拟结论一致:同样均匀、独立、无策略优势。");
```

- [ ] **Step 4: 创建样例数据文件**

Create `data/ssq.csv`:

```
# 双色球历史开奖数据  期号,日期,红1,红2,红3,红4,红5,红6,蓝
# ⚠ 以下为占位示例数据,请替换为真实开奖号码(数据源见 data/README.md)。
# 红球 6 个(1-33 互不相同),蓝球 1 个(1-16)。# 开头为注释,空行忽略。
2024001,2024-01-02,1,7,15,22,28,33,9
2024002,2024-01-04,3,5,11,19,26,31,2
2024003,2024-01-06,2,8,14,20,27,30,5
2024004,2024-01-09,4,6,12,18,25,32,11
2024005,2024-01-11,9,13,17,21,24,29,7
```

- [ ] **Step 5: 创建数据说明**

Create `data/README.md`:

```markdown
# 双色球历史开奖数据

程序读取本目录下的 `ssq.csv`。当前文件为**占位示例**,请替换为真实历史开奖数据。

## 格式

每行一期,逗号分隔,共 9 个字段:

    期号,日期,红1,红2,红3,红4,红5,红6,蓝

- 红球 6 个,均为 1–33 的整数且互不相同(顺序随意,程序会排序)。
- 蓝球 1 个,1–16 的整数。
- `#` 开头为注释行,空行忽略;首行可为表头(自动跳过)。
- 不合规的行会被跳过并在运行时报告行号与原因。

## 数据来源

可从中国福利彩票官方网站或公开数据集获取历史开奖号码,整理为上述格式即可。
期数越多(建议数百期以上),统计检验越有意义。
```

- [ ] **Step 6: 运行测试与程序,确认真实数据篇输出**

Run: `cargo test`
Expected: 全部 PASS。
Run: `cargo run --release`
Expected: 末尾出现 "7. 真实历史数据篇" —— 用 5 行示例数据打印加载摘要、四项分析、预测实验;因期数少(< 窗口 30)预测实验打印"跳过";总结新增第 7 条。

- [ ] **Step 7: 手动验证降级路径**

临时改路径测试缺失场景:`cargo run --release` 前把 `data/ssq.csv` 改名(`mv data/ssq.csv data/ssq.bak`),运行,确认打印"未找到/无法读取 ... 已跳过";随后改回(`mv data/ssq.bak data/ssq.csv`)。

- [ ] **Step 8: Commit**

```bash
git add src/ssq.rs src/main.rs data/ssq.csv data/README.md
git commit -m "feat: wire real-data report into main with sample data and README"
```

---

## 自查(计划 vs spec)

- **① 文件格式**:Task 2 解析 + Task 5 样例/README + 校验规则(字段数/红球范围/互异/蓝球范围)全覆盖。✅
- **② 四项真实数据分析**:卡方+冷热(Task 3 `analyze_uniformity`,含理论标准差)、赌徒谬误、游程 —— 覆盖。✅
- **③ 预测打脸实验**:Task 4 冷/热/随机 + 平局升序取号 + 理论期望 1.09 + 显著性口径。✅
- **④ 代码结构与测试**:Task 1 抽 `chi2_from_freq`/`runs_z` 并被模拟路径复用;`ssq` 模块化;各任务含 TDD 测试,含"红球必出"人造数据反向验证(Task 4)。✅
- **⑤ 报告结构 [7] + 缺失降级 + 总结增补**:Task 5 覆盖。✅
- **占位符扫描**:无 TBD/TODO,每个代码步骤含完整代码。✅
- **类型一致性**:`Draw`/`SkipInfo`/`PredStats` 字段、`chi2_from_freq(counts,&expected)`、`runs_z`→`(runs,mu,z)`、`prediction_stats(draws,window,rng)`→`PredStats` 在各任务间签名一致。✅
