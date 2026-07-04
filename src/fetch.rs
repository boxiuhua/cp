// 命令行抓取:调用系统 curl 拉取中国福彩网开奖数据,转换为项目 CSV。
// 这是唯一联网、且依赖运行时 curl 的模块;分析引擎保持离线。

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
        Some(s) => return Err(format!("接口返回 state={}(非成功):{}", s, &json[..json.len().min(200)])),
        None => return Err(format!("响应无法识别(缺 state):{}", &json[..json.len().min(200)])),
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
}
