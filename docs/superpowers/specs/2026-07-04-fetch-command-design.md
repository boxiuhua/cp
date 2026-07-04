# 抓取命令(fetch)— 设计文档

日期:2026-07-04
项目:`lottery_stats`(彩票随机性验证工具,Rust,Cargo 零外部依赖)
承接:A 阶段(双色球真实数据)、B 阶段(通用多彩种引擎)

## 背景与目标

现有工具靠手工准备 `data/<key>.csv`。本功能新增一个**命令行抓取子命令**,自动从中国福彩网官方接口拉取真实历史开奖数据、转换成项目的 CSV 格式并写入对应文件,供第 7 章分析直接读取。

已确认的决策:
- 形态:**命令行子命令**(非 GUI/网页)。
- 联网方式:调用**系统 curl**(把 TLS 甩给系统工具),Cargo 依赖仍为空;`fetch` 是唯一联网、且依赖运行时有 curl 的命令,其余分析保持离线纯净。
- 数据源:**中国福彩网 `findDrawNotice` 接口**,先支持其覆盖的**福彩 4 种**(ssq/3d/kl8/qlc);体彩 4 种(dlt/pl3/pl5/qxc)标"暂不支持抓取",抓取源做成每彩种可配置以便后续扩展。

已知风险(设计中承认):接口可能改版/限流/需要特定请求头;抓取仅建议个人学习使用(可能受官网服务条款约束)。

## ① CLI 子命令与模块

`main()` 改为参数分派,**无参数时行为不变**(照旧跑第 1-7 章报告):

```
cargo run --release                 # 无参数 → 第 1-7 章分析报告(不变)
cargo run --release -- fetch ssq    # 抓双色球 → data/ssq.csv(默认 100 期)
cargo run --release -- fetch 3d 300 # 抓福彩3D 最近 300 期
cargo run --release -- help         # 用法
```

```rust
// main.rs
let args: Vec<String> = std::env::args().collect();
match args.get(1).map(String::as_str) {
    None                    => { /* 现有第 1-7 章报告 */ }
    Some("fetch")           => fetch::run_fetch(&args[2..]),
    Some("help") | Some("--help") => fetch::print_usage(),
    Some(other)             => { eprintln!("未知命令:{}", other); fetch::print_usage(); }
}
```

新模块 `src/fetch.rs` 承载抓取全流程;分析引擎(`realdata`/`game_spec`)保持离线。抓取与分析通过 `data/<key>.csv` 文件解耦。

`game_spec.rs` 的 `GameSpec` 增加抓取源:
```rust
pub(crate) struct FetchSource { pub name_param: &'static str } // "ssq"/"3d"/"kl8"/"qlc"
// GameSpec 新增字段:pub fetch: Option<FetchSource>
```
福彩 4 种填 `Some(FetchSource{...})`,其余 4 种 `None`。

## ② curl 抓取、JSON 解析与列映射

**curl 调用**(带反屏蔽请求头):
```rust
let url = format!(
    "https://www.cwl.gov.cn/cwl_admin/front/cwlkj/search/kjxx/findDrawNotice?name={}&issueCount={}",
    src.name_param, count
);
std::process::Command::new("curl")
    .args(["-s", "--max-time", "20",
           "-H", "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
           "-H", "Referer: https://www.cwl.gov.cn/", &url])
    .output()
```
curl 不存在 → Err;退出码非 0 或 stdout 为空 → Err。

**极简 JSON 解析(零依赖)**:先确认响应含 `"state":0`;只提取每条记录的 `code`/`date`/`red`/`blue`。做法:定位每个 `"code":"` 出现处,以"本条 code 到下一条 code"为窗口,取窗口内首个 `"date":"`/`"red":"`/`"blue":"` 值。`date` 去掉星期后缀(取 `(` 之前)。

```rust
struct Entry { code: String, date: String, red: String, blue: String }
fn parse_result_entries(json: &str) -> Result<Vec<Entry>, String>   // 纯函数,可测
fn clean_date(s: &str) -> String
```

**列映射(统一规则)**:把 `red`、`blue` 按逗号拆号,拼成 `red_tokens ++ blue_tokens`,取前 `width` 个(width = 该彩种号码字段数 = `field_count - 2`):

| 彩种 | red | blue | 取前 width | 结果 |
|---|---|---|---|---|
| ssq | 6 红 | 1 蓝 | 7 | 6红+1蓝 |
| 3d | 3 位 | 空 | 3 | 3 位(保序) |
| kl8 | 20 号 | 空 | 20 | 20 号 |
| qlc | 7 号 | 1 特别号 | 7 | 丢弃特别号 |

```rust
fn map_entry(spec: &GameSpec, e: &Entry) -> Result<Vec<String>, String>
//   -> [code, clean_date, n1, n2, ...]  共 field_count 个 token
```
映射结果交给 `realdata::parse_record` 校验(复用范围/互异/位值校验),通过才收下。

## ③ 写文件、错误处理与测试

**写入**:有效记录按**期号升序**排序后写 `data/<key>.csv`;文件头一行注释 `# <名称> 抓取于中国福彩网 <N> 期  期号,日期,号码...`;完成后报告解析条数、写入路径(覆盖原文件)、首末期号。

**错误处理(不 panic,均返回清晰错误)**:
- 未知 key / `fetch=None`(体彩4种)→ 提示支持的 key 列表(ssq/3d/kl8/qlc)。
- curl 缺失 / 超时 / 非 0 退出 / 空响应 → 明确报错。
- `state != 0` 或无 `result` → 报错并附返回片段。
- 有效记录 0 条 → **不覆盖原文件**,报错退出。
- count 缺省 = 100。

**模块结构**:
- `src/fetch.rs`(新):`run_fetch`、`print_usage`、`build_url`、`curl_get`、`parse_result_entries`、`clean_date`、`map_entry`、排序+写文件。
- `src/game_spec.rs`:加 `FetchSource` 与 `GameSpec.fetch`。
- `src/main.rs`:参数分派。
- `Cargo.toml`:依赖保持为空(curl 是运行时外部程序,非 Cargo 依赖)。

**测试(全部离线,不触网)**:
- `parse_result_entries`:硬编码仿真 JSON(2-3 条 result)提取 code/date/red/blue 正确;`state!=0` 报错;无 result 报错。
- `clean_date("2024-01-02(二)")` == `"2024-01-02"`。
- `map_entry`:ssq(6+1→7)、3d(3→3)、qlc(7+丢特别号→7)、kl8(20→20)各一次,且结果能过 `realdata::parse_record`。
- 排序:乱序期号 → 升序。
- `run_fetch` 分派:未知 key、无 fetch 源(dlt)→ 返回错误。
- 联网部分(curl_get / 端到端)不做单测,靠手动 `cargo run -- fetch ssq` 验证,报告中说明。

**非目标(YAGNI)**:不支持体彩接口;不做增量合并(直接覆盖);不做定时/守护抓取;不解析中奖金额等分析无关字段。
