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

// 接口 JSON body -> (CSV 文本, 报告)。纯函数,不写文件。0 条有效记录返回 Err。
fn build_csv(spec: &GameSpec, body: &str) -> Result<(String, String), String> {
    let entries = parse_result_entries(body)?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut dropped = 0usize;
    for e in &entries {
        match map_entry(spec, e) {
            Ok(f) => rows.push(f),
            Err(_) => dropped += 1,
        }
    }
    if rows.is_empty() {
        return Err(format!("解析到 0 条有效记录(共 {} 条,均校验失败)。", entries.len()));
    }
    rows.sort_by(|a, b| {
        a[0].parse::<u64>().unwrap_or(0).cmp(&b[0].parse::<u64>().unwrap_or(0)).then(a[0].cmp(&b[0]))
    });
    let mut csv = String::new();
    csv.push_str(&format!("# {} {} 期  期号,日期,号码...\n", spec.name, rows.len()));
    for r in &rows {
        csv.push_str(&r.join(","));
        csv.push('\n');
    }
    let report = format!(
        "解析 {} 期(丢弃 {} 条),期号 {} → {}",
        rows.len(), dropped, rows[0][0], rows[rows.len() - 1][0]
    );
    Ok((csv, report))
}

// build_csv 后写入 spec.file。
pub(crate) fn process_and_write(spec: &GameSpec, body: &str) -> Result<String, String> {
    let (csv, report) = build_csv(spec, body)?;
    std::fs::write(spec.file, &csv).map_err(|e| format!("写入 {} 失败:{}", spec.file, e))?;
    Ok(format!("{}:{},已写入 {}", spec.name, report, spec.file))
}

pub(crate) fn print_usage() {
    println!("用法:");
    println!("  lottery_stats                    运行完整分析报告(读 data/*.csv)");
    println!("  lottery_stats fetch <彩种> [期数]   抓取并写入 data/<彩种>.csv(默认 100 期)");
    println!("  lottery_stats import <彩种> <文件>  从接口 JSON 文件导入并写入 data/<彩种>.csv");
    println!("  lottery_stats help               显示本说明");
    println!("  lottery_stats serve [端口]        启动本地网页(默认 8080),浏览器访问分析页面");
    println!("支持抓取的彩种:ssq(双色球) d3(福彩3D) kl8(快乐8) qlc(7乐彩)");
}

// 调用系统 curl 抓取 URL 内容。
fn curl_get(url: &str) -> Result<String, String> {
    let out = std::process::Command::new("curl")
        .args([
            "-s", "--max-time", "20",
            "-H", "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
            "-H", "Referer: https://www.cwl.gov.cn/",
            url,
        ])
        .output()
        .map_err(|e| format!("无法执行 curl(请确认系统已安装 curl):{}", e))?;
    if !out.status.success() {
        return Err(format!("curl 退出码非 0:{}", String::from_utf8_lossy(&out.stderr)));
    }
    let body = String::from_utf8_lossy(&out.stdout).to_string();
    if body.trim().is_empty() {
        return Err("curl 返回空响应(可能被限流或网络不通)".to_string());
    }
    Ok(body)
}

pub(crate) fn run_fetch(args: &[String]) {
    match do_fetch(args) {
        Ok(msg) => println!("{}", msg),
        Err(e) => eprintln!("抓取失败:{}", e),
    }
}

pub(crate) fn do_fetch(args: &[String]) -> Result<String, String> {
    let key = args.get(0).ok_or_else(|| "缺少彩种参数,用法见 `help`。".to_string())?;
    let count: u32 = match args.get(1) {
        Some(s) => s.parse().map_err(|_| format!("期数 '{}' 非法(应为正整数)", s))?,
        None => 100,
    };
    let spec = real_data_games()
        .into_iter()
        .find(|g| &g.key == key)
        .ok_or_else(|| format!("未知彩种 '{}'。支持:ssq/d3/kl8/qlc/dlt/pl3/pl5/qxc", key))?;
    let src = spec
        .fetch
        .as_ref()
        .ok_or_else(|| format!("{} 暂不支持抓取(仅福彩 ssq/3d/kl8/qlc 支持)", spec.name))?;
    let url = build_url(src.name_param, count);
    let body = curl_get(&url)?;
    process_and_write(&spec, &body)
}

pub(crate) fn run_import(args: &[String]) {
    match do_import(args) {
        Ok(msg) => println!("{}", msg),
        Err(e) => eprintln!("导入失败:{}", e),
    }
}

pub(crate) fn do_import(args: &[String]) -> Result<String, String> {
    let key = args.get(0).ok_or_else(|| "缺少彩种参数,用法见 `help`。".to_string())?;
    let path = args
        .get(1)
        .ok_or_else(|| "缺少文件路径,用法:import <彩种> <文件>".to_string())?;
    let spec = real_data_games()
        .into_iter()
        .find(|g| &g.key == key)
        .ok_or_else(|| format!("未知彩种 '{}'。支持:ssq/d3/kl8/qlc", key))?;
    spec.fetch
        .as_ref()
        .ok_or_else(|| format!("{} 非福彩 JSON 格式,import 仅支持 ssq/d3/kl8/qlc", spec.name))?;
    let body = std::fs::read_to_string(path)
        .map_err(|e| format!("无法读取文件 '{}':{}", path, e))?;
    process_and_write(&spec, &body)
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

    #[test]
    fn do_fetch_unknown_key_errs() {
        assert!(do_fetch(&["xyz".to_string()]).is_err());
    }

    #[test]
    fn do_fetch_unsupported_game_errs() {
        // dlt 无 fetch 源,应在触网前报错
        assert!(do_fetch(&["dlt".to_string()]).is_err());
    }

    #[test]
    fn do_fetch_bad_count_errs() {
        assert!(do_fetch(&["ssq".to_string(), "abc".to_string()]).is_err());
    }

    #[test]
    fn build_csv_sorts_and_formats() {
        let ssq = real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        let (csv, report) = build_csv(&ssq, SAMPLE).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines[0].starts_with('#')); // 头注释
        // 升序:2024001 在 2024002 之前
        assert_eq!(lines[1], "2024001,2024-01-02,01,07,15,22,28,33,09");
        assert_eq!(lines[2], "2024002,2024-01-04,03,05,11,19,26,31,02");
        assert!(report.contains("解析 2 期"));
    }

    #[test]
    fn build_csv_rejects_bad_state() {
        let ssq = real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        assert!(build_csv(&ssq, r#"{"state":1,"message":"x"}"#).is_err());
    }

    #[test]
    fn build_csv_rejects_all_invalid() {
        let ssq = real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        // state 成功但记录号码不足 => 0 条有效 => Err
        let json = r#"{"state":0,"result":[{"code":"1","date":"2024-01-01","red":"01,02","blue":""}]}"#;
        assert!(build_csv(&ssq, json).is_err());
    }

    #[test]
    fn do_import_unknown_key_errs() {
        assert!(do_import(&["xyz".to_string(), "f.json".to_string()]).is_err());
    }

    #[test]
    fn do_import_unsupported_game_errs() {
        // dlt 无 fetch 源 => 在读文件前报错
        assert!(do_import(&["dlt".to_string(), "f.json".to_string()]).is_err());
    }

    #[test]
    fn do_import_missing_args_errs() {
        // 只给 key,缺文件路径
        assert!(do_import(&["ssq".to_string()]).is_err());
    }

    #[test]
    fn do_import_missing_file_errs() {
        // ssq 合法且有源,但文件不存在 => 读文件失败,且不会写 data/ssq.csv
        assert!(do_import(&["ssq".to_string(), "C:/no/such/dir/nope-xyz-123.json".to_string()]).is_err());
    }
}
