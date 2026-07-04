# 导入命令(import)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增 `import <彩种> <文件>` 子命令,读取浏览器保存的福彩接口 JSON 文件,走与 fetch 完全相同的解析/校验/写入管线生成 `data/<key>.csv`。

**Architecture:** 把 `do_fetch` 里"body→CSV"逻辑抽成纯函数 `build_csv` + 落盘 `process_and_write`,fetch 与 import 共用;`do_import` 读文件后调用同一管线。全部在 `src/fetch.rs`,`main.rs` 加一条分派。

**Tech Stack:** Rust 2021,Cargo 零外部依赖(仅 std);`std::fs`。

## Global Constraints

- **Cargo 依赖为空**:不引入任何 crate,仅 std。
- **仅福彩 4 种**:import 只支持有 `fetch` 源的彩种(ssq/d3/kl8/qlc);其余报"不支持"。
- **不 panic**:所有错误路径返回 `Result::Err`。
- **防覆盖**:0 条有效记录时不写文件、返回 Err(复用 build_csv 的闸门)。
- **无参数行为不变**:`cargo run --release`(无参数)仍跑第 1-7 章分析。
- **测试离线且不污染 data/**:`build_csv` 是纯函数(不写文件);测试不得写入 data/ 下任何真实文件。
- **游戏 key 与 name_param**:福彩3D 的 `key` 是 `"d3"`(不是 "3d")。
- **命令**:`cargo test`、`cargo run --release`、`cargo run --release -- import ssq ssq.json`。
- **Git**:每任务末尾 commit,信息 body 末行:`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`。

---

### Task 1: 抽出共用管线 `build_csv` / `process_and_write`

**Files:**
- Modify: `src/fetch.rs`

**Interfaces:**
- Consumes: 现有 `parse_result_entries`、`map_entry`;`crate::game_spec::{GameSpec, real_data_games}`(已在文件顶部导入)。现有 `mod tests` 里已有一个 `const SAMPLE: &str`(含两条 result:code 2024002 与 2024001,red/blue 齐全),本任务测试复用它。
- Produces:
  - `fn build_csv(spec: &GameSpec, body: &str) -> Result<(String, String), String>`(返回 (CSV 文本, 报告),纯函数,不写文件)
  - `fn process_and_write(spec: &GameSpec, body: &str) -> Result<String, String>`(build_csv + 写 spec.file,返回落盘报告)
  - `do_fetch` 改为经 `process_and_write` 写出(行为不变)。

- [ ] **Step 1: 写失败测试**（加到 `src/fetch.rs` 的 `mod tests`)

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function build_csv`.

- [ ] **Step 3: 实现 build_csv / process_and_write**（加到 `src/fetch.rs`,`map_entry` 之后、`do_fetch` 之前)

```rust
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
fn process_and_write(spec: &GameSpec, body: &str) -> Result<String, String> {
    let (csv, report) = build_csv(spec, body)?;
    std::fs::write(spec.file, &csv).map_err(|e| format!("写入 {} 失败:{}", spec.file, e))?;
    Ok(format!("{}:{},已写入 {}", spec.name, report, spec.file))
}
```

- [ ] **Step 4: 重构 do_fetch 复用 process_and_write**

在 `do_fetch` 中,把从 `let body = curl_get(&url)?;` 之后的整段(`parse_result_entries` → 循环 `map_entry` → `is_empty` 判断 → 排序 → 拼 CSV → `fs::write` → `Ok(format!("抓取 ..."))`)替换为:

```rust
    let body = curl_get(&url)?;
    process_and_write(&spec, &body)
```

（即 `do_fetch` 末尾变成:构造 url、curl_get、然后 return `process_and_write`。保留前面的 key/count/spec/src 解析与校验不变。)

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test`
Expected: 3 个新 build_csv 测试 + 既有 32 个测试全部 PASS(do_fetch 的 3 个错误路径测试仍绿,因它们在 curl 前返回)。构建应无新警告。

- [ ] **Step 6: 验证 fetch 行为不变**

Run: `cargo run --release -- fetch dlt`
Expected: `抓取失败:超级大乐透 暂不支持抓取(仅福彩 ssq/d3/kl8/qlc 支持)`(与重构前一致)。
Run: `cargo run --release`
Expected: 照旧打印第 1-7 章报告。

- [ ] **Step 7: Commit**

```bash
git add src/fetch.rs
git commit -m "refactor: extract build_csv/process_and_write shared by fetch"
```

---

### Task 2: `import` 命令

**Files:**
- Modify: `src/fetch.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `process_and_write`(Task 1);`real_data_games`。
- Produces:
  - `pub(crate) fn run_import(args: &[String])`
  - `pub(crate) fn do_import(args: &[String]) -> Result<String, String>`
  - `print_usage` 增加 import 说明行;`main` 增加 `import` 分派。

- [ ] **Step 1: 写失败测试**（加到 `src/fetch.rs` 的 `mod tests`;都在写文件前返回 Err,离线安全,不碰 data/)

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function do_import`.

- [ ] **Step 3: 实现 run_import / do_import**（加到 `src/fetch.rs`,`do_fetch` 之后)

```rust
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
```

- [ ] **Step 4: 在 print_usage 加一行 + main 分派**

在 `src/fetch.rs` 的 `print_usage` 里,`fetch` 那行之后加:

```rust
    println!("  lottery_stats import <彩种> <文件>  从接口 JSON 文件导入并写入 data/<彩种>.csv");
```

在 `src/main.rs` 的参数分派 `match` 中,`Some("fetch") => {...}` 分支之后加:

```rust
        Some("import") => {
            fetch::run_import(&cli_args[2..]);
            return;
        }
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test`
Expected: 4 个新 do_import 测试 + 既有全部 PASS。`cargo build --release` 无警告(run_import/do_import 均已被 main 使用)。

- [ ] **Step 6: 端到端验证(离线)**

Run: `cargo run --release -- import dlt whatever.json`
Expected: `导入失败:超级大乐透 非福彩 JSON 格式,import 仅支持 ssq/d3/kl8/qlc`。
Run: `cargo run --release -- import ssq no-such-file.json`
Expected: `导入失败:无法读取文件 'no-such-file.json':...`。
构造一个真 JSON 验证成功路径(写入 scratchpad 再导入,避免污染 data/ 前先备份):
```bash
printf '%s' '{"state":0,"result":[{"code":"2024001","date":"2024-01-02(二)","red":"01,07,15,22,28,33","blue":"09"},{"code":"2024002","date":"2024-01-04(四)","red":"03,05,11,19,26,31","blue":"02"}]}' > "$TMP/ssq_sample.json"
cp data/ssq.csv "$TMP/ssq.csv.bak"
cargo run --release -- import ssq "$TMP/ssq_sample.json"
```
Expected: `双色球:解析 2 期(丢弃 0 条),期号 2024001 → 2024002,已写入 data/ssq.csv`;随后 `cargo run --release` 第 7 章双色球用这 2 期分析。验证后 `cp "$TMP/ssq.csv.bak" data/ssq.csv` 还原占位数据(保持仓库 data/ssq.csv 为占位示例)。

（`$TMP` 用 scratchpad 目录:`C:/Users/bxh/AppData/Local/Temp/claude/D--workspase-rust-cp/91c95fde-7bc2-46cd-ad8d-68894bab5f36/scratchpad`。）

- [ ] **Step 7: Commit**

```bash
git add src/fetch.rs src/main.rs
git commit -m "feat: import subcommand to load draw data from a saved API JSON file"
```

---

## 自查(计划 vs spec)

- **① 共用管线重构**:Task 1 `build_csv`(纯函数)+`process_and_write`,do_fetch 改走它。✅
- **② import 命令**:Task 2 `do_import`(key/path/未知/无源/读文件)+`run_import`+main 分派+print_usage。✅
- **③ 错误处理**:缺参/未知 key/无源/文件缺失/0 条有效 → 均 Err 不覆盖。✅
- **④ 测试离线不污染**:build_csv 纯函数测试不写文件;do_import 错误路径测试均在写前返回;端到端手动验证用 scratchpad 文件并还原 data/ssq.csv。✅
- **占位符扫描**:无 TBD/TODO,每步含完整代码。✅
- **类型一致性**:`build_csv->Result<(String,String),String>`、`process_and_write->Result<String,String>`、`do_import->Result<String,String>` 跨任务一致;复用 `parse_result_entries`/`map_entry`/`real_data_games` 签名正确;key "d3" 一致。✅
