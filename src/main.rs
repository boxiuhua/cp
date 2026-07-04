// =====================================================================
//  彩票随机性验证 · 概率论实证工具  (Rust, 零外部依赖)
// ---------------------------------------------------------------------
//  目的:用真实数学 + 统计检验证明——
//    (1) 8 种中国彩票的头奖概率极低且固定;
//    (2) 每种玩法的期望值恒为负(长期必亏);
//    (3) 开奖号码是均匀随机、前后独立的,任何"算法预测"都无效;
//    (4) "冷号该出了"是赌徒谬误——条件概率不变。
//
//  编译:  cargo run --release
// =====================================================================

mod game_spec;
mod ssq;

// ------------------------- 1. 无依赖 PRNG -----------------------------
// xorshift128+,周期长、质量高,足够做蒙特卡洛与随机性演示。
// 用固定种子 => 结果可复现(科学实验应可重复)。
pub(crate) struct Rng {
    s0: u64,
    s1: u64,
}
impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        // splitmix64 扩散种子,避免弱初始状态
        let mut z = seed.wrapping_add(0x9E3779B97F4A7C15);
        let mut sm = || {
            z = z.wrapping_add(0x9E3779B97F4A7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
            x ^ (x >> 31)
        };
        Rng { s0: sm(), s1: sm() }
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.s0;
        let y = self.s1;
        self.s0 = y;
        x ^= x << 23;
        self.s1 = x ^ y ^ (x >> 17) ^ (y >> 26);
        self.s1.wrapping_add(y)
    }
    // [0, n) 均匀整数,拒绝采样去偏
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        let zone = u64::MAX - u64::MAX % n;
        loop {
            let r = self.next_u64();
            if r < zone {
                return r % n;
            }
        }
    }
    // 从 1..=hi 中不重复抽 k 个(部分 Fisher–Yates)
    pub(crate) fn sample(&mut self, hi: u32, k: u32) -> Vec<u32> {
        let mut pool: Vec<u32> = (1..=hi).collect();
        for i in 0..k as usize {
            let j = i + self.below((hi as usize - i) as u64) as usize;
            pool.swap(i, j);
        }
        let mut out: Vec<u32> = pool[..k as usize].to_vec();
        out.sort_unstable();
        out
    }
}

// --------------------- 2. 数学函数(自实现)---------------------------

// 组合数 C(n,k),用 f64 避免溢出(结果可能达到 10^12 量级)。
fn comb(n: u64, k: u64) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut r = 1.0f64;
    for i in 0..k {
        r = r * (n - i) as f64 / (i + 1) as f64;
    }
    r
}

// ln(Gamma(x)) —— Lanczos 近似,供不完全 Gamma 使用。
fn gammln(x: f64) -> f64 {
    const C: [f64; 6] = [
        76.18009172947146,
        -86.50532032941677,
        24.01409824083091,
        -1.231739572450155,
        0.1208650973866179e-2,
        -0.5395239384953e-5,
    ];
    let mut y = x;
    let tmp = x + 5.5 - (x + 0.5) * (x + 5.5).ln();
    let mut ser = 1.000000000190015;
    for c in C.iter() {
        y += 1.0;
        ser += c / y;
    }
    -tmp + (2.5066282746310005 * ser / x).ln()
}

// 正则化下不完全 Gamma P(a,x)（级数展开,x < a+1 时收敛快）
fn gammp_series(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let gln = gammln(a);
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut del = sum;
    for _ in 0..500 {
        ap += 1.0;
        del *= x / ap;
        sum += del;
        if del.abs() < sum.abs() * 1e-14 {
            break;
        }
    }
    sum * (-x + a * x.ln() - gln).exp()
}

// 正则化上不完全 Gamma Q(a,x)（连分式,x >= a+1 时用）
fn gammq_cf(a: f64, x: f64) -> f64 {
    let gln = gammln(a);
    let tiny = 1e-30;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..500 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    (-x + a * x.ln() - gln).exp() * h
}

// 卡方分布的“右尾 p 值”: P(X^2_df >= chi2)
// = Q(df/2, chi2/2)。p 越大越说明数据符合原假设(均匀分布)。
pub(crate) fn chi2_pvalue(chi2: f64, df: f64) -> f64 {
    let a = df / 2.0;
    let x = chi2 / 2.0;
    if x < a + 1.0 {
        1.0 - gammp_series(a, x)
    } else {
        gammq_cf(a, x)
    }
}

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

// 标准正态 CDF,用 erf 近似(Abramowitz-Stegun 7.1.26,误差 < 1.5e-7)
fn erf(x: f64) -> f64 {
    let s = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    s * y
}
pub(crate) fn normal_two_sided_p(z: f64) -> f64 {
    // 双尾 p 值 = 2 * (1 - Phi(|z|))
    let phi = 0.5 * (1.0 + erf(z.abs() / std::f64::consts::SQRT_2));
    2.0 * (1.0 - phi)
}

// ----------------------- 3. 彩种定义与概率 ---------------------------

struct Game {
    name: &'static str,
    rule: &'static str,
    // 头奖组合总数(=1/概率),用 f64 存储大数
    combos: f64,
    // 官方大致返奖率(投入 1 元期望回收多少元),公开区间约 0.50~0.59
    return_rate: f64,
}

fn games() -> Vec<Game> {
    vec![
        Game {
            name: "双色球",
            rule: "红球 6/33 + 蓝球 1/16",
            combos: comb(33, 6) * 16.0, // 17,721,088
            return_rate: 0.50,
        },
        Game {
            name: "超级大乐透",
            rule: "前区 5/35 + 后区 2/12",
            combos: comb(35, 5) * comb(12, 2), // 21,425,712
            return_rate: 0.50,
        },
        Game {
            name: "福彩3D",
            rule: "直选 3 位 (000-999)",
            combos: 1_000.0,
            return_rate: 0.52, // 2元中1040元 => 1040/1000/2*... 见下方精确演示
        },
        Game {
            name: "排列3",
            rule: "直选 3 位 (000-999)",
            combos: 1_000.0,
            return_rate: 0.52,
        },
        Game {
            name: "排列5",
            rule: "直选 5 位 (00000-99999)",
            combos: 100_000.0,
            return_rate: 0.50,
        },
        Game {
            name: "7星彩",
            rule: "前6位 0-9 + 末位 0-14",
            combos: 1_000_000.0 * 15.0, // 15,000,000
            return_rate: 0.50,
        },
        Game {
            name: "7乐彩",
            rule: "7/30",
            combos: comb(30, 7), // 2,035,800
            return_rate: 0.50,
        },
        Game {
            name: "快乐8",
            rule: "选10中10 (20/80 开出)",
            combos: comb(80, 10) / comb(20, 10), // 8,911,711
            return_rate: 0.59,
        },
    ]
}

fn print_probability_table() {
    println!("\n========== 1. 头奖精确概率(组合数学计算)==========");
    println!(
        "{:<12}{:<26}{:>18}{:>16}",
        "彩种", "玩法", "头奖组合数", "中奖概率"
    );
    println!("{}", "-".repeat(72));
    for g in games() {
        println!(
            "{:<12}{:<26}{:>18}{:>16}",
            g.name,
            g.rule,
            format_int(g.combos),
            format!("1/{}", format_int(g.combos))
        );
    }
    println!("\n结论:头奖概率完全由组合数决定,与选号方式、历史走势无关。");
}

// 千分位格式化
pub(crate) fn format_int(x: f64) -> String {
    let n = x.round() as u64;
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::new();
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

// ----------------------- 4. 期望值 / 返奖率 --------------------------

fn print_expected_value() {
    println!("\n========== 2. 期望值:为什么长期必亏 ==========");
    println!("期望值 EV = 返奖率 - 1  (每投入 1 元的净期望)\n");
    println!("{:<12}{:>12}{:>16}{:>18}", "彩种", "返奖率", "每元净期望", "投1万元期望亏损");
    println!("{}", "-".repeat(58));
    for g in games() {
        let ev = g.return_rate - 1.0;
        println!(
            "{:<12}{:>11.0}%{:>15.2}{:>16.0}元",
            g.name,
            g.return_rate * 100.0,
            ev,
            -ev * 10_000.0
        );
    }
    println!("\n每一种玩法的净期望都 < 0,且与所选号码无关。");
    println!("没有任何选号算法能把负期望变正期望(等价于'永动机')。");

    // 福彩3D 直选精确演示
    println!("\n【福彩3D 直选精确演示】");
    println!("  单注 2 元,直选中奖固定奖金 1040 元,概率 1/1000。");
    let ev = 1040.0 * (1.0 / 1000.0) - 2.0;
    println!("  EV = 1040 × 1/1000 − 2 = {:.2} 元/注 (即返奖率 {:.0}%)", ev, 1040.0 / 1000.0 / 2.0 * 100.0);
    println!("  => 每买一注,数学期望净亏 {:.2} 元。", -ev);
}

// ----------------- 5. 卡方拟合优度检验(均匀性)---------------------
// 模拟大量真随机开奖,统计双色球红球 1..33 各号出现频次,
// 用卡方检验判断"是否偏离均匀分布"。真随机 => p 值不显著 => 无法拒绝均匀。
fn chi_square_uniformity_demo(rng: &mut Rng) {
    println!("\n========== 3. 卡方拟合优度检验:号码是否均匀? ==========");
    let draws = 100_000u32; // 模拟 10 万期开奖
    let hi = 33u32;
    let pick = 6u32;
    let mut freq = vec![0u64; (hi + 1) as usize];
    for _ in 0..draws {
        for b in rng.sample(hi, pick) {
            freq[b as usize] += 1;
        }
    }
    // 每期抽 6 个,总抽取 = draws*6,均摊到 33 个号
    let total = (draws * pick) as f64;
    let expected = total / hi as f64;
    let chi2 = chi2_from_freq(&freq[1..=hi as usize], expected);
    let df = (hi - 1) as f64;
    let p = chi2_pvalue(chi2, df);
    println!("模拟期数: {}  (每期红球 6/33)", format_int(draws as f64));
    println!("各号理论期望频次: {:.1}", expected);
    println!("卡方统计量 χ² = {:.2}   自由度 df = {}", chi2, df as u32);
    println!("p 值 = {:.4}", p);
    if p > 0.05 {
        println!("=> p > 0.05,无法拒绝'均匀分布'原假设:号码确实均匀随机。");
    } else {
        println!("=> 本次样本偶然偏离(重跑仍应长期回归均匀)。");
    }
    // 展示最高/最低频号,说明"冷热差异"只是随机波动
    let mut idx: Vec<usize> = (1..=hi as usize).collect();
    idx.sort_by_key(|&i| freq[i]);
    println!(
        "最冷号 {:02}(出现 {} 次) vs 最热号 {:02}(出现 {} 次),差异仅 {:.2}%,纯属随机涨落。",
        idx[0],
        freq[idx[0]],
        idx[hi as usize - 1],
        freq[idx[hi as usize - 1]],
        (freq[idx[hi as usize - 1]] - freq[idx[0]]) as f64 / expected * 100.0
    );
}

// -------------- 6. 赌徒谬误证明:条件概率不变 -----------------------
// "某号已连续 N 期没出,下期是不是更容易出?" —— 用模拟验证:不会。
fn gamblers_fallacy_demo(rng: &mut Rng) {
    println!("\n========== 4. 赌徒谬误实证:'冷号该出了'是错的 ==========");
    let hi = 33u32;
    let pick = 6u32;
    let target = 7u32; // 观察 7 号红球
    let trials = 2_000_000u32;
    // 统计:在"上一期没出 target"的条件下,本期出 target 的频率
    let mut gap_hit = [0u64; 2]; // [没出后本期出, 没出后本期还没出]
    let mut prev_absent = false;
    let base_p = pick as f64 / hi as f64; // 无条件出现概率 6/33
    for _ in 0..trials {
        let drawn = rng.sample(hi, pick);
        let hit = drawn.contains(&target);
        if prev_absent {
            gap_hit[if hit { 0 } else { 1 }] += 1;
        }
        prev_absent = !hit;
    }
    let cond_p = gap_hit[0] as f64 / (gap_hit[0] + gap_hit[1]) as f64;
    println!("观察红球 {:02},模拟 {} 期。", target, format_int(trials as f64));
    println!("无条件出现概率 P(出) = 6/33 = {:.4}", base_p);
    println!("条件概率 P(本期出 | 上期没出) = {:.4}", cond_p);
    println!(
        "两者差异 = {:.4}(≈0)=> 历史遗漏对下期概率毫无影响,'冷号回补'是幻觉。",
        (cond_p - base_p).abs()
    );
}

// ----------------- 7. 游程检验(序列独立性)-------------------------
// Wald–Wolfowitz Runs Test:检验一个二值序列是否独立随机。
// 这里把每期"是否出现红球7"编码为 0/1 序列,检验其无自相关。
fn runs_test_demo(rng: &mut Rng) {
    println!("\n========== 5. 游程检验:开奖序列是否独立? ==========");
    let n = 50_000usize;
    let hi = 33u32;
    let pick = 6u32;
    let target = 7u32;
    let mut seq = Vec::with_capacity(n);
    for _ in 0..n {
        seq.push(rng.sample(hi, pick).contains(&target));
    }
    let n1 = seq.iter().filter(|&&b| b).count() as f64; // 出现次数
    let n0 = n as f64 - n1; // 未出现次数
    let (runs, mu, z) = runs_z(&seq);
    let p = normal_two_sided_p(z);
    println!("序列长度 {},出现={:.0} 未出现={:.0}", n, n1, n0);
    println!("实际游程数 R = {:.0},理论均值 μ = {:.1}", runs, mu);
    println!("Z = {:.3},双尾 p 值 = {:.4}", z, p);
    if p > 0.05 {
        println!("=> p > 0.05:序列通过独立性检验,前后期之间没有可利用的规律。");
    } else {
        println!("=> 偶然显著(重跑应回归)。");
    }
}

// --------------- 8. 蒙特卡洛长期投注模拟(资金曲线)-----------------
// 模拟坚持买福彩3D直选(每注2元,中奖1040元),看资金如何衰减。
fn monte_carlo_bankroll(rng: &mut Rng) {
    println!("\n========== 6. 蒙特卡洛长期投注模拟(福彩3D直选)==========");
    let bets = 2_000_000u32; // 买 200 万注,让回报率充分收敛
    let cost = 2.0;
    let prize = 1040.0;
    let mut bankroll = 0.0f64; // 累计盈亏
    let mut wins = 0u32;
    let checkpoints = [1_000u32, 10_000, 100_000, 500_000, 2_000_000];
    let mut ci = 0;
    println!("每注成本 {:.0} 元,直选中奖固定返 {:.0} 元,中奖概率 1/1000。\n", cost, prize);
    println!("{:>12}{:>10}{:>16}{:>14}", "已投注数", "中奖次数", "累计盈亏(元)", "回报率");
    println!("{}", "-".repeat(54));
    for i in 1..=bets {
        bankroll -= cost;
        // 抽一个 000-999,命中固定目标号(比如 123)才中奖
        if rng.below(1000) == 123 {
            bankroll += prize;
            wins += 1;
        }
        if ci < checkpoints.len() && i == checkpoints[ci] {
            let spent = i as f64 * cost;
            println!(
                "{:>12}{:>10}{:>16.0}{:>13.1}%",
                format_int(i as f64),
                wins,
                bankroll,
                (spent + bankroll) / spent * 100.0
            );
            ci += 1;
        }
    }
    let spent = bets as f64 * cost;
    println!(
        "\n最终:投入 {:.0} 元,收回 {:.0} 元,净亏 {:.0} 元,实际回报率 {:.1}%。",
        spent,
        spent + bankroll,
        -bankroll,
        (spent + bankroll) / spent * 100.0
    );
    println!("=> 随投注数增加,回报率稳定收敛到理论返奖率 ~52%(大数定律)。");
    println!("   投得越多、亏得越确定——这正是负期望博弈的本质。");
}

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║   彩票随机性验证 · 概率论实证工具 (Rust / 零依赖)        ║");
    println!("║   用数学证明:彩票不可预测,不存在可盈利策略。            ║");
    println!("╚════════════════════════════════════════════════════════╝");

    print_probability_table();
    print_expected_value();

    // 固定种子 => 可复现的科学实验
    let mut rng = Rng::new(20260703);
    chi_square_uniformity_demo(&mut rng);
    gamblers_fallacy_demo(&mut rng);
    runs_test_demo(&mut rng);
    monte_carlo_bankroll(&mut rng);

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

    println!("\n════════════════════ 总结 ════════════════════");
    println!("1. 头奖概率由组合数唯一决定,固定不变。");
    println!("2. 所有玩法期望值恒为负,长期必亏。");
    println!("3. 卡方检验:号码均匀分布,无冷热规律可用。");
    println!("4. 条件概率不变:'冷号该出了'是赌徒谬误。");
    println!("5. 游程检验:开奖序列独立,无自相关可预测。");
    println!("6. 蒙特卡洛:投得越多,回报率越确定地收敛到 ~50%。");
    println!("7. 真实历史数据(若已填充)与理论/模拟结论一致:同样均匀、独立、无策略优势。");
    println!("\n=> 理性结论:彩票是纯娱乐消费,不是投资。量力而行。");
}

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
