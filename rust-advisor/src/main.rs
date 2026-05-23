use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;

const VERSION: &str = "0.1.0";
const DEFAULT_ADDR: &str = "127.0.0.1:8787";
const COLORS: [&str; 5] = ["white", "blue", "green", "red", "black"];

#[derive(Debug, Clone)]
struct Config {
    addr: String,
    engine: EngineMode,
    model_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineMode {
    Heuristic,
    DinoBoardNative,
}

#[derive(Debug, Deserialize)]
struct AnalyzeRequest {
    #[serde(default = "default_game")]
    game: String,
    #[serde(default)]
    capabilities: HashMap<String, Value>,
    #[serde(default)]
    cards: Vec<CardInput>,
}

#[derive(Debug, Deserialize)]
struct CardInput {
    client_id: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    card_id: Option<Value>,
    #[serde(default)]
    tier: Option<i64>,
    #[serde(default)]
    points: Option<i64>,
    #[serde(default)]
    bonus_color: Option<String>,
    #[serde(default)]
    cost: HashMap<String, Value>,
    #[serde(default)]
    location: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnalyzeResponse {
    ok: bool,
    engine: String,
    version: String,
    game: String,
    scope: String,
    cards: Vec<CardValue>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CardValue {
    client_id: String,
    value: f64,
    confidence: f64,
    method: String,
    label: String,
    reasons: Vec<String>,
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn main() {
    let config = match parse_config(env::args().skip(1).collect()) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            std::process::exit(2);
        }
    };

    if let Some(path) = &config.model_path {
        if !path.exists() {
            eprintln!("Model path does not exist: {}", path.display());
            std::process::exit(2);
        }
    }

    let listener = TcpListener::bind(&config.addr).unwrap_or_else(|err| {
        panic!("failed to bind {}: {err}", config.addr);
    });
    println!(
        "GemHUD advisor {} listening on http://{} ({})",
        VERSION,
        config.addr,
        match config.engine {
            EngineMode::Heuristic => "public-card heuristic",
            EngineMode::DinoBoardNative => "DinoBoard native adapter placeholder",
        }
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let config = config.clone();
                thread::spawn(move || {
                    if let Err(err) = handle_connection(stream, &config) {
                        eprintln!("request failed: {err}");
                    }
                });
            }
            Err(err) => eprintln!("connection failed: {err}"),
        }
    }
}

fn parse_config(args: Vec<String>) -> Result<Config, String> {
    let mut addr = DEFAULT_ADDR.to_string();
    let mut engine = EngineMode::Heuristic;
    let mut model_path = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--addr" => {
                i += 1;
                addr = args
                    .get(i)
                    .ok_or_else(|| "--addr requires a value".to_string())?
                    .clone();
            }
            "--engine" => {
                i += 1;
                let raw = args
                    .get(i)
                    .ok_or_else(|| "--engine requires a value".to_string())?;
                engine = match raw.as_str() {
                    "heuristic" => EngineMode::Heuristic,
                    "dinoboard-native" => EngineMode::DinoBoardNative,
                    _ => return Err(format!("unsupported engine: {raw}")),
                };
            }
            "--model" => {
                i += 1;
                model_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--model requires a value".to_string())?,
                ));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }
    Ok(Config {
        addr,
        engine,
        model_path,
    })
}

fn print_usage() {
    eprintln!(
        "Usage: gemhud-advisor [--addr 127.0.0.1:8787] [--engine heuristic|dinoboard-native] [--model path]\n\
         \n\
         The current executable serves GemHUD's values-only /analyze API.\n\
         The dinoboard-native engine mode is reserved for a future native DinoBoard ABI;\n\
         ONNX alone is not enough to run DinoBoard MCTS."
    );
}

fn handle_connection(mut stream: TcpStream, config: &Config) -> Result<(), String> {
    let request = read_http_request(&mut stream)?;
    let response = route(request, config);
    stream
        .write_all(response.as_bytes())
        .map_err(|err| format!("write failed: {err}"))?;
    Ok(())
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 4096];
    let mut header_end = None;

    loop {
        let n = stream
            .read(&mut temp)
            .map_err(|err| format!("read failed: {err}"))?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..n]);
        if let Some(pos) = find_header_end(&buffer) {
            header_end = Some(pos);
            break;
        }
        if buffer.len() > 1024 * 1024 {
            return Err("request headers too large".to_string());
        }
    }

    let header_end = header_end.ok_or_else(|| "invalid HTTP request".to_string())?;
    let headers_raw = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = headers_raw.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_length = 0_usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().unwrap_or(0);
            }
        }
    }

    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let n = stream
            .read(&mut temp)
            .map_err(|err| format!("read body failed: {err}"))?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..n]);
        if buffer.len() > 8 * 1024 * 1024 {
            return Err("request body too large".to_string());
        }
    }
    let body_end = (body_start + content_length).min(buffer.len());
    Ok(HttpRequest {
        method,
        path,
        body: buffer[body_start..body_end].to_vec(),
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|w| w == b"\r\n\r\n")
}

fn route(request: HttpRequest, config: &Config) -> String {
    match (request.method.as_str(), request.path.as_str()) {
        ("OPTIONS", _) => json_response(204, json!({})),
        ("GET", "/health") => json_response(
            200,
            json!({
                "ok": true,
                "service": "gemhud-advisor",
                "version": VERSION,
                "scope": "base Splendor public card value analysis only",
                "automation": false,
                "engine": engine_name(config.engine),
                "model_path": config.model_path.as_ref().map(|p| p.display().to_string()),
            }),
        ),
        ("POST", "/analyze") => analyze_route(&request.body, config),
        _ => json_response(
            404,
            json!({
                "ok": false,
                "error": "not found",
            }),
        ),
    }
}

fn analyze_route(body: &[u8], config: &Config) -> String {
    let req: AnalyzeRequest = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => {
            return json_response(
                400,
                json!({
                    "ok": false,
                    "error": format!("invalid JSON: {err}"),
                }),
            )
        }
    };

    let game = req.game.trim().to_ascii_lowercase();
    if !matches!(
        game.as_str(),
        "splendor" | "splendor_base" | "base_splendor"
    ) {
        return json_response(
            400,
            json!({
                "ok": false,
                "error": "GemHUD currently supports base Splendor only. Expansion variants are disabled.",
            }),
        );
    }
    if req
        .capabilities
        .get("automation")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return json_response(
            400,
            json!({
                "ok": false,
                "error": "GemHUD advisor accepts values-only clients and does not support automation.",
            }),
        );
    }
    if config.engine == EngineMode::DinoBoardNative {
        return json_response(
            501,
            json!({
                "ok": false,
                "error": "dinoboard-native mode needs a native DinoBoard engine ABI. Loading ONNX alone cannot run legal actions, MCTS, or feature encoding.",
            }),
        );
    }

    let mut warnings = Vec::new();
    if req.cards.is_empty() {
        warnings.push("No cards were detected in the request.".to_string());
    }
    let cards = req.cards.iter().map(score_card).collect();
    let response = AnalyzeResponse {
        ok: true,
        engine: "gemhud-rust-card-value-v0".to_string(),
        version: VERSION.to_string(),
        game: "splendor_base".to_string(),
        scope: "public visible cards; values only; no action automation".to_string(),
        cards,
        warnings,
    };
    json_response(
        200,
        serde_json::to_value(response).unwrap_or_else(|_| json!({ "ok": false })),
    )
}

fn score_card(card: &CardInput) -> CardValue {
    let tier = clamp_i64(card.tier.unwrap_or(2), 1, 3);
    let points = clamp_i64(card.points.unwrap_or(0), 0, 10);
    let cost_total: i64 = card
        .cost
        .iter()
        .filter(|(color, _)| COLORS.contains(&color.as_str()))
        .map(|(_, value)| value_to_i64(value).unwrap_or(0).max(0))
        .sum();
    let color_count = card
        .cost
        .iter()
        .filter(|(color, value)| {
            COLORS.contains(&color.as_str()) && value_to_i64(value).unwrap_or(0) > 0
        })
        .count() as f64;

    let mut reasons = Vec::new();
    if let Some(points) = card.points {
        reasons.push(format!("{points} prestige"));
    }
    if let Some(tier) = card.tier {
        reasons.push(format!("tier {tier}"));
    }
    if let Some(color) = &card.bonus_color {
        if !color.trim().is_empty() {
            reasons.push(format!("{} bonus", color.trim()));
        }
    }
    if cost_total > 0 {
        reasons.push(format!("cost {cost_total}"));
    }
    if let Some(source) = &card.source {
        if source == "gamedatas" {
            reasons.push("BGA data hint".to_string());
        }
    }
    if card.card_id.is_some() {
        // Touch the field so strict builds catch schema drift without making it
        // part of the scoring signal yet.
    }

    let prestige_efficiency = points as f64 / (cost_total.max(1) as f64);
    let low_cost_bonus = ((7.0 - cost_total as f64) / 7.0).max(0.0) * 0.12;
    let color_focus_bonus = ((4.0 - color_count) / 4.0).max(0.0) * 0.08;
    let tier_prior = match tier {
        1 => 0.18,
        2 => 0.32,
        3 => 0.46,
        _ => 0.28,
    };
    let mut score = tier_prior
        + points as f64 * 0.095
        + prestige_efficiency * 0.22
        + low_cost_bonus
        + color_focus_bonus;

    if points == 0 && tier == 1 {
        score += 0.08;
        reasons.push("early engine card".to_string());
    }
    if points >= 4 {
        score += 0.08;
        reasons.push("high prestige".to_string());
    }
    if cost_total == 0 {
        score *= 0.7;
    }
    if card
        .location
        .as_ref()
        .map(|v| v == "reserved")
        .unwrap_or(false)
    {
        score *= 0.92;
    }

    let value = clamp_f64(score, 0.0, 1.0);
    CardValue {
        client_id: card.client_id.clone(),
        value,
        confidence: card_confidence(card),
        method: "public-card-heuristic-v0".to_string(),
        label: value_label(value).to_string(),
        reasons: reasons.into_iter().take(5).collect(),
    }
}

fn card_confidence(card: &CardInput) -> f64 {
    let mut score = 0.2;
    if card.tier.is_some() {
        score += 0.2;
    }
    if card.points.is_some() {
        score += 0.2;
    }
    if card
        .bonus_color
        .as_ref()
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        score += 0.2;
    }
    if !card.cost.is_empty() {
        score += 0.2;
    }
    clamp_f64(score, 0.2, 1.0)
}

fn value_label(value: f64) -> &'static str {
    if value >= 0.66 {
        "high"
    } else if value >= 0.33 {
        "medium"
    } else {
        "low"
    }
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
}

fn clamp_i64(value: i64, min: i64, max: i64) -> i64 {
    value.max(min).min(max)
}

fn clamp_f64(value: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.max(min).min(max)
    } else {
        min
    }
}

fn default_game() -> String {
    "splendor_base".to_string()
}

fn engine_name(engine: EngineMode) -> &'static str {
    match engine {
        EngineMode::Heuristic => "heuristic",
        EngineMode::DinoBoardNative => "dinoboard-native",
    }
}

fn json_response(status: u16, body: Value) -> String {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        501 => "Not Implemented",
        _ => "OK",
    };
    let body = if status == 204 {
        String::new()
    } else {
        serde_json::to_string(&body).unwrap_or_else(|_| "{\"ok\":false}".to_string())
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        body.len(),
        body
    )
}
