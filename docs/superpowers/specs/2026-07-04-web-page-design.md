# 网页分析页面(serve 命令)— 设计文档

日期:2026-07-04
项目:`lottery_stats`(Rust,Cargo 零外部依赖)
承接:真实数据引擎(第 7 章 8 彩种)+ fetch/import 命令。

## 背景与目标

现有分析只在终端输出。新增 `serve` 子命令:启动一个**零依赖手写 HTTP 服务器**,提供本地网页,用于:
1. **看分析结果**(卡方均匀性、赌徒谬误、游程、预测打脸);
2. **切换彩种**(下拉选择,即时刷新);
3. **同步中奖号码**(页面粘贴/上传接口 JSON → 服务器走 import 管线写 CSV);
4. **方法与策略说明**(概率方法、冷/热/随机策略组合等静态中文讲解)。

保持 Cargo 零依赖:HTTP 服务器用 `std::net::TcpListener` 手写,前端用原生 HTML/JS/CSS(无框架、无 CDN)。仅绑定 `127.0.0.1`,仅本机访问。

已确认决策:内置服务器(非静态报告);同步用页面粘贴 JSON(绕开福彩 WAF,不做服务器端抓取)。

## ① 架构与"分析结果结构化"重构

现有 `analyze_uniformity/analyze_gamblers_fallacy/analyze_runs` 直接 `println` 文本,页面需要**数据**。重构:这些函数改为**返回结果结构体**,再由薄打印层格式化成现有终端文本。**一份计算,两个出口**(CLI 文本 + 网页 JSON),第 7 章 CLI 输出保持逐字不变(测试 + 快照兜底)。`prediction_stats` 已返回数据(`Vec<CompPred>`),直接复用。

结果结构(`src/realdata.rs`):
```rust
pub(crate) struct UniformRow { pub label: String, pub expected: f64, pub chi2: f64, pub df: u32,
    pub p: f64, pub uniform: bool, pub extra: Option<(u32,u64,u32,u64,f64)> } // extra=(cold,coldN,hot,hotN,sd),仅池型
pub(crate) struct GamblerRow { pub label: String, pub base_p: f64, pub cond_p: f64, pub samples: u64, pub diff: f64, pub enough: bool }
pub(crate) struct RunsRow    { pub label: String, pub appear: u64, pub runs: f64, pub mu: f64, pub z: f64, pub p: f64, pub independent: bool }
pub(crate) struct Coverage   { pub first_issue: String, pub first_date: String, pub last_issue: String, pub last_date: String, pub count: usize, pub latest: Vec<Vec<u32>> }
pub(crate) struct GameAnalysis { pub coverage: Coverage, pub uniformity: Vec<UniformRow>,
    pub gambler: Vec<GamblerRow>, pub runs: Vec<RunsRow>, pub pred_n: usize, pub pred: Vec<CompPred> }
```
- 计算函数:`compute_uniformity(spec,&draws)->Vec<UniformRow>`、`compute_gamblers(spec,&draws)->Vec<GamblerRow>`、`compute_runs(spec,&draws)->Vec<RunsRow>`、`compute_coverage(&draws)->Coverage`、`analyze_game(spec,&draws,rng)->GameAnalysis`。
- 打印层:`print_uniformity(&[UniformRow])` 等,产生与现在**逐字相同**的第 7 章文本;`run_game_report` 改为 compute→print。

模块划分:
- `src/realdata.rs`:结果结构 + compute_* + print_*(第 7 章仍走这些)。
- `src/server.rs`(新):HTTP 解析、Response、JSON 序列化、路由 `handle(...)`、静态页面、`serve()` 监听循环。
- `src/main.rs`:加 `serve` 分派。

## ② HTTP 接口与页面

手写 HTTP/1.1(仅需 GET/POST,thread-per-connection)。

| 方法 路径 | 作用 | 返回 |
|---|---|---|
| `GET /` | 主页面 | 内嵌 index.html(text/html) |
| `GET /api/games` | 彩种列表 + 是否有数据/可导入 | JSON |
| `GET /api/analysis?game=<key>` | 该彩种完整分析(或 available:false+reason) | JSON |
| `POST /api/import` | body=`{"game":"ssq","json":"<接口JSON>"}` → import 管线写 CSV | JSON `{ok, message}` |

- `/api/analysis`:数据不足(<2 期或缺文件)→ `{available:false, reason}`。
- `/api/import`:复用 `build_csv`/`process_and_write`,仅福彩 4 种;校验不过/0 有效 → `{ok:false, message}`,不覆盖原文件。
- 安全:仅绑 `127.0.0.1`;路由白名单(不按 URL 读任意文件);请求体大小上限(如 2MB)防滥用。

页面(单页,原生 JS):顶部彩种下拉;`数据同步`区(接口 URL 可点链接 + 粘贴框 + 同步按钮 + 状态);`分析结果`区(切换即刷新:卡方/冷热、赌徒谬误、游程、预测三策略 vs 理论期望);`方法与策略说明`区(静态中文:组合数与头奖概率、期望值恒负、卡方/赌徒谬误/游程各验什么、冷/热/随机为何都不优于瞎蒙)。

## ③ 错误处理与安全

- 每连接独立线程,panic 隔离(单个坏请求不拖垮服务器)。
- 未知路径 404、错误方法 405、非法 JSON body 400,均返回 JSON + 清晰消息,不 panic。
- 端口被占用 → 启动清晰报错退出。
- 仅 `127.0.0.1`;路由白名单;body 大小上限。

## ④ 测试(全离线,不起真实监听)

- **纯请求处理**:`handle(method,path,query,body)->Response` 做成纯函数,直接单测:`/api/games`、`/api/analysis?game=ssq`(用现有占位 data/ssq.csv)、未知路径 404、`/api/import` 合法/非法 body。
- **结构化重构**:结果结构字段值用已知输入核对;`print_*` 输出与重构前第 7 章文本一致(快照对比,保证 CLI 不变)。
- **HTTP 解析**:请求行/头/body 解析单测(畸形请求 → 400,不 panic)。
- **JSON 序列化**:结构体 → JSON 字符串单测(转义、数值格式)。
- socket 层(accept 循环)不单测,靠手动 `serve` + 浏览器验证。

## ⑤ 实现拆分(每个任务独立可测)

1. 分析改返回结果结构体 + `print_*` 打印层;第 7 章输出逐字不变。
2. 结构体 → JSON 手写序列化(零依赖)。
3. 极简 HTTP 请求解析(method/path/query/headers/body)+ `Response` 类型。
4. 路由 `handle(...)` 纯函数:`/api/games`、`/api/analysis`、`/api/import`(复用 import 管线)。
5. 内嵌 `index.html`(布局 + 切换彩种 + 同步框 + 方法说明)+ `GET /`。
6. `serve` 命令:TcpListener 绑 127.0.0.1、线程处理、接 `handle`;main 分派。

## 非目标(YAGNI)

- 不做鉴权/用户系统(仅本机);不做 HTTPS;不做体彩导入;不引图表库(数值 + CSS 条形);不做服务器端 fetch(同步只用粘贴 JSON);不做 WebSocket/热更新。
