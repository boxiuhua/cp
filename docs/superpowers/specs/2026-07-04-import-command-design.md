# 导入命令(import)— 设计文档

日期:2026-07-04
项目:`lottery_stats`(Rust,Cargo 零外部依赖)
承接:fetch 命令(`docs/superpowers/specs/2026-07-04-fetch-command-design.md`)

## 背景与目标

`fetch` 命令用系统 curl 抓取中国福彩网接口,但该接口现有反爬 WAF,curl 抓不动(302→403)。而**用户的浏览器能打开同一接口 URL**。因此新增 `import` 命令:用户在浏览器打开接口、把返回的 JSON 存成文件,`import` 读取该文件并走与 fetch **完全相同**的解析/校验/写入管线,只是数据来自本地文件而非网络。这补上 fetch 的短板,几乎零新逻辑。

已确认决策:
- 导入格式:**福彩接口原始 JSON**(与 fetch 处理的 `findDrawNotice` 响应同格式)。
- 仅支持福彩 4 种(ssq/d3/kl8/qlc),即有 `fetch` 源的彩种;体彩格式不同,不适用。

## ① 共用管线重构

把 `do_fetch` 里"body → 解析 → 校验 → 排序 → 写 CSV"抽成两个函数(`src/fetch.rs`):

```rust
// 纯函数:接口 JSON body → (CSV 文本, 报告字符串),不碰文件系统(便于测试,不污染 data/)。
fn build_csv(spec: &GameSpec, body: &str) -> Result<(String, String), String>;
// 落盘:build_csv 后写入 spec.file。
fn process_and_write(spec: &GameSpec, body: &str) -> Result<String, String>;
```

- `build_csv`:`parse_result_entries(body)` → 逐条 `map_entry`(无效计入 dropped)→ `rows.is_empty()` 则 Err(不写)→ 按期号升序 → 拼头注释 + 各行 → 返回 (csv 文本, 报告)。
- `process_and_write`:`let (csv, report) = build_csv(spec, body)?; fs::write(spec.file, csv)?; Ok(report)`。
- `do_fetch` 改为:curl 拿 body → `process_and_write(&spec, &body)`(行为不变)。

## ② import 命令

CLI:
```
cargo run --release -- import <彩种> <文件>     # 读文件,转换写入 data/<彩种>.csv
例:cargo run --release -- import ssq ssq.json
```

`main.rs` 分派新增:`Some("import") => fetch::run_import(&args[2..])`;`print_usage` 增加 import 说明行。

```rust
pub(crate) fn run_import(args: &[String]) {
    match do_import(args) {
        Ok(msg) => println!("{}", msg),
        Err(e) => eprintln!("导入失败:{}", e),
    }
}

pub(crate) fn do_import(args: &[String]) -> Result<String, String> {
    let key = args.get(0).ok_or_else(|| "缺少彩种参数。用法见 `help`。".to_string())?;
    let path = args.get(1).ok_or_else(|| "缺少文件路径。用法:import <彩种> <文件>".to_string())?;
    let spec = real_data_games().into_iter().find(|g| &g.key == key)
        .ok_or_else(|| format!("未知彩种 '{}'。", key))?;
    spec.fetch.as_ref()
        .ok_or_else(|| format!("{} 非福彩 JSON 格式,import 仅支持 ssq/d3/kl8/qlc", spec.name))?;
    let body = std::fs::read_to_string(path)
        .map_err(|e| format!("无法读取文件 '{}':{}", path, e))?;
    process_and_write(&spec, &body)
}
```

## ③ 错误处理

- 缺参数 / 未知 key / 无 fetch 源(体彩4种)/ 文件不存在 → 各返回清晰中文 Err,不写文件。
- JSON 无法识别(缺 state / state≠0)、0 条有效记录 → 复用 fetch 已有闸门,报错且**不覆盖**原文件。

## ④ 测试(全离线,不污染 data/)

- `build_csv`:喂仿真 ssq JSON(2 期)→ 核对返回的 CSV 文本(含头注释、按期号升序的数据行)与报告字符串;**不写任何文件**。
- `build_csv` 拒绝:state≠0 → Err;0 条有效 → Err。
- `do_import`:未知 key、无源(dlt)、文件不存在 → 各返回 Err(均在写文件前)。
- 回归:fetch 改走 `process_and_write` 后,现有 32 个测试仍全绿(行为不变)。

## 非目标(YAGNI)

- 不支持体彩 JSON 格式;不做增量合并(仍覆盖写);不自动识别 key(需显式传参)。
