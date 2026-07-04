# 抓取命令(fetch)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增命令行子命令 `fetch <彩种> [期数]`,用系统 curl 从中国福彩网接口抓取福彩 4 种(ssq/3d/kl8/qlc)真实开奖数据,转换成项目 CSV 写入 `data/<key>.csv`。

**Architecture:** 新增 `src/fetch.rs` 承载抓取全流程(URL、curl、极简 JSON 解析、列映射、写文件);`GameSpec` 加可选 `fetch` 源;`main.rs` 参数分派(无参数=原分析报告)。抓取与分析通过 CSV 文件解耦,分析引擎保持离线。

**Tech Stack:** Rust 2021,Cargo 零外部依赖(仅 std);运行时依赖系统 `curl`;`std::process::Command` 调 curl,`std::fs` 写文件。

## Global Constraints

- **Cargo 依赖为空**:`Cargo.toml` 的 `[dependencies]` 保持空;curl 是运行时外部程序,非 Cargo 依赖。不引入任何 JSON/HTTP crate。
- **无参数行为不变**:`cargo run --release`(无参数)仍运行第 1-7 章分析报告。
- **仅福彩 4 种可抓**:ssq/3d/kl8/qlc 配 `FetchSource`;dlt/pl3/pl5/qxc 为 `None`,抓取时报"暂不支持"。
- **接口 URL(逐字)**:`https://www.cwl.gov.cn/cwl_admin/front/cwlkj/search/kjxx/findDrawNotice?name={name}&issueCount={count}`,`name` ∈ {ssq,3d,kl8,qlc}。
- **curl 请求头(逐字)**:`User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)` 与 `Referer: https://www.cwl.gov.cn/`,并加 `-s --max-time 20`。
- **列映射规则**:`red` 与 `blue` 按逗号拆号拼接后取前 `width` 个(width = `field_count()-2`);映射结果必须通过 `realdata::parse_record` 校验才收下。
- **不 panic**:所有错误路径返回 `Result::Err` 并打印清晰中文提示。
- **测试离线**:单元测试不得触网;联网部分(curl_get、端到端)靠手动验证。
- **命令**:`cargo test`、`cargo run --release`、`cargo run --release -- fetch ssq`。
- **Git**:仓库已初始化;每任务末尾 commit,提交信息 body 末行:`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`。

---

### Task 1: `GameSpec` 增加抓取源

**Files:**
- Modify: `src/game_spec.rs`

**Interfaces:**
- Produces:
  - `pub(crate) struct FetchSource { pub name_param: &'static str }`
  - `GameSpec` 新增字段 `pub fetch: Option<FetchSource>`
  - `real_data_games()` 中 ssq/3d/kl8/qlc 填 `Some(FetchSource{...})`,其余 `None`。

- [ ] **Step 1: 写失败测试**（加到 `src/game_spec.rs` 的 `mod tests`)

```rust
    #[test]
    fn fetch_sources_configured() {
        let g = real_data_games();
        let src = |k: &str| {
            g.iter().find(|x| x.key == k).unwrap().fetch.as_ref().map(|s| s.name_param)
        };
        assert_eq!(src("ssq"), Some("ssq"));
        assert_eq!(src("3d"), Some("3d"));
        assert_eq!(src("kl8"), Some("kl8"));
        assert_eq!(src("qlc"), Some("qlc"));
        assert_eq!(src("dlt"), None);
        assert_eq!(src("pl3"), None);
        assert_eq!(src("pl5"), None);
        assert_eq!(src("qxc"), None);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `no field 'fetch' on type ... GameSpec` / `cannot find ... FetchSource`.

- [ ] **Step 3: 实现**

在 `src/game_spec.rs` 中,`Component` 定义之后加:

```rust
// 该彩种的抓取源(福彩接口的 name 参数)。None 表示暂不支持抓取。
pub(crate) struct FetchSource {
    pub name_param: &'static str,
}
```

在 `GameSpec` 结构体加字段:

```rust
    pub fetch: Option<FetchSource>,
```

在 `real_data_games()` 里,给每个 `GameSpec { ... }` 补上 `fetch` 字段:
- ssq → `fetch: Some(FetchSource { name_param: "ssq" }),`
- dlt → `fetch: None,`
- d3 → `fetch: Some(FetchSource { name_param: "3d" }),`
- pl3 → `fetch: None,`
- pl5 → `fetch: None,`
- qxc → `fetch: None,`
- qlc → `fetch: Some(FetchSource { name_param: "qlc" }),`
- kl8 → `fetch: Some(FetchSource { name_param: "kl8" }),`

（字段加在 `components: vec![...]` 之后即可。)

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: `fetch_sources_configured` 与既有测试全部 PASS。dead-code 警告(fetch 尚未被读)可接受。

- [ ] **Step 5: Commit**

```bash
git add src/game_spec.rs
git commit -m "feat: add optional FetchSource to GameSpec for welfare-lottery games"
```

---

### Task 2: `src/fetch.rs` — JSON 解析

**Files:**
- Create: `src/fetch.rs`
- Modify: `src/main.rs`(加 `mod fetch;`)

**Interfaces:**
- Consumes: `crate::game_spec::{GameSpec, real_data_games}`.
- Produces:
  - `pub(crate) struct Entry { pub code: String, pub date: String, pub red: String, pub blue: String }`
  - `pub(crate) fn clean_date(s: &str) -> String`
  - `pub(crate) fn parse_result_entries(json: &str) -> Result<Vec<Entry>, String>`

- [ ] **Step 1: 建文件并写失败测试**

Create `src/fetch.rs`:

```rust
// 命令行抓取:调用系统 curl 拉取中国福彩网开奖数据,转换为项目 CSV。
// 这是唯一联网、且依赖运行时 curl 的模块;分析引擎保持离线。

pub(crate) struct Entry {
    pub code: String,
    pub date: String,
    pub red: String,
    pub blue: String,
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
```

Modify `src/main.rs`: add `mod fetch;` next to `mod realdata;`.

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function clean_date` / `parse_result_entries`.

- [ ] **Step 3: 实现**（加到 `src/fetch.rs`,`Entry` 之后、`#[cfg(test)]` 之前)

```rust
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
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: 4 个新 fetch 测试 + 既有测试全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/fetch.rs src/main.rs
git commit -m "feat: minimal JSON extraction for welfare-lottery draw notices"
```

---

### Task 3: 列映射 + URL 构造

**Files:**
- Modify: `src/fetch.rs`

**Interfaces:**
- Consumes: `crate::realdata::parse_record`, `crate::game_spec::{GameSpec, real_data_games}`.
- Produces:
  - `pub(crate) fn build_url(name_param: &str, count: u32) -> String`
  - `pub(crate) fn map_entry(spec: &GameSpec, e: &Entry) -> Result<Vec<String>, String>`

- [ ] **Step 1: 写失败测试**（加到 `src/fetch.rs` 的 `mod tests`)

```rust
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
        let f = map_entry(&game("3d"), &entry("1", "3,8,1", "")).unwrap();
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function build_url` / `map_entry`.

- [ ] **Step 3: 实现**（加到 `src/fetch.rs`,`parse_result_entries` 之后)

```rust
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
```

需在 `src/fetch.rs` 顶部(文件开头,`Entry` 之前)加导入:

```rust
use crate::game_spec::{real_data_games, GameSpec};
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: 6 个新映射测试 + 既有测试全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/fetch.rs
git commit -m "feat: map welfare-lottery entries to validated CSV fields"
```

---

### Task 4: `run_fetch` 编排 + curl + main 分派

**Files:**
- Modify: `src/fetch.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: 上述 `build_url`/`parse_result_entries`/`map_entry`;`real_data_games`。
- Produces:
  - `pub(crate) fn print_usage()`
  - `pub(crate) fn run_fetch(args: &[String])`
  - `pub(crate) fn do_fetch(args: &[String]) -> Result<String, String>`(供测试)

- [ ] **Step 1: 写失败测试**（加到 `src/fetch.rs` 的 `mod tests`;这些用例都在触网前返回错误,离线可测)

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function do_fetch`.

- [ ] **Step 3: 实现**（加到 `src/fetch.rs`,`map_entry` 之后)

```rust
pub(crate) fn print_usage() {
    println!("用法:");
    println!("  lottery_stats                    运行完整分析报告(读 data/*.csv)");
    println!("  lottery_stats fetch <彩种> [期数]   抓取并写入 data/<彩种>.csv(默认 100 期)");
    println!("  lottery_stats help               显示本说明");
    println!("支持抓取的彩种:ssq(双色球) 3d(福彩3D) kl8(快乐8) qlc(7乐彩)");
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
        .ok_or_else(|| format!("未知彩种 '{}'。支持:ssq/3d/kl8/qlc/dlt/pl3/pl5/qxc", key))?;
    let src = spec
        .fetch
        .as_ref()
        .ok_or_else(|| format!("{} 暂不支持抓取(仅福彩 ssq/3d/kl8/qlc 支持)", spec.name))?;
    let url = build_url(src.name_param, count);
    let body = curl_get(&url)?;
    let entries = parse_result_entries(&body)?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut dropped = 0usize;
    for e in &entries {
        match map_entry(&spec, e) {
            Ok(f) => rows.push(f),
            Err(_) => dropped += 1,
        }
    }
    if rows.is_empty() {
        return Err(format!("解析到 0 条有效记录(共 {} 条,均校验失败),未写入。", entries.len()));
    }
    // 按期号升序(转成时间序,便于游程/预测分析)
    rows.sort_by(|a, b| {
        a[0].parse::<u64>().unwrap_or(0).cmp(&b[0].parse::<u64>().unwrap_or(0)).then(a[0].cmp(&b[0]))
    });
    let mut out = String::new();
    out.push_str(&format!(
        "# {} 抓取于中国福彩网 {} 期  期号,日期,号码...\n",
        spec.name, rows.len()
    ));
    for r in &rows {
        out.push_str(&r.join(","));
        out.push('\n');
    }
    std::fs::write(spec.file, &out).map_err(|e| format!("写入 {} 失败:{}", spec.file, e))?;
    Ok(format!(
        "抓取 {}:解析 {} 期(丢弃 {} 条),写入 {}。期号 {} → {}",
        spec.key, rows.len(), dropped, spec.file, rows[0][0], rows[rows.len() - 1][0]
    ))
}
```

- [ ] **Step 4: 在 main.rs 加参数分派**

在 `src/main.rs` 的 `fn main()` 最开头(现有第一条 `println!` 之前)插入:

```rust
    let cli_args: Vec<String> = std::env::args().collect();
    match cli_args.get(1).map(String::as_str) {
        Some("fetch") => {
            fetch::run_fetch(&cli_args[2..]);
            return;
        }
        Some("help") | Some("--help") => {
            fetch::print_usage();
            return;
        }
        Some(other) => {
            eprintln!("未知命令:{}", other);
            fetch::print_usage();
            return;
        }
        None => {}
    }
```

（`None` 分支为空 => 无参数时继续执行下面原有的第 1-7 章报告,行为不变。)

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test`
Expected: 3 个新 do_fetch 测试 + 既有全部 PASS。构建应无 dead-code 警告(fetch 字段与函数均已被使用)。

- [ ] **Step 6: 验证无参数行为不变 + help**

Run: `cargo run --release`
Expected: 照旧打印第 1-7 章报告(与本任务前一致)。
Run: `cargo run --release -- help`
Expected: 打印用法说明。
Run: `cargo run --release -- fetch dlt`
Expected: `抓取失败:超级大乐透 暂不支持抓取(仅福彩 ssq/3d/kl8/qlc 支持)`。

- [ ] **Step 7: 手动联网验证(尽力,不阻塞)**

Run: `cargo run --release -- fetch ssq 30`
Expected(有网且 curl 可用):`抓取 ssq:解析 N 期...写入 data/ssq.csv`;随后 `cargo run --release` 第 7 章双色球用真实数据分析。
若网络/接口不可用则打印"抓取失败:..."并**不覆盖** data/ssq.csv —— 属预期降级,记录到报告即可,不视为任务失败。

- [ ] **Step 8: Commit**

```bash
git add src/fetch.rs src/main.rs
git commit -m "feat: fetch subcommand wiring with curl and CLI dispatch"
```

---

## 自查(计划 vs spec)

- **① CLI 子命令与模块**:Task 4 main 分派(无参数=原报告)、`fetch`/`help`/未知命令;`FetchSource`+`GameSpec.fetch` 在 Task 1。✅
- **② curl+JSON+映射**:Task 2 `parse_result_entries`/`clean_date`/state 校验;Task 3 `map_entry`(red++blue 取 width)+`build_url`;curl 请求头在 Task 4 `curl_get`,URL 逐字匹配。✅
- **③ 写文件/错误/测试**:Task 4 升序写入 + 头注释 + 0 条不覆盖 + 各错误路径;测试全离线(unknown key / 无源 / 坏 count / JSON 解析 / 映射),联网靠手动。✅
- **占位符扫描**:无 TBD/TODO,每步含完整代码。✅
- **类型一致性**:`Entry{code,date,red,blue}`、`parse_result_entries -> Result<Vec<Entry>,String>`、`map_entry(spec,e)->Result<Vec<String>,String>`、`do_fetch(args)->Result<String,String>`、`GameSpec.fetch: Option<FetchSource>` 跨任务一致;`parse_record`/`field_count` 复用签名正确。✅
