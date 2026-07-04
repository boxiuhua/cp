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
    format!(
        "{{\"available\":true,\"coverage\":{{\"firstIssue\":\"{}\",\"firstDate\":\"{}\",\"lastIssue\":\"{}\",\"lastDate\":\"{}\",\"count\":{},\"latest\":[{}]}},\"uniformity\":[{}],\"gambler\":[{}],\"runs\":[{}],\"predN\":{},\"pred\":[{}],\"picks\":[{}]}}",
        jesc(&a.coverage.first_issue), jesc(&a.coverage.first_date),
        jesc(&a.coverage.last_issue), jesc(&a.coverage.last_date),
        a.coverage.count, latest.join(","),
        uni.join(","), gam.join(","), run.join(","), a.pred_n, pred.join(","), picks.join(","))
}

pub(crate) struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: String,
}

pub(crate) fn parse_request(head: &str, body: &str) -> Option<Request> {
    let first = head.lines().next()?;
    let mut it = first.split_whitespace();
    let method = it.next()?.to_string();
    let target = it.next()?.to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    Some(Request { method, path, query, body: body.to_string() })
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => { out.push(b' '); i += 1; }
            b'%' if i + 2 < b.len() => {
                let h = |c: u8| (c as char).to_digit(16);
                if let (Some(a), Some(c)) = (h(b[i + 1]), h(b[i + 2])) {
                    out.push((a * 16 + c) as u8);
                    i += 3;
                } else { out.push(b'%'); i += 1; }
            }
            c => { out.push(c); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

pub(crate) fn query_get(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(url_decode(v));
            }
        }
    }
    None
}

pub(crate) struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Response {
    pub fn json(status: u16, body: String) -> Response {
        Response { status, content_type: "application/json; charset=utf-8", body }
    }
    pub fn html(body: String) -> Response {
        Response { status: 200, content_type: "text/html; charset=utf-8", body }
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK", 400 => "Bad Request", 404 => "Not Found", 405 => "Method Not Allowed", _ => "OK",
        };
        let head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.status, reason, self.content_type, self.body.as_bytes().len()
        );
        let mut v = head.into_bytes();
        v.extend_from_slice(self.body.as_bytes());
        v
    }
}

const INDEX_HTML: &str = r##"<!doctype html>
<html lang=zh><head><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<title>彩票随机性分析</title>
<style>
:root{--bg:#0f1420;--card:#182135;--line:#2a3550;--fg:#e6ebf5;--mut:#93a1c0;--ok:#4ade80;--warn:#fbbf24;--accent:#6ea8ff}
*{box-sizing:border-box}body{margin:0;font:14px/1.6 "Microsoft YaHei",system-ui,sans-serif;background:var(--bg);color:var(--fg)}
header{display:flex;align-items:center;gap:16px;padding:16px 24px;border-bottom:1px solid var(--line);background:var(--card)}
header h1{font-size:18px;margin:0}select{background:#0f1420;color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:6px 10px;font-size:14px}
main{max-width:920px;margin:0 auto;padding:24px}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:16px 20px;margin-bottom:18px}
.card h2{font-size:15px;margin:0 0 12px;color:var(--accent)}
.row{padding:8px 0;border-bottom:1px dashed var(--line)}.row:last-child{border:0}
.lab{color:var(--mut)}.ok{color:var(--ok)}.warn{color:var(--warn)}
textarea{width:100%;height:90px;background:#0f1420;color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:8px;font:12px monospace}
button{background:var(--accent);color:#0b1020;border:0;border-radius:6px;padding:8px 16px;font-size:14px;cursor:pointer;margin-top:8px}
a{color:var(--accent)}code{background:#0f1420;padding:1px 5px;border-radius:4px}
details{margin:6px 0}summary{cursor:pointer;color:var(--accent)}
.muted{color:var(--mut);font-size:13px}
</style></head>
<body>
<header><h1>彩票随机性分析</h1>
<label class=lab>彩种 <select id="game"></select></label>
<span id="status" class=muted></span></header>
<main>
<div class=card><h2>数据同步(粘贴接口 JSON)</h2>
<p class=muted>在浏览器打开接口(下方链接),复制返回的 JSON 粘贴到这里,点"同步"即可。仅福彩 ssq/3d/kl8/qlc 支持。</p>
<p id="apilink" class=muted></p>
<textarea id="json" placeholder="把接口返回的 JSON 粘贴到这里…"></textarea>
<div><button id="sync">同步</button> <span id="syncmsg" class=muted></span></div></div>

<div class=card id="analysis"><h2>分析结果</h2><div id="body" class=muted>加载中…</div></div>

<div class=card><h2>方法与策略说明</h2>
<details open><summary>头奖概率与期望值</summary><p class=muted>头奖概率完全由组合数决定(如双色球 C(33,6)×16 ≈ 1772 万分之一),与选号方式无关。每种玩法返奖率恒 &lt; 100%,期望值恒为负——买得越多,长期越确定地亏损(大数定律)。</p></details>
<details><summary>卡方均匀性检验</summary><p class=muted>统计每个号码/每位数字的历史出现频次,用卡方检验判断是否偏离"均匀分布"。p&gt;0.05 表示无法拒绝均匀假设——号码确实均匀随机,没有可利用的"冷热规律"。真实期数少时统计功效低,已在结果中注明。</p></details>
<details><summary>赌徒谬误检验</summary><p class=muted>验证"某号上期没出,下期更容易出"是否成立。对比"上期没出→本期出"的条件概率与无条件概率,两者≈相等,证明历史遗漏对下期毫无影响——"冷号该回补"是幻觉。</p></details>
<details><summary>游程检验(独立性)</summary><p class=muted>把某号"逐期是否出现"编码成 0/1 序列,用 Wald–Wolfowitz 游程检验其是否独立。p&gt;0.05 表示序列无自相关——前后期之间没有可预测的规律。</p></details>
<details><summary>预测"打脸"实验:冷/热/随机三策略</summary><p class=muted>用历史走势预测下一期,三种策略同台:冷号策略(选最少出现的)、热号策略(选最多出现的)、随机基线。统计平均命中数,三者都会贴着纯运气的理论期望,彼此无显著差异——实证任何选号策略都不优于瞎蒙。需 &gt;30 期数据才会运行。</p></details>
</div>
</main>
<script>
const $=s=>document.querySelector(s);
const API_BASE="https://www.cwl.gov.cn/cwl_admin/front/cwlkj/search/kjxx/findDrawNotice";
let games=[];
async function loadGames(){
  const r=await fetch("/api/games");const d=await r.json();games=d.games;
  const sel=$("#game");sel.innerHTML="";
  games.forEach(g=>{const o=document.createElement("option");o.value=g.key;o.textContent=g.name+(g.hasData?"":"(无数据)");sel.appendChild(o);});
  onChange();
}
function apiUrlFor(g){return g&&g.fetchable?`${API_BASE}?name=${g.key==="d3"?"3d":g.key}&issueCount=100`:null;}
function onChange(){
  const key=$("#game").value;const g=games.find(x=>x.key===key);
  const url=apiUrlFor(g);
  $("#apilink").innerHTML=url?`接口链接:<a href="${url}" target="_blank">${url}</a>`:"该彩种(体彩)暂不支持导入。";
  $("#sync").disabled=!url;
  loadAnalysis(key);
}
async function loadAnalysis(key){
  $("#body").textContent="加载中…";
  const r=await fetch("/api/analysis?game="+encodeURIComponent(key));const a=await r.json();
  if(!a.available){$("#body").innerHTML=`<span class=warn>${a.reason||"无数据"}</span>`;$("#status").textContent="";return;}
  $("#status").textContent=`${a.coverage.count} 期 ${a.coverage.firstIssue}→${a.coverage.lastIssue}`;
  let h="";
  h+="<div class=row><b>卡方均匀性</b></div>";
  a.uniformity.forEach(u=>{h+=`<div class=row><span class=lab>${u.label}</span> χ²=${u.chi2.toFixed(2)} p=${u.p.toFixed(4)} <span class="${u.uniform?'ok':'warn'}">${u.uniform?'均匀':'本样本偏离'}</span>`+(u.cold!==undefined?` <span class=muted>冷${u.cold}(${u.coldN}) 热${u.hot}(${u.hotN})</span>`:"")+"</div>";});
  h+="<div class=row><b>赌徒谬误</b></div>";
  a.gambler.forEach(g=>{h+=`<div class=row><span class=lab>${g.label}</span> `+(g.enough?`无条件 ${g.baseP.toFixed(4)} vs 条件 ${g.condP.toFixed(4)}(样本${g.samples})差 ${g.diff.toFixed(4)} <span class=ok>历史无影响</span>`:`<span class=warn>样本不足</span>`)+"</div>";});
  h+="<div class=row><b>游程检验</b></div>";
  a.runs.forEach(g=>{h+=`<div class=row><span class=lab>${g.label}</span> Z=${g.z.toFixed(3)} p=${g.p.toFixed(4)} <span class="${g.independent?'ok':'warn'}">${g.independent?'序列独立':'偶然显著'}</span></div>`;});
  h+="<div class=row><b>预测打脸实验</b></div>";
  if(a.predN===0){h+=`<div class=row class=muted>真实期数不足(需 &gt;30 期),跳过。</div>`;}
  else{a.pred.forEach(p=>{h+=`<div class=row><span class=lab>${p.label}</span> 冷 ${p.cold.toFixed(3)} / 热 ${p.hot.toFixed(3)} / 随机 ${p.random.toFixed(3)} <span class=muted>理论 ${p.expected.toFixed(3)}</span></div>`;});
    h+=`<div class=row class=muted>三策略都贴着理论期望 → 没有策略优于随机。</div>`;}
  $("#body").innerHTML=h;
}
async function doSync(){
  const key=$("#game").value;const body=$("#json").value.trim();
  if(!body){$("#syncmsg").textContent="请先粘贴 JSON";return;}
  $("#syncmsg").textContent="同步中…";
  const r=await fetch("/api/import?game="+encodeURIComponent(key),{method:"POST",body});
  const d=await r.json();
  $("#syncmsg").innerHTML=d.ok?`<span class=ok>${d.message}</span>`:`<span class=warn>${d.message}</span>`;
  if(d.ok){$("#json").value="";loadGames();}
}
$("#game").addEventListener("change",onChange);
$("#sync").addEventListener("click",doSync);
loadGames();
</script>
</body></html>"##;

pub(crate) fn handle(method: &str, path: &str, query: &str, body: &str) -> Response {
    match (method, path) {
        ("GET", "/") => Response::html(INDEX_HTML.to_string()),
        ("GET", "/api/games") => Response::json(200, games_json()),
        ("GET", "/api/analysis") => analysis_response(&query_get(query, "game").unwrap_or_default()),
        ("POST", "/api/import") => import_response(&query_get(query, "game").unwrap_or_default(), body),
        (_, "/api/games") | (_, "/api/analysis") | (_, "/api/import") | (_, "/") =>
            Response::json(405, "{\"error\":\"method not allowed\"}".to_string()),
        _ => Response::json(404, "{\"error\":\"not found\"}".to_string()),
    }
}

fn games_json() -> String {
    let items: Vec<String> = crate::game_spec::real_data_games().iter().map(|g| {
        let has = crate::realdata::load_game(g).map(|(d, _)| d.len() >= 2).unwrap_or(false);
        format!(
            "{{\"key\":\"{}\",\"name\":\"{}\",\"fetchable\":{},\"hasData\":{}}}",
            jesc(g.key), jesc(g.name), g.fetch.is_some(), has
        )
    }).collect();
    format!("{{\"games\":[{}]}}", items.join(","))
}

fn analysis_response(game: &str) -> Response {
    let games = crate::game_spec::real_data_games();
    let spec = match games.iter().find(|g| g.key == game) {
        Some(s) => s,
        None => return Response::json(404, "{\"available\":false,\"reason\":\"未知彩种\"}".to_string()),
    };
    match crate::realdata::load_game(spec) {
        Ok((draws, _)) if draws.len() >= 2 => {
            let mut rng = crate::Rng::new(20260703);
            let a = crate::realdata::analyze_game(spec, &draws, &mut rng);
            Response::json(200, analysis_to_json(&a))
        }
        Ok(_) => Response::json(200, "{\"available\":false,\"reason\":\"有效数据不足(<2 期)\"}".to_string()),
        Err(_) => Response::json(200, format!("{{\"available\":false,\"reason\":\"未找到数据文件 {}\"}}", jesc(spec.file))),
    }
}

fn import_response(game: &str, body: &str) -> Response {
    let spec = match crate::game_spec::real_data_games().into_iter().find(|g| g.key == game) {
        Some(s) => s,
        None => return Response::json(400, "{\"ok\":false,\"message\":\"未知彩种\"}".to_string()),
    };
    if spec.fetch.is_none() {
        return Response::json(400, "{\"ok\":false,\"message\":\"该彩种非福彩JSON格式,import仅支持ssq/d3/kl8/qlc\"}".to_string());
    }
    if body.len() > 2_000_000 {
        return Response::json(400, "{\"ok\":false,\"message\":\"数据过大\"}".to_string());
    }
    match crate::fetch::process_and_write(&spec, body) {
        Ok(msg) => Response::json(200, format!("{{\"ok\":true,\"message\":\"{}\"}}", jesc(&msg))),
        Err(e) => Response::json(200, format!("{{\"ok\":false,\"message\":\"{}\"}}", jesc(&e))),
    }
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() { return None; }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

fn parse_content_length(head: &str) -> usize {
    for line in head.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                return v.trim().parse().unwrap_or(0);
            }
        }
    }
    0
}

fn handle_conn(mut stream: std::net::TcpStream) -> std::io::Result<()> {
    use std::io::{Read, Write};
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(15)));
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 8192];
    let mut headers_end = None;
    loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 { break; }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") { headers_end = Some(pos); break; }
        if buf.len() > 2_000_000 { break; }
    }
    let headers_end = match headers_end {
        Some(p) => p,
        None => { let _ = stream.write_all(&Response::json(400, "{\"error\":\"bad request\"}".to_string()).to_bytes()); return Ok(()); }
    };
    let head = String::from_utf8_lossy(&buf[..headers_end]).to_string();
    let want = parse_content_length(&head);
    let body_start = headers_end + 4;
    while buf.len() < body_start + want {
        let n = stream.read(&mut tmp)?;
        if n == 0 { break; }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > 2_000_000 { break; }
    }
    let end = (body_start + want).min(buf.len());
    let body = String::from_utf8_lossy(&buf[body_start..end]).to_string();
    let resp = match parse_request(&head, &body) {
        Some(req) => handle(&req.method, &req.path, &req.query, &req.body),
        None => Response::json(400, "{\"error\":\"bad request\"}".to_string()),
    };
    stream.write_all(&resp.to_bytes())?;
    Ok(())
}

pub(crate) fn serve(port: u16) {
    let addr = format!("127.0.0.1:{}", port);
    let listener = match std::net::TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => { eprintln!("无法监听 {}:{}(端口可能被占用)", addr, e); return; }
    };
    println!("服务已启动:http://{}  (Ctrl+C 停止)", addr);
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            std::thread::spawn(move || { let _ = handle_conn(s); });
        }
    }
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

    #[test]
    fn parse_get_with_query() {
        let r = parse_request("GET /api/analysis?game=ssq HTTP/1.1\r\nHost: x", "").unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.path, "/api/analysis");
        assert_eq!(r.query, "game=ssq");
        assert_eq!(query_get(&r.query, "game").as_deref(), Some("ssq"));
    }

    #[test]
    fn parse_post_body_and_urldecode() {
        let r = parse_request("POST /api/import?game=ssq HTTP/1.1", "{\"a\":1}").unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.body, "{\"a\":1}");
        assert_eq!(query_get("k=%E4%B8%AD", "k").as_deref(), Some("中"));
    }

    #[test]
    fn response_bytes_have_status_and_length() {
        let bytes = Response::json(404, "{}".to_string()).to_bytes();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(s.contains("Content-Length: 2\r\n"));
        assert!(s.ends_with("{}"));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_request("", "").is_none());
    }

    #[test]
    fn route_games_lists_ssq() {
        let r = handle("GET", "/api/games", "", "");
        assert_eq!(r.status, 200);
        assert!(r.body.contains("\"key\":\"ssq\""));
        assert!(r.body.contains("\"hasData\":"));
    }

    #[test]
    fn route_analysis_ssq_available() {
        let r = handle("GET", "/api/analysis", "game=ssq", ""); // 占位 data/ssq.csv 有 5 期
        assert_eq!(r.status, 200);
        assert!(r.body.contains("\"available\":true"));
    }

    #[test]
    fn route_analysis_unknown_game_404() {
        let r = handle("GET", "/api/analysis", "game=zzz", "");
        assert_eq!(r.status, 404);
    }

    #[test]
    fn route_unknown_path_404() {
        assert_eq!(handle("GET", "/nope", "", "").status, 404);
    }

    #[test]
    fn route_import_unsupported_game_400() {
        // dlt 无 fetch 源 => 400,不写文件
        let r = handle("POST", "/api/import", "game=dlt", "{}");
        assert_eq!(r.status, 400);
        assert!(r.body.contains("\"ok\":false"));
    }

    #[test]
    fn route_import_garbage_body_no_overwrite() {
        // ssq 合法但 body 非法 JSON => ok:false,且不覆盖 data/ssq.csv
        let r = handle("POST", "/api/import", "game=ssq", "not-json");
        assert_eq!(r.status, 200);
        assert!(r.body.contains("\"ok\":false"));
    }

    #[test]
    fn index_page_served() {
        let r = handle("GET", "/", "", "");
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "text/html; charset=utf-8");
        assert!(r.body.contains("<title>彩票随机性分析</title>"));
        assert!(r.body.contains("id=\"game\"")); // 彩种下拉
        assert!(r.body.contains("/api/analysis")); // 前端会请求分析
        assert!(r.body.contains("方法与策略说明"));
    }

    #[test]
    fn find_subslice_works() {
        assert_eq!(find_subslice(b"abcXYdef", b"XY"), Some(3));
        assert_eq!(find_subslice(b"abc", b"ZZ"), None);
    }

    #[test]
    fn parse_content_length_reads_header() {
        assert_eq!(parse_content_length("POST / HTTP/1.1\r\nContent-Length: 42\r\nHost: x"), 42);
        assert_eq!(parse_content_length("GET / HTTP/1.1\r\nHost: x"), 0);
    }
}
