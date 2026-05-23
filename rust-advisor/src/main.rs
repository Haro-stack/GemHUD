use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::ptr;
use std::thread;

const VERSION: &str = "0.1.0";
const DEFAULT_ADDR: &str = "127.0.0.1:8787";
const COLORS: [&str; 5] = ["white", "blue", "green", "red", "black"];

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> c_int;
}

#[derive(Debug, Clone)]
struct Config {
    addr: String,
    engine: EngineMode,
    model_path: Option<PathBuf>,
    dinoboard_dll: Option<PathBuf>,
    simulations: i32,
    seed: u64,
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
    #[serde(default)]
    buy_action_id: Option<i64>,
    #[serde(default)]
    reserve_action_id: Option<i64>,
    #[serde(default)]
    market_index: Option<i64>,
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

type StringFree = unsafe extern "C" fn(*mut c_char);
type Json0 = unsafe extern "C" fn() -> *mut c_char;
type CreateSession =
    unsafe extern "C" fn(*const c_char, *const c_char, u64, c_int, *mut *mut c_char) -> *mut c_void;
type DestroySession = unsafe extern "C" fn(*mut c_void);
type DecideJson =
    unsafe extern "C" fn(*mut c_void, c_int, f64, c_int, *mut *mut c_char) -> *mut c_char;

struct DinoBoardApi {
    module: *mut c_void,
    string_free: StringFree,
    available_games_json: Json0,
    session_create: CreateSession,
    session_destroy: DestroySession,
    session_decide_json: DecideJson,
}

impl Drop for DinoBoardApi {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            FreeLibrary(self.module);
        }
    }
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
    if config.engine == EngineMode::DinoBoardNative {
        let Some(path) = &config.dinoboard_dll else {
            eprintln!("--engine dinoboard-native requires --dinoboard-dll");
            std::process::exit(2);
        };
        if !path.exists() {
            eprintln!("DinoBoard C ABI DLL does not exist: {}", path.display());
            std::process::exit(2);
        }
        if config.model_path.is_none() {
            eprintln!("--engine dinoboard-native requires --model");
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
            EngineMode::DinoBoardNative => "DinoBoard C ABI native adapter",
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
    let mut dinoboard_dll = None;
    let mut simulations = 96_i32;
    let mut seed = 20260524_u64;
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
            "--dinoboard-dll" => {
                i += 1;
                dinoboard_dll =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        "--dinoboard-dll requires a value".to_string()
                    })?));
            }
            "--simulations" => {
                i += 1;
                simulations = args
                    .get(i)
                    .ok_or_else(|| "--simulations requires a value".to_string())?
                    .parse::<i32>()
                    .map_err(|err| format!("invalid --simulations value: {err}"))?;
                if simulations <= 0 {
                    return Err("--simulations must be positive".to_string());
                }
            }
            "--seed" => {
                i += 1;
                seed = args
                    .get(i)
                    .ok_or_else(|| "--seed requires a value".to_string())?
                    .parse::<u64>()
                    .map_err(|err| format!("invalid --seed value: {err}"))?;
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
        dinoboard_dll,
        simulations,
        seed,
    })
}

fn print_usage() {
    eprintln!(
        "Usage: gemhud-advisor [--addr 127.0.0.1:8787] [--engine heuristic|dinoboard-native] [--model path] [--dinoboard-dll path]\n\
         \n\
         The current executable serves GemHUD's values-only /analyze API.\n\
         dinoboard-native loads DinoBoard's C ABI DLL and uses ONNX/MCTS root values.\n\
         It still needs a BGA snapshot mapper for fully accurate live-position values."
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
                "dinoboard_dll": config.dinoboard_dll.as_ref().map(|p| p.display().to_string()),
                "simulations": config.simulations,
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
    let mut warnings = Vec::new();
    if req.cards.is_empty() {
        warnings.push("No cards were detected in the request.".to_string());
    }
    let cards = if config.engine == EngineMode::DinoBoardNative {
        match score_cards_with_dinoboard(&req.cards, config, &mut warnings) {
            Ok(cards) => cards,
            Err(err) => {
                warnings.push(format!("DinoBoard native unavailable: {err}"));
                req.cards.iter().map(score_card).collect()
            }
        }
    } else {
        req.cards.iter().map(score_card).collect()
    };
    let response = AnalyzeResponse {
        ok: true,
        engine: if config.engine == EngineMode::DinoBoardNative {
            "gemhud-dinoboard-c-abi-v0".to_string()
        } else {
            "gemhud-rust-card-value-v0".to_string()
        },
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

fn score_cards_with_dinoboard(
    cards: &[CardInput],
    config: &Config,
    warnings: &mut Vec<String>,
) -> Result<Vec<CardValue>, String> {
    let dll = config
        .dinoboard_dll
        .as_ref()
        .ok_or_else(|| "--dinoboard-dll is required".to_string())?;
    let model = config
        .model_path
        .as_ref()
        .ok_or_else(|| "--model is required".to_string())?;
    let api = unsafe { load_dinoboard_api(dll)? };
    let model_c = CString::new(model.to_string_lossy().as_bytes())
        .map_err(|_| "model path contains NUL byte".to_string())?;
    let game_c = CString::new("splendor_2p").unwrap();
    unsafe {
        let mut err: *mut c_char = ptr::null_mut();
        let session =
            (api.session_create)(game_c.as_ptr(), model_c.as_ptr(), config.seed, 0, &mut err);
        if session.is_null() {
            return Err(take_error(err, api.string_free));
        }
        let raw = (api.session_decide_json)(session, config.simulations, 0.0, 1, &mut err);
        (api.session_destroy)(session);
        if raw.is_null() {
            return Err(take_error(err, api.string_free));
        }
        let decide_json = owned_string(raw, api.string_free);
        let parsed: Value = serde_json::from_str(&decide_json)
            .map_err(|err| format!("invalid DinoBoard JSON: {err}"))?;
        let _games_json = owned_string((api.available_games_json)(), api.string_free);
        warnings.push(
            "DinoBoard native mode is active. Values are mapped from DinoBoard root action values; full live BGA-state accuracy still depends on the snapshot mapper."
                .to_string(),
        );
        Ok(cards
            .iter()
            .map(|card| score_card_with_dinoboard(card, &parsed))
            .collect())
    }
}

fn score_card_with_dinoboard(card: &CardInput, root: &Value) -> CardValue {
    let buy = card
        .buy_action_id
        .and_then(|id| root_action_value(root, id))
        .map(|v| ("buy", card.buy_action_id.unwrap(), v));
    let reserve = card
        .reserve_action_id
        .and_then(|id| root_action_value(root, id))
        .map(|v| ("reserve", card.reserve_action_id.unwrap(), v));
    if let Some((kind, action_id, raw_value)) = best_action_value(buy, reserve) {
        let value = clamp_f64((raw_value + 1.0) / 2.0, 0.0, 1.0);
        return CardValue {
            client_id: card.client_id.clone(),
            value,
            confidence: if card.market_index.is_some() {
                0.75
            } else {
                0.55
            },
            method: "dinoboard-c-abi-root-action-value-v0".to_string(),
            label: value_label(value).to_string(),
            reasons: vec![
                format!("{kind} action {action_id}"),
                format!("root value {raw_value:.3}"),
                "values only".to_string(),
            ],
        };
    }
    let mut fallback = score_card(card);
    fallback.method = "public-card-heuristic-v0-no-dinoboard-action".to_string();
    fallback
        .reasons
        .push("no mapped DinoBoard action".to_string());
    fallback
}

fn best_action_value(
    buy: Option<(&'static str, i64, f64)>,
    reserve: Option<(&'static str, i64, f64)>,
) -> Option<(&'static str, i64, f64)> {
    match (buy, reserve) {
        (Some(b), Some(r)) => {
            if b.2 >= r.2 {
                Some(b)
            } else {
                Some(r)
            }
        }
        (Some(b), None) => Some(b),
        (None, Some(r)) => Some(r),
        (None, None) => None,
    }
}

fn root_action_value(root: &Value, action_id: i64) -> Option<f64> {
    let values = root
        .get("stats")?
        .get("action_values")?
        .get(action_id.to_string())?
        .as_array()?;
    values.first()?.as_f64()
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

unsafe fn load_dinoboard_api(path: &PathBuf) -> Result<DinoBoardApi, String> {
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("dinoboard-native is currently implemented for Windows DLL loading".to_string())
    }
    #[cfg(windows)]
    {
        let dll_c = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| "DLL path contains NUL byte".to_string())?;
        let module = LoadLibraryA(dll_c.as_ptr());
        if module.is_null() {
            return Err(format!("LoadLibraryA failed for {}", path.display()));
        }
        Ok(DinoBoardApi {
            module,
            string_free: load_symbol(module, "dinoboard_string_free")?,
            available_games_json: load_symbol(module, "dinoboard_available_games_json")?,
            session_create: load_symbol(module, "dinoboard_session_create")?,
            session_destroy: load_symbol(module, "dinoboard_session_destroy")?,
            session_decide_json: load_symbol(module, "dinoboard_session_decide_json")?,
        })
    }
}

#[cfg(windows)]
unsafe fn load_symbol<T>(module: *mut c_void, name: &str) -> Result<T, String> {
    let cname = CString::new(name).unwrap();
    let ptr = GetProcAddress(module, cname.as_ptr());
    if ptr.is_null() {
        return Err(format!("GetProcAddress failed for {name}"));
    }
    Ok(std::mem::transmute_copy(&ptr))
}

unsafe fn owned_string(ptr: *mut c_char, free_fn: StringFree) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let out = CStr::from_ptr(ptr).to_string_lossy().into_owned();
    free_fn(ptr);
    out
}

unsafe fn take_error(ptr: *mut c_char, free_fn: StringFree) -> String {
    if ptr.is_null() {
        return "unknown error".to_string();
    }
    owned_string(ptr, free_fn)
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
