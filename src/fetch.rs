// 命令行抓取:调用系统 curl 拉取中国福彩网开奖数据,转换为项目 CSV。
// 这是唯一联网、且依赖运行时 curl 的模块;分析引擎保持离线。

use crate::game_spec::{real_data_games, GameSpec};

pub(crate) struct Entry {
    pub code: String,
    pub date: String,
    pub red: String,
    pub blue: String,
}

// 去掉日期里的星期后缀:"2024-01-02(二)" -> "2024-01-02"
pub(crate) fn clean_date(s: &str) -> String {
    match s.find('(') {
        Some(i) => s[..i].trim().to_string(),
        None => s.trim().to_string(),
    }
}

// 安全截取前 max 字节(回退到最近的字符边界),用于错误信息里的响应片段。
fn head(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// 读取顶层 "state" 整数值。
fn read_state(json: &str) -> Option<i64> {
    let i = json.find("\"state\"")?;
    let rest = &json[i + "\"state\"".len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let end = after
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(after.len());
    after[..end].parse().ok()
}

// 在窗口内取 "key":"value" 的 value。
fn field(window: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\":\"", key);
    let i = window.find(&pat)?;
    let rest = &window[i + pat.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// 从接口 JSON 提取每期的 code/date/red/blue。
pub(crate) fn parse_result_entries(json: &str) -> Result<Vec<Entry>, String> {
    match read_state(json) {
        Some(0) => {}
        Some(s) => return Err(format!("接口返回 state={}(非成功):{}", s, head(json, 200))),
        None => return Err(format!("响应无法识别(缺 state):{}", head(json, 200))),
    }
    let code_pat = "\"code\":\"";
    let starts: Vec<usize> = json.match_indices(code_pat).map(|(i, _)| i).collect();
    if starts.is_empty() {
        return Err("未找到任何开奖记录(result 为空?)".to_string());
    }
    let mut entries = Vec::with_capacity(starts.len());
    for (n, &s) in starts.iter().enumerate() {
        let end = if n + 1 < starts.len() { starts[n + 1] } else { json.len() };
        let window = &json[s..end];
        let code = field(window, "code").ok_or("记录缺 code 字段")?;
        let date = field(window, "date").unwrap_or_default();
        let red = field(window, "red").unwrap_or_default();
        let blue = field(window, "blue").unwrap_or_default();
        entries.push(Entry { code, date: clean_date(&date), red, blue });
    }
    Ok(entries)
}

pub(crate) fn build_url(name_param: &str, count: u32) -> String {
    format!(
        "https://www.cwl.gov.cn/cwl_admin/front/cwlkj/search/kjxx/findDrawNotice?name={}&issueCount={}",
        name_param, count
    )
}

// 把一期接口记录映射成 CSV 字段 [期号, 日期, 号码...],并用 parse_record 校验。
pub(crate) fn map_entry(spec: &GameSpec, e: &Entry) -> Result<Vec<String>, String> {
    let width = spec.field_count() - 2;
    let mut nums: Vec<String> = Vec::new();
    for tok in e.red.split(',').chain(e.blue.split(',')) {
        let t = tok.trim();
        if !t.is_empty() {
            nums.push(t.to_string());
        }
    }
    if nums.len() < width {
        return Err(format!("{} 期号 {}:号码数不足({} < {})", spec.name, e.code, nums.len(), width));
    }
    nums.truncate(width);
    let mut fields = vec![e.code.clone(), e.date.clone()];
    fields.extend(nums);
    let refs: Vec<&str> = fields.iter().map(|s| s.as_str()).collect();
    crate::realdata::parse_record(spec, &refs)
        .map_err(|err| format!("{} 期号 {}:{}", spec.name, e.code, err))?;
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"state":0,"message":"","result":[
{"code":"2024002","date":"2024-01-04(四)","red":"03,05,11,19,26,31","blue":"02","sales":"1"},
{"code":"2024001","date":"2024-01-02(二)","red":"01,07,15,22,28,33","blue":"09","sales":"1"}
]}"#;

    #[test]
    fn clean_date_strips_weekday() {
        assert_eq!(clean_date("2024-01-02(二)"), "2024-01-02");
        assert_eq!(clean_date("2024-01-02"), "2024-01-02");
    }

    #[test]
    fn parses_entries() {
        let e = parse_result_entries(SAMPLE).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].code, "2024002");
        assert_eq!(e[0].date, "2024-01-04"); // 星期后缀已去除
        assert_eq!(e[0].red, "03,05,11,19,26,31");
        assert_eq!(e[1].blue, "09");
    }

    #[test]
    fn rejects_non_success_state() {
        assert!(parse_result_entries(r#"{"state":1,"message":"限流"}"#).is_err());
    }

    #[test]
    fn rejects_empty_result() {
        assert!(parse_result_entries(r#"{"state":0,"result":[]}"#).is_err());
    }

    #[test]
    fn error_snippet_is_char_boundary_safe() {
        // 构造一个 state!=0 且在第 200 字节附近有多字节中文的响应,确保不 panic
        let mut s = String::from("{\"state\":1,\"message\":\"");
        while s.len() < 199 {
            s.push('x');
        }
        s.push('限'); // 多字节字符跨越第 200 字节
        s.push_str("\"}");
        assert!(parse_result_entries(&s).is_err()); // 应返回 Err,不得 panic
    }

    fn game(k: &str) -> crate::game_spec::GameSpec {
        real_data_games().into_iter().find(|g| g.key == k).unwrap()
    }
    fn entry(code: &str, red: &str, blue: &str) -> Entry {
        Entry { code: code.into(), date: "2024-01-02".into(), red: red.into(), blue: blue.into() }
    }

    #[test]
    fn url_format() {
        assert_eq!(
            build_url("ssq", 100),
            "https://www.cwl.gov.cn/cwl_admin/front/cwlkj/search/kjxx/findDrawNotice?name=ssq&issueCount=100"
        );
    }

    #[test]
    fn map_ssq_appends_blue() {
        let f = map_entry(&game("ssq"), &entry("2024001", "01,07,15,22,28,33", "09")).unwrap();
        assert_eq!(f, vec!["2024001", "2024-01-02", "01", "07", "15", "22", "28", "33", "09"]);
    }

    #[test]
    fn map_qlc_drops_special_ball() {
        let f = map_entry(&game("qlc"), &entry("1", "01,05,11,17,22,26,29", "30")).unwrap();
        assert_eq!(f.len(), 9); // 期号+日期+7 号
        assert!(!f[2..].contains(&"30".to_string())); // 特别号被丢弃
    }

    #[test]
    fn map_3d_keeps_positions() {
        let f = map_entry(&game("d3"), &entry("1", "3,8,1", "")).unwrap();
        assert_eq!(f, vec!["1", "2024-01-02", "3", "8", "1"]);
    }

    #[test]
    fn map_kl8_twenty_numbers() {
        let red = (1..=20).map(|n| format!("{:02}", n)).collect::<Vec<_>>().join(",");
        let f = map_entry(&game("kl8"), &entry("1", &red, "")).unwrap();
        assert_eq!(f.len(), 22); // 期号+日期+20 号
    }

    #[test]
    fn map_rejects_insufficient_numbers() {
        assert!(map_entry(&game("ssq"), &entry("1", "01,02,03", "")).is_err());
    }
}
