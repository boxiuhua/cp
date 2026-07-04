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
}
