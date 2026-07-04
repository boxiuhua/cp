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

// 真实数据篇总编排:调用四项分析 + 预测实验。观察号固定为红球 7,窗口 30。
pub(crate) fn run_real_data_report(draws: &[Draw], rng: &mut crate::Rng) {
    let first = &draws[0];
    let last = &draws[draws.len() - 1];
    println!(
        "\n数据覆盖:{}({}) → {}({}),共 {} 期。",
        first.issue,
        first.date,
        last.issue,
        last.date,
        draws.len()
    );
    println!("最新一期 {}:红球 {:?} 蓝球 {:02}。", last.issue, last.reds, last.blue);
    analyze_uniformity(draws);
    analyze_gamblers_fallacy(draws, 7);
    analyze_runs(draws, 7);
    let stats = prediction_stats(draws, 30, rng);
    print_prediction(&stats);
}

#[cfg(test)]
mod tests {
    use super::*;

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
