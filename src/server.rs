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
    format!(
        "{{\"available\":true,\"coverage\":{{\"firstIssue\":\"{}\",\"firstDate\":\"{}\",\"lastIssue\":\"{}\",\"lastDate\":\"{}\",\"count\":{},\"latest\":[{}]}},\"uniformity\":[{}],\"gambler\":[{}],\"runs\":[{}],\"predN\":{},\"pred\":[{}]}}",
        jesc(&a.coverage.first_issue), jesc(&a.coverage.first_date),
        jesc(&a.coverage.last_issue), jesc(&a.coverage.last_date),
        a.coverage.count, latest.join(","),
        uni.join(","), gam.join(","), run.join(","), a.pred_n, pred.join(","))
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

// GET / 的页面(Task 5 用真实内容替换此占位)。
const INDEX_HTML: &str = "<!doctype html><meta charset=utf-8><title>lottery_stats</title><p>页面占位</p>";

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
}
