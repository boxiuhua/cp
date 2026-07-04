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
