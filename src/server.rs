// 零依赖 HTTP 服务器与页面。分析引擎保持离线;仅本机访问。

use crate::realdata::GameAnalysis;

pub(crate) fn jesc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

pub(crate) fn num(x: f64) -> String {
    if x.is_finite() { format!("{:.6}", x) } else { "null".to_string() }
}

pub(crate) fn analysis_to_json(a: &GameAnalysis) -> String {
    let uni: Vec<String> = a.uniformity.iter().map(|r| {
        let extra = match &r.extra {
            Some((cold, cn, hot, hn, sd)) => format!(
                ",\"cold\":{},\"coldN\":{},\"hot\":{},\"hotN\":{},\"sd\":{}", cold, cn, hot, hn, num(*sd)),
            None => String::new(),
        };
        format!(
            "{{\"label\":\"{}\",\"expected\":{},\"chi2\":{},\"df\":{},\"p\":{},\"uniform\":{}{}}}",
            jesc(&r.label), num(r.expected), num(r.chi2), r.df, num(r.p), r.uniform, extra)
    }).collect();
    let gam: Vec<String> = a.gambler.iter().map(|r| format!(
        "{{\"label\":\"{}\",\"baseP\":{},\"condP\":{},\"samples\":{},\"diff\":{},\"enough\":{}}}",
        jesc(&r.label), num(r.base_p), num(r.cond_p), r.samples, num(r.diff), r.enough)).collect();
    let run: Vec<String> = a.runs.iter().map(|r| format!(
        "{{\"label\":\"{}\",\"appear\":{},\"runs\":{},\"mu\":{},\"z\":{},\"p\":{},\"independent\":{}}}",
        jesc(&r.label), r.appear, num(r.runs), num(r.mu), num(r.z), num(r.p), r.independent)).collect();
    let pred: Vec<String> = a.pred.iter().map(|p| format!(
        "{{\"label\":\"{}\",\"cold\":{},\"hot\":{},\"random\":{},\"expected\":{}}}",
        jesc(&p.label), num(p.cold), num(p.hot), num(p.random), num(p.expected))).collect();
    let latest: Vec<String> = a.coverage.latest.iter().map(|seg| {
        let nums: Vec<String> = seg.iter().map(|n| n.to_string()).collect();
        format!("[{}]", nums.join(","))
    }).collect();
    format!(
        "{{\"available\":true,\"coverage\":{{\"firstIssue\":\"{}\",\"firstDate\":\"{}\",\"lastIssue\":\"{}\",\"lastDate\":\"{}\",\"count\":{},\"latest\":[{}]}},\"uniformity\":[{}],\"gambler\":[{}],\"runs\":[{}],\"predN\":{},\"pred\":[{}]}}",
        jesc(&a.coverage.first_issue), jesc(&a.coverage.first_date),
        jesc(&a.coverage.last_issue), jesc(&a.coverage.last_date),
        a.coverage.count, latest.join(","),
        uni.join(","), gam.join(","), run.join(","), a.pred_n, pred.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jesc_escapes() {
        assert_eq!(jesc("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(jesc("line\n"), "line\\n");
    }

    #[test]
    fn num_formats() {
        assert_eq!(num(1.0), "1.000000");
        assert_eq!(num(f64::NAN), "null");
        assert_eq!(num(f64::INFINITY), "null");
    }

    #[test]
    fn analysis_json_has_sections() {
        let ssq = crate::game_spec::real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        let draws = crate::realdata::load_game(&ssq).unwrap().0; // 占位 data/ssq.csv(5 期)
        let mut rng = crate::Rng::new(1);
        let a = crate::realdata::analyze_game(&ssq, &draws, &mut rng);
        let j = analysis_to_json(&a);
        assert!(j.contains("\"available\":true"));
        assert!(j.contains("\"uniformity\":["));
        assert!(j.contains("\"gambler\":["));
        assert!(j.contains("\"runs\":["));
        assert!(j.contains("\"pred\":["));
        assert!(j.contains("\"coverage\":"));
    }
}
