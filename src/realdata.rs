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

pub(crate) struct UniformRow {
    pub label: String,
    pub expected: f64,
    pub chi2: f64,
    pub df: u32,
    pub p: f64,
    pub uniform: bool,
    pub extra: Option<(u32, u64, u32, u64, f64)>, // (cold, coldN, hot, hotN, sd) 仅池型
}
pub(crate) struct GamblerRow {
    pub label: String, pub base_p: f64, pub cond_p: f64, pub samples: u64, pub diff: f64, pub enough: bool,
}
pub(crate) struct RunsRow {
    pub label: String, pub appear: u64, pub runs: f64, pub mu: f64, pub z: f64, pub p: f64, pub independent: bool,
}
pub(crate) struct Coverage {
    pub first_issue: String, pub first_date: String, pub last_issue: String, pub last_date: String,
    pub count: usize, pub latest: Vec<Vec<u32>>,
}
pub(crate) struct GameAnalysis {
    pub coverage: Coverage,
    pub uniformity: Vec<UniformRow>,
    pub gambler: Vec<GamblerRow>,
    pub runs: Vec<RunsRow>,
    pub pred_n: usize,
    pub pred: Vec<CompPred>,
    pub picks: Vec<StrategyPick>,
}

pub(crate) fn compute_uniformity(spec: &GameSpec, draws: &[DrawRecord]) -> Vec<UniformRow> {
    let mut rows = Vec::new();
    for (ci, comp) in spec.components.iter().enumerate() {
        match comp {
            Component::Pool { label, size, pick } => {
                let counts = pool_counts(draws, ci, *size);
                let expected = draws.len() as f64 * *pick as f64 / *size as f64;
                let chi2 = crate::chi2_from_freq(&counts[1..=*size as usize], expected);
                let df = *size - 1;
                let p = crate::chi2_pvalue(chi2, df as f64);
                let sd = expected.sqrt();
                let mut idx: Vec<usize> = (1..=*size as usize).collect();
                idx.sort_by_key(|&i| counts[i]);
                let (cold, hot) = (idx[0], idx[*size as usize - 1]);
                rows.push(UniformRow {
                    label: format!("{} {}/{}", label, pick, size),
                    expected, chi2, df, p, uniform: p > 0.05,
                    extra: Some((cold as u32, counts[cold], hot as u32, counts[hot], sd)),
                });
            }
            Component::Digits { label, bases } => {
                for (pos, &base) in bases.iter().enumerate() {
                    let counts = digit_counts(draws, ci, pos, base);
                    let expected = draws.len() as f64 / base as f64;
                    let chi2 = crate::chi2_from_freq(&counts, expected);
                    let df = base - 1;
                    let p = crate::chi2_pvalue(chi2, df as f64);
                    rows.push(UniformRow {
                        label: format!("{} 第{}位 0-{}", label, pos + 1, base - 1),
                        expected, chi2, df, p, uniform: p > 0.05, extra: None,
                    });
                }
            }
        }
    }
    rows
}

pub(crate) fn format_uniformity(rows: &[UniformRow]) -> String {
    let mut s = String::from("\n-- [真实] 卡方均匀性检验 --\n");
    for r in rows {
        s.push_str(&format!(
            "[{}] 期望频次 {:.1}  χ²={:.2} df={} p={:.4}  =>{}\n",
            r.label, r.expected, r.chi2, r.df, r.p,
            if r.uniform { "均匀" } else { "本样本偏离(小样本功效低)" }
        ));
        if let Some((cold, cn, hot, hn, sd)) = r.extra {
            s.push_str(&format!(
                "  最冷 {:02}({}次) vs 最热 {:02}({}次),差 ≈{:.1}σ,属随机涨落。\n",
                cold, cn, hot, hn, (hn - cn) as f64 / sd.max(1e-9)
            ));
        }
    }
    s
}

pub(crate) fn compute_gamblers(spec: &GameSpec, draws: &[DrawRecord]) -> Vec<GamblerRow> {
    let mut rows = Vec::new();
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
            rows.push(GamblerRow { label, base_p: base, cond_p: 0.0, samples: 0, diff: 0.0, enough: false });
        } else {
            let cond = gap_hit as f64 / denom as f64;
            rows.push(GamblerRow { label, base_p: base, cond_p: cond, samples: denom, diff: (cond - base).abs(), enough: true });
        }
    }
    rows
}

pub(crate) fn format_gamblers(rows: &[GamblerRow]) -> String {
    let mut s = String::from("\n-- [真实] 赌徒谬误检验 --\n");
    for r in rows {
        if !r.enough {
            s.push_str(&format!("[{}] 样本不足,跳过。\n", r.label));
        } else {
            s.push_str(&format!(
                "[{}] 无条件 P={:.4}  条件 P(出|上期没出)={:.4}(样本{}次)  差={:.4} =>历史遗漏无影响。\n",
                r.label, r.base_p, r.cond_p, r.samples, r.diff
            ));
        }
    }
    s
}

pub(crate) fn compute_runs(spec: &GameSpec, draws: &[DrawRecord]) -> Vec<RunsRow> {
    let mut rows = Vec::new();
    for (ci, comp) in spec.components.iter().enumerate() {
        let target = default_target(comp);
        let seq: Vec<bool> = draws.iter().map(|d| target_hit(d, ci, &target)).collect();
        let (runs, mu, z) = crate::runs_z(&seq);
        let p = crate::normal_two_sided_p(z);
        rows.push(RunsRow {
            label: describe_target(comp, &target),
            appear: seq.iter().filter(|&&b| b).count() as u64,
            runs, mu, z, p, independent: p > 0.05,
        });
    }
    rows
}

pub(crate) fn format_runs(rows: &[RunsRow]) -> String {
    let mut s = String::from("\n-- [真实] 游程检验 --\n");
    for r in rows {
        s.push_str(&format!(
            "[{}] 出现 {} 次  R={:.0} μ={:.1} Z={:.3} 双尾p={:.4} =>{}\n",
            r.label, r.appear, r.runs, r.mu, r.z, r.p,
            if r.independent { "序列独立" } else { "偶然显著(小样本)" }
        ));
    }
    s
}

pub(crate) fn compute_coverage(draws: &[DrawRecord]) -> Coverage {
    let f = &draws[0];
    let l = &draws[draws.len() - 1];
    Coverage {
        first_issue: f.issue.clone(), first_date: f.date.clone(),
        last_issue: l.issue.clone(), last_date: l.date.clone(),
        count: draws.len(), latest: l.components.clone(),
    }
}

pub(crate) struct StrategyPick {
    pub strategy: String,
    pub why: String,
    pub ticket: Vec<Vec<u32>>, // 每组件一段号码
}

// 按冷/热/机选三策略各生成一注号码(仅演示,概率并不更优)。
pub(crate) fn strategy_picks(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng) -> Vec<StrategyPick> {
    // (名称, 为什么, 模式) 模式:0=冷 1=热 2=机选
    let modes: [(&str, &str, u8); 3] = [
        ("冷号", "赌'冷号该回补了'——但历史遗漏不改变下期概率,无效。", 0),
        ("热号", "赌'热号继续走强'——但开奖独立同分布,无效。", 1),
        ("机选", "完全不依赖历史,随机一注。", 2),
    ];
    let mut out = Vec::with_capacity(3);
    for (name, why, mode) in modes {
        let mut ticket: Vec<Vec<u32>> = Vec::with_capacity(spec.components.len());
        for (ci, comp) in spec.components.iter().enumerate() {
            match comp {
                Component::Pool { size, pick, .. } => {
                    let seg = match mode {
                        0 => { let wc = pool_counts(draws, ci, *size); pick_pool(&wc, *size, *pick, false) }
                        1 => { let wc = pool_counts(draws, ci, *size); pick_pool(&wc, *size, *pick, true) }
                        _ => rng.sample(*size, *pick),
                    };
                    ticket.push(seg);
                }
                Component::Digits { bases, .. } => {
                    let mut seg = Vec::with_capacity(bases.len());
                    for (pos, &base) in bases.iter().enumerate() {
                        let d = match mode {
                            0 => { let wc = digit_counts(draws, ci, pos, base); pick_digit(&wc, false) }
                            1 => { let wc = digit_counts(draws, ci, pos, base); pick_digit(&wc, true) }
                            _ => rng.below(base as u64) as u32,
                        };
                        seg.push(d);
                    }
                    ticket.push(seg);
                }
            }
        }
        out.push(StrategyPick { strategy: name.to_string(), why: why.to_string(), ticket });
    }
    out
}

pub(crate) fn analyze_game(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng) -> GameAnalysis {
    let (pred_n, pred) = prediction_stats(spec, draws, 30, rng);
    let picks = strategy_picks(spec, draws, rng);
    GameAnalysis {
        coverage: compute_coverage(draws),
        uniformity: compute_uniformity(spec, draws),
        gambler: compute_gamblers(spec, draws),
        runs: compute_runs(spec, draws),
        pred_n, pred, picks,
    }
}

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

use crate::game_spec::real_data_games;

// 单彩种报告:覆盖行 + 四项分析 + 预测实验。
pub(crate) fn run_game_report(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng) {
    let a = analyze_game(spec, draws, rng);
    let c = &a.coverage;
    println!(
        "数据覆盖:{}({}) → {}({}),共 {} 期。最新一期号码 {:?}",
        c.first_issue, c.first_date, c.last_issue, c.last_date, c.count, c.latest
    );
    print!("{}", format_uniformity(&a.uniformity));
    print!("{}", format_gamblers(&a.gambler));
    print!("{}", format_runs(&a.runs));
    print_prediction(a.pred_n, &a.pred);
}

// 第 7 章总编排:遍历 8 种彩票,有数据文件的跑完整分析,缺失则一行跳过。
pub(crate) fn run_all_real_data(rng: &mut crate::Rng) {
    println!("\n========== 7. 真实历史数据篇(全彩种)==========");
    for spec in real_data_games() {
        match load_game(&spec) {
            Ok((draws, skips)) => {
                println!(
                    "\n【{}·{}】{}:解析 {} 期,跳过 {} 行。",
                    spec.name, spec.key, spec.file, draws.len(), skips.len()
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
                println!("\n【{}·{}】未找到 {},跳过(填入真实数据即可启用)。", spec.name, spec.key, spec.file);
            }
        }
    }
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

    #[test]
    fn strategy_picks_hot_cold_random() {
        let ssq = real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        let rows: Vec<DrawRecord> = (0..10).map(|_| rec(vec![vec![1,2,3,4,5,6], vec![1]])).collect();
        let mut rng = crate::Rng::new(1);
        let picks = strategy_picks(&ssq, &rows, &mut rng);
        assert_eq!(picks.len(), 3);
        assert_eq!(picks[0].strategy, "冷号");
        assert_eq!(picks[1].strategy, "热号");
        assert_eq!(picks[2].strategy, "机选");
        // 每注含 2 组件(红+蓝)
        assert_eq!(picks[0].ticket.len(), 2);
        // 热号红段=最热的 6 个 = [1,2,3,4,5,6];冷号红段=最冷的 6 个(计数0,按号升序)= [7..12]
        assert_eq!(picks[1].ticket[0], vec![1,2,3,4,5,6]);
        assert_eq!(picks[0].ticket[0], vec![7,8,9,10,11,12]);
        // 机选红段:6 个互异、均 ∈ [1,33]
        let r = &picks[2].ticket[0];
        assert_eq!(r.len(), 6);
        assert!(r.iter().all(|&n| (1..=33).contains(&n)));
    }

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

    #[test]
    fn compute_and_format_uniformity_pool() {
        let ssq = real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        // 两期,红球都 [1,2,3,4,5,6],蓝球 [1]/[2]
        let draws = vec![rec(vec![vec![1,2,3,4,5,6], vec![1]]),
                         rec(vec![vec![1,2,3,4,5,6], vec![2]])];
        let rows = compute_uniformity(&ssq, &draws);
        assert_eq!(rows.len(), 2); // 红球 + 蓝球
        assert!(rows[0].label.starts_with("红球 6/33"));
        assert!(rows[0].extra.is_some()); // 池型带冷热
        let s = format_uniformity(&rows);
        assert!(s.starts_with("\n-- [真实] 卡方均匀性检验 --\n"));
        assert!(s.contains("[红球 6/33] 期望频次"));
    }

    #[test]
    fn compute_runs_and_gamblers_shape() {
        let ssq = real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        let draws = vec![rec(vec![vec![1,2,3,4,5,7], vec![1]]),
                         rec(vec![vec![1,2,3,4,5,6], vec![2]]),
                         rec(vec![vec![1,2,3,4,5,7], vec![3]])];
        assert_eq!(compute_gamblers(&ssq, &draws).len(), 2);
        assert_eq!(compute_runs(&ssq, &draws).len(), 2);
        let cov = compute_coverage(&draws);
        assert_eq!(cov.count, 3);
        assert_eq!(cov.latest.len(), 2); // 两个组件
    }
}
