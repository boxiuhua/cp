# 策略选号演示 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 页面按冷号/热号/机选三策略各生成一注真实号码 + "为什么这么选",并醒目标注"中奖概率相同、并不更优"(诚实教学演示,非荐号)。

**Architecture:** `realdata.rs` 加 `StrategyPick` + `strategy_picks`(复用 pool_counts/digit_counts/pick_pool/pick_digit),塞进 `GameAnalysis.picks`;`server.rs` 序列化;`index.html` 渲染。CLI 第 7 章不打印 picks,输出保持不变。

**Tech Stack:** Rust 2021,Cargo 零依赖(仅 std);前端原生 JS。

## Global Constraints

- **Cargo 依赖为空**,仅 std,不引 crate。前端无外部 CDN。
- **诚实框定**:页面必须含醒目标注"这三注中奖概率与任意号码完全相同(1/1772万),仅演示"。不得暗示策略更优。
- **CLI 第 7 章输出逐字不变**:picks 只进结构体供网页,`run_game_report`/`format_*` 不打印。
- **可复现**:strategy_picks 用 `crate::Rng`(analyze_game 传入的固定种子 rng);冷/热平局按号码/数字升序。
- **全历史频次**:冷/热按全部 draws 的频次(pool_counts/digit_counts),不用滑动窗口。
- **命令**:`cargo test`、`cargo run --release`、`cargo run --release -- serve 8080`。
- **Git**:每任务末尾 commit,信息 body 末行:`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`。

---

### Task 1: StrategyPick + strategy_picks + GameAnalysis.picks

**Files:**
- Modify: `src/realdata.rs`

**Interfaces:**
- Consumes: `pool_counts`/`digit_counts`/`pick_pool`/`pick_digit`(同模块)、`Component`、`crate::Rng`(`sample`/`below`)。
- Produces:
  - `pub(crate) struct StrategyPick { pub strategy: String, pub why: String, pub ticket: Vec<Vec<u32>> }`
  - `pub(crate) fn strategy_picks(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng) -> Vec<StrategyPick>`
  - `GameAnalysis` 新增字段 `pub picks: Vec<StrategyPick>`;`analyze_game` 填充它。

- [ ] **Step 1: 写失败测试**（加到 `src/realdata.rs` 的 `mod tests`,复用 `rec`)

```rust
    #[test]
    fn strategy_picks_hot_cold_random() {
        let ssq = real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        let rows: Vec<DrawRecord> = (0..10).map(|_| rec(vec![vec![1,2,3,4,5,6], vec![1]])).collect();
        let mut rng = crate::Rng::new(1);
        let picks = strategy_picks(&ssq, &rows, &mut rng);
        assert_eq!(picks.len(), 3);
        assert_eq!(picks[0].strategy, "冷号");
        assert_eq!(picks[1].strategy, "热号");
        assert_eq!(picks[2].strategy, "机选");
        // 每注含 2 组件(红+蓝)
        assert_eq!(picks[0].ticket.len(), 2);
        // 热号红段=最热的 6 个 = [1,2,3,4,5,6];冷号红段=最冷的 6 个(计数0,按号升序)= [7..12]
        assert_eq!(picks[1].ticket[0], vec![1,2,3,4,5,6]);
        assert_eq!(picks[0].ticket[0], vec![7,8,9,10,11,12]);
        // 机选红段:6 个互异、均 ∈ [1,33]
        let r = &picks[2].ticket[0];
        assert_eq!(r.len(), 6);
        assert!(r.iter().all(|&n| (1..=33).contains(&n)));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: 编译错误 `cannot find function strategy_picks`.

- [ ] **Step 3: 实现 StrategyPick + strategy_picks**（加到 `src/realdata.rs`,`analyze_game` 之前)

```rust
pub(crate) struct StrategyPick {
    pub strategy: String,
    pub why: String,
    pub ticket: Vec<Vec<u32>>, // 每组件一段号码
}

// 按冷/热/机选三策略各生成一注号码(仅演示,概率并不更优)。
pub(crate) fn strategy_picks(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng) -> Vec<StrategyPick> {
    // (名称, 为什么, 模式) 模式:0=冷 1=热 2=机选
    let modes: [(&str, &str, u8); 3] = [
        ("冷号", "赌'冷号该回补了'——但历史遗漏不改变下期概率,无效。", 0),
        ("热号", "赌'热号继续走强'——但开奖独立同分布,无效。", 1),
        ("机选", "完全不依赖历史,随机一注。", 2),
    ];
    let mut out = Vec::with_capacity(3);
    for (name, why, mode) in modes {
        let mut ticket: Vec<Vec<u32>> = Vec::with_capacity(spec.components.len());
        for (ci, comp) in spec.components.iter().enumerate() {
            match comp {
                Component::Pool { size, pick, .. } => {
                    let seg = match mode {
                        0 => { let wc = pool_counts(draws, ci, *size); pick_pool(&wc, *size, *pick, false) }
                        1 => { let wc = pool_counts(draws, ci, *size); pick_pool(&wc, *size, *pick, true) }
                        _ => rng.sample(*size, *pick),
                    };
                    ticket.push(seg);
                }
                Component::Digits { bases, .. } => {
                    let mut seg = Vec::with_capacity(bases.len());
                    for (pos, &base) in bases.iter().enumerate() {
                        let d = match mode {
                            0 => { let wc = digit_counts(draws, ci, pos, base); pick_digit(&wc, false) }
                            1 => { let wc = digit_counts(draws, ci, pos, base); pick_digit(&wc, true) }
                            _ => rng.below(base as u64) as u32,
                        };
                        seg.push(d);
                    }
                    ticket.push(seg);
                }
            }
        }
        out.push(StrategyPick { strategy: name.to_string(), why: why.to_string(), ticket });
    }
    out
}
```

- [ ] **Step 4: 给 GameAnalysis 加 picks 字段并在 analyze_game 填充**

在 `GameAnalysis` 结构体末尾加字段:

```rust
    pub picks: Vec<StrategyPick>,
```

把 `analyze_game` 改为:

```rust
pub(crate) fn analyze_game(spec: &GameSpec, draws: &[DrawRecord], rng: &mut crate::Rng) -> GameAnalysis {
    let (pred_n, pred) = prediction_stats(spec, draws, 30, rng);
    let picks = strategy_picks(spec, draws, rng);
    GameAnalysis {
        coverage: compute_coverage(draws),
        uniformity: compute_uniformity(spec, draws),
        gambler: compute_gamblers(spec, draws),
        runs: compute_runs(spec, draws),
        pred_n, pred, picks,
    }
}
```

（`run_game_report` 不改 —— 它不读 picks,故 CLI 第 7 章输出不变。)

- [ ] **Step 5: 运行测试 + 确认 CLI 不变**

Run: `cargo test`
Expected: `strategy_picks_hot_cold_random` + 既有全部 PASS。
Run: `cargo run --release`
Expected: 第 7 章输出与本任务前**逐字一致**(不出现任何选号内容)。

- [ ] **Step 6: Commit**

```bash
git add src/realdata.rs
git commit -m "feat: strategy_picks (cold/hot/random) added to GameAnalysis"
```

---

### Task 2: JSON 序列化 picks

**Files:**
- Modify: `src/server.rs`

**Interfaces:**
- Consumes: `crate::realdata::StrategyPick`(经 `GameAnalysis.picks`)、`jesc`。
- Produces: `analysis_to_json` 输出新增 `"picks":[...]`。

- [ ] **Step 1: 写失败测试**（加到 `src/server.rs` 的 `mod tests`)

```rust
    #[test]
    fn analysis_json_has_picks() {
        let ssq = crate::game_spec::real_data_games().into_iter().find(|g| g.key == "ssq").unwrap();
        let draws = crate::realdata::load_game(&ssq).unwrap().0;
        let mut rng = crate::Rng::new(1);
        let a = crate::realdata::analyze_game(&ssq, &draws, &mut rng);
        let j = analysis_to_json(&a);
        assert!(j.contains("\"picks\":["));
        assert!(j.contains("\"strategy\":\"冷号\""));
        assert!(j.contains("\"ticket\":["));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: `analysis_json_has_picks` FAIL(输出无 picks)。

- [ ] **Step 3: 在 analysis_to_json 里序列化 picks**

在 `analysis_to_json` 内,`let latest: Vec<String> = ...` 之后、最终 `format!(...)` 之前,加:

```rust
    let picks: Vec<String> = a.picks.iter().map(|p| {
        let tk: Vec<String> = p.ticket.iter().map(|seg| {
            let ns: Vec<String> = seg.iter().map(|n| n.to_string()).collect();
            format!("[{}]", ns.join(","))
        }).collect();
        format!(
            "{{\"strategy\":\"{}\",\"why\":\"{}\",\"ticket\":[{}]}}",
            jesc(&p.strategy), jesc(&p.why), tk.join(",")
        )
    }).collect();
```

在最终 `format!` 的 JSON 里、`"pred":[{}]` 之后追加 `,"picks":[{}]`,并把 `picks.join(",")` 作为对应参数加到 `format!` 参数列表末尾。例如把结尾:

```rust
        ...,\"predN\":{},\"pred\":[{}]}}",
        ..., a.pred_n, pred.join(","))
```

改为:

```rust
        ...,\"predN\":{},\"pred\":[{}],\"picks\":[{}]}}",
        ..., a.pred_n, pred.join(","), picks.join(","))
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: `analysis_json_has_picks` + 既有全部 PASS。可用 `cargo build` 确认无新错误。

- [ ] **Step 5: Commit**

```bash
git add src/server.rs
git commit -m "feat: serialize strategy picks into analysis JSON"
```

---

### Task 3: 页面渲染「策略选号」卡片

**Files:**
- Modify: `src/server.rs`(INDEX_HTML 常量)

**Interfaces:**
- Consumes: `/api/analysis` 返回的 `picks`。
- Produces: 页面在分析卡片上方新增策略选号卡片 + 渲染逻辑;`index_page_served` 增断言。

- [ ] **Step 1: 写失败测试**（加到 `src/server.rs` 的 `mod tests`)

```rust
    #[test]
    fn index_page_has_strategy_section() {
        let r = handle("GET", "/", "", "");
        assert_eq!(r.status, 200);
        assert!(r.body.contains("策略选号(仅演示)"));
        assert!(r.body.contains("中奖概率与任意号码"));
        assert!(r.body.contains("id=\"picksbody\""));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`
Expected: `index_page_has_strategy_section` FAIL(页面尚无该卡片)。

- [ ] **Step 3: 在 INDEX_HTML 加卡片 + 渲染**

INDEX_HTML 是 `r##"..."##` 原始字符串。做两处编辑:

(a) 在"数据同步"卡片(`<div class=card><h2>数据同步…`)**之后**、分析卡片(`<div class=card id="analysis">`)**之前**,插入策略选号卡片:

```html
<div class=card id="picks"><h2>策略选号(仅演示)</h2>
<p class=warn>⚠ 这三注号码的中奖概率与任意号码完全相同(约 1/1772万)。本演示只为展示"策略"长什么样,并不能提高中奖机会。</p>
<div id="picksbody" class=muted>加载中…</div>
<p class=muted>下方「预测打脸实验」显示冷/热/随机三策略历史平均命中数都≈理论值——它们并不比机选更优。</p></div>
```

(b) 在 `<script>` 里的 `loadAnalysis(key)` 函数中渲染 picks。找到函数里"不可用"分支(`if(!a.available){...return;}`),在该分支内把 picksbody 设为占位:

```js
  if(!a.available){$("#body").innerHTML=`<span class=warn>${a.reason||"无数据"}</span>`;$("#picksbody").innerHTML="<span class=muted>(无数据,无法演示选号)</span>";$("#status").textContent="";return;}
```

并在函数后半段(拿到 available 的 `a` 之后,渲染 `#body` 之前或之后)加入 picks 渲染:

```js
  let ph="";
  (a.picks||[]).forEach(p=>{
    const t=p.ticket.map(seg=>seg.map(n=>String(n).padStart(2,"0")).join(" ")).join(" | ");
    ph+=`<div class=row><b>${p.strategy}</b> <code>${t}</code><div class=muted>为什么:${p.why}</div></div>`;
  });
  $("#picksbody").innerHTML=ph;
```

- [ ] **Step 4: 运行测试确认通过 + 手动验证**

Run: `cargo test`
Expected: `index_page_has_strategy_section` + 既有全部 PASS。`cargo build --release` 无警告。
手动:后台 `cargo run --release -- serve 8093`,`curl -s --noproxy '*' "http://127.0.0.1:8093/api/analysis?game=ssq"` 确认返回含 `"picks"`;浏览器开页面确认策略选号卡片显示三注号码 + "为什么" + 警示。验证后停服务;若写入 data/ 则 `git checkout -- data/`。

- [ ] **Step 5: Commit**

```bash
git add src/server.rs
git commit -m "feat: strategy-picks card on the web page with honest caveat"
```

---

## 自查(计划 vs spec)

- **① 模型与算法**:Task 1 StrategyPick + strategy_picks(冷/热全历史 + 机选 rng)+ GameAnalysis.picks;CLI 不打印(输出不变)。✅
- **② JSON**:Task 2 analysis_to_json 加 picks(strategy/why 经 jesc,ticket 嵌套整数)。✅
- **③ 页面**:Task 3 策略选号卡片 + 诚实警示 + 指向回测 + 渲染;不可用时占位。✅
- **诚实框定**:警示文案在 Task 3 页面 + Global Constraints。✅
- **占位符**:无 TBD,每步含完整代码。✅
- **类型一致**:`StrategyPick{strategy,why,ticket}`、`strategy_picks(spec,draws,rng)->Vec<StrategyPick>`、`GameAnalysis.picks`、JSON `picks`/`strategy`/`ticket` 跨任务一致。✅
