use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::ptr;
use std::thread;
use std::time::Instant;

const VERSION: &str = "0.1.0";
const DEFAULT_ADDR: &str = "127.0.0.1:8787";
const DEFAULT_SIMULATIONS: i32 = 256;
const DEFAULT_SEED: u64 = 20260524;
const CONFIG_FILE_NAME: &str = "gemhud-advisor.config.json";
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

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    addr: Option<String>,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    model: Option<PathBuf>,
    #[serde(default)]
    model_path: Option<PathBuf>,
    #[serde(default)]
    dinoboard_dll: Option<PathBuf>,
    #[serde(default)]
    simulations: Option<i32>,
    #[serde(default)]
    seed: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeRequest {
    #[serde(default = "default_game")]
    game: String,
    #[serde(default)]
    capabilities: HashMap<String, Value>,
    #[serde(default)]
    cards: Vec<CardInput>,
    #[serde(default)]
    dinoboard_snapshot: Option<Value>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    recommendation: Option<ActionRecommendation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    recommendations: Vec<ActionRecommendation>,
}

#[derive(Debug, Serialize)]
struct CardValue {
    client_id: String,
    value: f64,
    confidence: f64,
    method: String,
    label: String,
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    self_status: Option<CardPurchaseStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opponent_status: Option<CardPurchaseStatus>,
}

#[derive(Debug, Clone, Serialize)]
struct CardPurchaseStatus {
    can_buy_now: bool,
    turns_to_buy: i64,
    token_deficit: i64,
    gold_used: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    player_index: Option<usize>,
    label: String,
}

#[derive(Debug, Clone, Serialize)]
struct ActionRecommendation {
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<f64>,
    confidence: f64,
    method: String,
    reasons: Vec<String>,
}

struct NativeAnalysis {
    cards: Vec<CardValue>,
    recommendations: Vec<ActionRecommendation>,
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
type SetIntField = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_int,
    c_int,
    c_int,
    *mut *mut c_char,
) -> c_int;
type SetVizField = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_int,
    c_int,
    c_int,
    c_int,
    *mut *mut c_char,
) -> c_int;
type RebuildViews = unsafe extern "C" fn(*mut c_void, *mut *mut c_char) -> c_int;
type DecideJson =
    unsafe extern "C" fn(*mut c_void, c_int, f64, c_int, *mut *mut c_char) -> *mut c_char;

struct DinoBoardApi {
    module: *mut c_void,
    string_free: StringFree,
    available_games_json: Json0,
    session_create: CreateSession,
    session_destroy: DestroySession,
    session_set_int_field: SetIntField,
    session_set_viz_field: SetVizField,
    session_rebuild_views: RebuildViews,
    session_decide_json: DecideJson,
    splendor_card_pool_json: Json0,
    splendor_nobles_json: Json0,
}

#[derive(Debug, Clone, Deserialize)]
struct DinoCardDef {
    id: i64,
    tier: i64,
    bonus: i64,
    points: i64,
    cost: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct DinoNobleDef {
    id: i64,
    requirements: Vec<i64>,
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
    println!(
        "Config: simulations={}, model={}, dinoboard_dll={}",
        config.simulations,
        config
            .model_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string()),
        config
            .dinoboard_dll
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string())
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
    let mut config_path = None;
    let mut arg_index = 0;
    while arg_index < args.len() {
        if args[arg_index] == "--config" {
            arg_index += 1;
            config_path = Some(PathBuf::from(
                args.get(arg_index)
                    .ok_or_else(|| "--config requires a value".to_string())?,
            ));
        }
        arg_index += 1;
    }

    let mut addr = DEFAULT_ADDR.to_string();
    let mut engine = EngineMode::Heuristic;
    let mut engine_explicit = false;
    let mut model_path = None;
    let mut dinoboard_dll = None;
    let mut simulations = DEFAULT_SIMULATIONS;
    let mut seed = DEFAULT_SEED;

    if let Some(file_config) = load_file_config(config_path.as_deref())? {
        if let Some(value) = file_config.addr {
            addr = value;
        }
        if let Some(raw) = file_config.engine {
            engine = parse_engine(&raw)?;
            engine_explicit = true;
        }
        model_path = file_config.model_path.or(file_config.model).or(model_path);
        dinoboard_dll = file_config.dinoboard_dll.or(dinoboard_dll);
        if let Some(value) = file_config.simulations {
            simulations = validate_simulations(value)?;
        }
        if let Some(value) = file_config.seed {
            seed = value;
        }
    }

    if dinoboard_dll.is_none() {
        dinoboard_dll = auto_detect_dinoboard_dll();
    }
    if model_path.is_none() {
        model_path = auto_detect_dinoboard_model();
    }
    if !engine_explicit && dinoboard_dll.is_some() && model_path.is_some() {
        engine = EngineMode::DinoBoardNative;
    }

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                args.get(i)
                    .ok_or_else(|| "--config requires a value".to_string())?;
            }
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
                engine = parse_engine(raw)?;
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
                simulations = validate_simulations(
                    args.get(i)
                        .ok_or_else(|| "--simulations requires a value".to_string())?
                        .parse::<i32>()
                        .map_err(|err| format!("invalid --simulations value: {err}"))?,
                )?;
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

fn parse_engine(raw: &str) -> Result<EngineMode, String> {
    match raw {
        "heuristic" => Ok(EngineMode::Heuristic),
        "dinoboard-native" => Ok(EngineMode::DinoBoardNative),
        _ => Err(format!("unsupported engine: {raw}")),
    }
}

fn validate_simulations(value: i32) -> Result<i32, String> {
    if value <= 0 {
        return Err("--simulations must be positive".to_string());
    }
    Ok(value)
}

fn load_file_config(explicit_path: Option<&Path>) -> Result<Option<FileConfig>, String> {
    let path = explicit_path.map(PathBuf::from).or_else(|| {
        default_config_paths()
            .into_iter()
            .find(|path| path.exists())
    });
    let Some(path) = path else {
        return Ok(None);
    };
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    let config = serde_json::from_str::<FileConfig>(&text)
        .map_err(|err| format!("invalid config {}: {err}", path.display()))?;
    Ok(Some(config))
}

fn default_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(current) = env::current_dir() {
        paths.push(current.join(CONFIG_FILE_NAME));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join(CONFIG_FILE_NAME));
            for ancestor in dir.ancestors().take(5) {
                paths.push(ancestor.join(CONFIG_FILE_NAME));
            }
        }
    }
    dedupe_paths(paths)
}

fn auto_detect_dinoboard_dll() -> Option<PathBuf> {
    dinoboard_root_candidates()
        .into_iter()
        .map(|root| root.join("build-capi").join("dinoboard_c_api.dll"))
        .find(|path| path.exists())
}

fn auto_detect_dinoboard_model() -> Option<PathBuf> {
    dinoboard_root_candidates()
        .into_iter()
        .map(|root| {
            root.join("games")
                .join("splendor")
                .join("model")
                .join("splendor_2p.onnx")
        })
        .find(|path| path.exists())
}

fn dinoboard_root_candidates() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(r"D:\codex\Haro-DinoBoard")];
    if let Ok(current) = env::current_dir() {
        add_dinoboard_siblings(&mut paths, &current);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            add_dinoboard_siblings(&mut paths, dir);
        }
    }
    dedupe_paths(paths)
}

fn add_dinoboard_siblings(paths: &mut Vec<PathBuf>, start: &Path) {
    for ancestor in start.ancestors().take(8) {
        paths.push(ancestor.join("Haro-DinoBoard"));
        if let Some(parent) = ancestor.parent() {
            paths.push(parent.join("Haro-DinoBoard"));
        }
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        let key = path.to_string_lossy().to_string();
        if seen.insert(key) {
            out.push(path);
        }
    }
    out
}

fn print_usage() {
    eprintln!(
        "Usage: gemhud-advisor [--config path] [--addr 127.0.0.1:8787] [--engine heuristic|dinoboard-native] [--model path] [--dinoboard-dll path] [--simulations 256]\n\
         \n\
         The current executable serves GemHUD's values-only /analyze API.\n\
         With no arguments, it auto-detects a local Haro-DinoBoard DLL/model when available.\n\
         Optional config file: gemhud-advisor.config.json next to the executable or current directory."
    );
}

fn handle_connection(mut stream: TcpStream, config: &Config) -> Result<(), String> {
    let request = read_http_request(&mut stream)?;
    let method = request.method.clone();
    let path = request.path.clone();
    let body_len = request.body.len();
    let started = Instant::now();
    println!("{method} {path} body={body_len} bytes");
    let response = route(request, config);
    println!(
        "{method} {path} done in {}ms",
        started.elapsed().as_millis()
    );
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
    let analyze_started = Instant::now();

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
    if let Some(snapshot) = &req.dinoboard_snapshot {
        append_snapshot_warnings(snapshot, &mut warnings);
    }
    println!(
        "Analyze start: cards={}, snapshot={}, engine={}, simulations={}",
        req.cards.len(),
        req.dinoboard_snapshot.is_some(),
        engine_name(config.engine),
        config.simulations
    );
    let (cards, recommendations) = if config.engine == EngineMode::DinoBoardNative {
        match score_cards_with_dinoboard(
            &req.cards,
            config,
            &mut warnings,
            req.dinoboard_snapshot.as_ref(),
        ) {
            Ok(analysis) => (analysis.cards, analysis.recommendations),
            Err(err) => {
                warnings.push(format!("DinoBoard native unavailable: {err}"));
                let cards = req
                    .cards
                    .iter()
                    .map(|card| {
                        req.dinoboard_snapshot
                            .as_ref()
                            .map(|snapshot| score_card_with_snapshot(card, snapshot))
                            .unwrap_or_else(|| score_card(card))
                    })
                    .collect();
                (
                    cards,
                    recommendations_from_snapshot(req.dinoboard_snapshot.as_ref(), &req.cards),
                )
            }
        }
    } else {
        let cards = req
            .cards
            .iter()
            .map(|card| {
                req.dinoboard_snapshot
                    .as_ref()
                    .map(|snapshot| score_card_with_snapshot(card, snapshot))
                    .unwrap_or_else(|| score_card(card))
            })
            .collect();
        (
            cards,
            recommendations_from_snapshot(req.dinoboard_snapshot.as_ref(), &req.cards),
        )
    };
    let recommendation = recommendations.first().cloned();
    let recommendation_label = recommendation
        .as_ref()
        .map(|item| item.label.as_str())
        .unwrap_or("-");
    println!(
        "Analyze result: cards={}, recommendation={}, recommendations={}, warnings={}, elapsed={}ms",
        cards.len(),
        recommendation_label,
        recommendations.len(),
        warnings.len(),
        analyze_started.elapsed().as_millis()
    );
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
        recommendation,
        recommendations,
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
    snapshot: Option<&Value>,
) -> Result<NativeAnalysis, String> {
    let dll = config
        .dinoboard_dll
        .as_ref()
        .ok_or_else(|| "--dinoboard-dll is required".to_string())?;
    let configured_model = config
        .model_path
        .as_ref()
        .ok_or_else(|| "--model is required".to_string())?;
    let game_id = snapshot
        .and_then(|value| value.get("game_id").and_then(Value::as_str))
        .unwrap_or("splendor_2p");
    let model = model_path_for_game(configured_model, game_id);
    let api = unsafe { load_dinoboard_api(dll)? };
    let model_c = CString::new(model.to_string_lossy().as_bytes())
        .map_err(|_| "model path contains NUL byte".to_string())?;
    let game_c = CString::new(game_id).map_err(|_| "game_id contains NUL byte".to_string())?;
    unsafe {
        let mut err: *mut c_char = ptr::null_mut();
        let session =
            (api.session_create)(game_c.as_ptr(), model_c.as_ptr(), config.seed, 0, &mut err);
        if session.is_null() {
            return Err(take_error(err, api.string_free));
        }
        if let Some(snapshot) = snapshot {
            if !snapshot
                .get("supported")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                (api.session_destroy)(session);
                return Err(
                    "snapshot includes unsupported expansion data for the base DinoBoard model"
                        .to_string(),
                );
            }
            if let Err(err) =
                apply_splendor_snapshot_to_dinoboard(&api, session, snapshot, warnings)
            {
                (api.session_destroy)(session);
                return Err(err);
            }
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
        if snapshot.is_some() {
            warnings.push(
                "DinoBoard native mode applied the mapped BGA public snapshot before MCTS."
                    .to_string(),
            );
        } else {
            warnings.push(
                "DinoBoard native mode is active without a BGA snapshot, so values use the initial generated state."
                    .to_string(),
            );
        }
        Ok(NativeAnalysis {
            cards: cards
                .iter()
                .map(|card| score_card_with_dinoboard(card, &parsed, snapshot))
                .collect(),
            recommendations: recommend_actions_from_root(&parsed, 3),
        })
    }
}

fn model_path_for_game(configured_model: &Path, game_id: &str) -> PathBuf {
    let filename = match game_id {
        "splendor_2p" => "splendor_2p.onnx",
        "splendor_3p" => "splendor_3p.onnx",
        "splendor_4p" => "splendor_4p.onnx",
        _ => return configured_model.to_path_buf(),
    };
    configured_model
        .parent()
        .map(|parent| parent.join(filename))
        .filter(|candidate| candidate.exists())
        .unwrap_or_else(|| configured_model.to_path_buf())
}

fn score_card_with_dinoboard(
    card: &CardInput,
    root: &Value,
    snapshot: Option<&Value>,
) -> CardValue {
    let buy = card
        .buy_action_id
        .and_then(|id| root_action_value(root, id))
        .map(|v| ("buy", card.buy_action_id.unwrap(), v));
    let reserve = card
        .reserve_action_id
        .and_then(|id| root_action_value(root, id))
        .map(|v| ("reserve", card.reserve_action_id.unwrap(), v));
    let mut scored = snapshot
        .map(|snapshot| score_card_with_snapshot(card, snapshot))
        .unwrap_or_else(|| score_card(card));
    if let Some((kind, action_id, raw_value)) = best_action_value(buy, reserve) {
        let root_value = clamp_f64((raw_value + 1.0) / 2.0, 0.0, 1.0);
        scored.value = clamp_f64(scored.value * 0.78 + root_value * 0.22, 0.0, 1.0);
        scored.confidence = scored.confidence.max(if card.market_index.is_some() {
            0.88
        } else {
            0.65
        });
        scored.method = "strategic-heuristic-plus-dinoboard-v1".to_string();
        scored.label = value_label(scored.value).to_string();
        scored.reasons.push(format!("{kind} action {action_id}"));
        scored.reasons.push(format!("root value {raw_value:.3}"));
        scored.reasons.truncate(10);
        return scored;
    }
    scored.method = "strategic-heuristic-v1-no-dinoboard-action".to_string();
    scored
        .reasons
        .push("no mapped DinoBoard action".to_string());
    scored
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

fn recommend_actions_from_root(root: &Value, limit: usize) -> Vec<ActionRecommendation> {
    let Some(actions) = root
        .get("stats")
        .and_then(|stats| stats.get("root_actions"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut ranked: Vec<(i64, f64)> = actions
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let action = value_to_i64(item)?;
            let raw = root_action_rank_value(root, actions, idx, action);
            Some((action, raw))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
        .into_iter()
        .take(limit)
        .map(|(action, raw_value)| {
            let value = clamp_f64((raw_value + 1.0) / 2.0, 0.0, 1.0);
            ActionRecommendation {
                label: label_action_id_detailed(action),
                action_id: Some(action),
                value: Some(value),
                confidence: 0.78,
                method: "dinoboard-c-abi-root-action-value-v0".to_string(),
                reasons: vec![
                    format!("root action {action}"),
                    format!("root value {raw_value:.3}"),
                    "ranked by MCTS root action value".to_string(),
                    "values only; no automation".to_string(),
                ],
            }
        })
        .collect()
}

fn root_action_rank_value(root: &Value, actions: &[Value], idx: usize, action: i64) -> f64 {
    root_action_value(root, action).unwrap_or_else(|| {
        root.get("stats")
            .and_then(|stats| stats.get("root_values"))
            .and_then(Value::as_array)
            .and_then(|values| values.get(idx))
            .and_then(Value::as_f64)
            .or_else(|| {
                actions
                    .iter()
                    .position(|item| value_to_i64(item) == Some(action))
                    .and_then(|pos| {
                        root.get("stats")
                            .and_then(|stats| stats.get("root_values"))
                            .and_then(Value::as_array)
                            .and_then(|values| values.get(pos))
                            .and_then(Value::as_f64)
                    })
            })
            .unwrap_or(-1.0)
    })
}

fn recommendations_from_snapshot(
    snapshot: Option<&Value>,
    cards: &[CardInput],
) -> Vec<ActionRecommendation> {
    recommend_action_from_snapshot(snapshot, cards)
        .into_iter()
        .collect()
}

fn recommend_action_from_snapshot(
    snapshot: Option<&Value>,
    cards: &[CardInput],
) -> Option<ActionRecommendation> {
    let snapshot = snapshot?;
    let player = current_snapshot_player(snapshot)?;
    let gems = value_i64_array(player.get("tokens"), 6);
    let bonuses = value_i64_array(player.get("bonuses"), 5);
    let mut best_buy: Option<(&CardInput, f64)> = None;
    let mut best_target: Option<(&CardInput, f64, i64)> = None;
    for card in cards {
        let cost = card_cost_vector(card);
        let (deficit, _) = payment_deficit(&cost, &bonuses, &gems);
        let value = score_card_with_snapshot(card, snapshot).value;
        if deficit == 0 && best_buy.map(|(_, score)| value > score).unwrap_or(true) {
            best_buy = Some((card, value));
        }
        if deficit > 0
            && best_target
                .map(|(_, score, old_deficit)| {
                    value - deficit as f64 * 0.04 > score - old_deficit as f64 * 0.04
                })
                .unwrap_or(true)
        {
            best_target = Some((card, value, deficit));
        }
    }
    if let Some((card, value)) = best_buy {
        return Some(ActionRecommendation {
            label: format!("购买 {}", short_card_label(card)),
            action_id: card.buy_action_id,
            value: Some(value),
            confidence: 0.72,
            method: "state-aware-heuristic-v1".to_string(),
            reasons: vec![
                "card is affordable now".to_string(),
                short_card_reason(card),
                "values only; no automation".to_string(),
            ],
        });
    }

    let reserved_count = player
        .get("reserved")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    if let Some((card, value, deficit)) = best_target {
        let bank = value_i64_array(snapshot.get("bank"), 6);
        let take = suggested_gems_for_card(card, &bonuses, &gems, &bank);
        if !take.is_empty() {
            return Some(ActionRecommendation {
                label: format!("拿宝石 {}", take.join(" ")),
                action_id: None,
                value: Some(clamp_f64(value - deficit as f64 * 0.03, 0.0, 1.0)),
                confidence: 0.62,
                method: "state-aware-heuristic-v1".to_string(),
                reasons: vec![
                    format!("toward {}", short_card_label(card)),
                    format!("{deficit} tokens short"),
                    "values only; no automation".to_string(),
                ],
            });
        }
        if reserved_count < 3 {
            return Some(ActionRecommendation {
                label: format!("预定 {}", short_card_label(card)),
                action_id: card.reserve_action_id,
                value: Some(clamp_f64(value * 0.92, 0.0, 1.0)),
                confidence: 0.56,
                method: "state-aware-heuristic-v1".to_string(),
                reasons: vec![
                    "no useful gem take found".to_string(),
                    short_card_reason(card),
                    "values only; no automation".to_string(),
                ],
            });
        }
    }
    Some(ActionRecommendation {
        label: "放弃或调整目标".to_string(),
        action_id: None,
        value: Some(0.2),
        confidence: 0.35,
        method: "state-aware-heuristic-v1".to_string(),
        reasons: vec!["no clearly useful public action found".to_string()],
    })
}

fn suggested_gems_for_card(
    card: &CardInput,
    bonuses: &[i64],
    gems: &[i64],
    bank: &[i64],
) -> Vec<String> {
    let cost = card_cost_vector(card);
    let mut needed: Vec<(usize, i64)> = (0..COLORS.len())
        .map(|idx| {
            (
                idx,
                (cost.get(idx).copied().unwrap_or(0)
                    - bonuses.get(idx).copied().unwrap_or(0)
                    - gems.get(idx).copied().unwrap_or(0))
                .max(0),
            )
        })
        .filter(|(idx, need)| *need > 0 && bank.get(*idx).copied().unwrap_or(0) > 0)
        .collect();
    needed.sort_by(|a, b| b.1.cmp(&a.1));
    if let Some((idx, need)) = needed.first().copied() {
        if need >= 2 && bank.get(idx).copied().unwrap_or(0) >= 4 {
            return vec![color_short(idx).to_string(), color_short(idx).to_string()];
        }
    }
    let mut chosen: Vec<usize> = needed.into_iter().take(3).map(|(idx, _)| idx).collect();
    if chosen.len() < 3 {
        let mut fillers: Vec<usize> = (0..COLORS.len())
            .filter(|idx| {
                !chosen.contains(idx)
                    && bank.get(*idx).copied().unwrap_or(0) > 0
                    && !is_overstocked_color(*idx, gems)
            })
            .collect();
        fillers.sort_by_key(|idx| cost.get(*idx).copied().unwrap_or(0));
        fillers.reverse();
        for idx in fillers {
            chosen.push(idx);
            if chosen.len() >= 3 {
                break;
            }
        }
    }
    if chosen.len() < 3 {
        for idx in 0..COLORS.len() {
            if !chosen.contains(&idx) && bank.get(idx).copied().unwrap_or(0) > 0 {
                chosen.push(idx);
                if chosen.len() >= 3 {
                    break;
                }
            }
        }
    }
    chosen
        .into_iter()
        .map(|idx| color_short(idx).to_string())
        .collect()
}

fn is_overstocked_color(idx: usize, gems: &[i64]) -> bool {
    gems.get(idx).copied().unwrap_or(0) >= 3
}

fn color_short(idx: usize) -> &'static str {
    match idx {
        0 => "W",
        1 => "U",
        2 => "G",
        3 => "R",
        4 => "B",
        _ => "?",
    }
}

fn label_action_id_detailed(action: i64) -> String {
    match action {
        0..=11 => format!("购买 T{} 第{}张", action / 4 + 1, action % 4 + 1),
        12..=23 => {
            let slot = action - 12;
            format!("预定 T{} 第{}张", slot / 4 + 1, slot % 4 + 1)
        }
        24..=26 => format!("预定 T{} 牌堆", action - 23),
        27..=29 => format!("购买预定牌{}", action - 26),
        30..=39 => take_three_combo((action - 30) as usize)
            .map(|colors| format!("拿宝石 {}", colors_label(&colors)))
            .unwrap_or_else(|| "拿三种宝石".to_string()),
        40..=49 => take_two_different_combo((action - 40) as usize)
            .map(|colors| format!("拿宝石 {}", colors_label(&colors)))
            .unwrap_or_else(|| "拿两种宝石".to_string()),
        50..=54 => format!("拿宝石 {}", color_short((action - 50) as usize)),
        55..=59 => {
            let color = color_short((action - 55) as usize);
            format!("拿宝石 {color} {color}")
        }
        60..=64 => format!("选择贵族{}", action - 59),
        65..=70 => format!("弃宝石 {}", color_short((action - 65) as usize)),
        _ => "执行最高价值合法动作".to_string(),
    }
}

fn take_three_combo(index: usize) -> Option<[usize; 3]> {
    const COMBOS: [[usize; 3]; 10] = [
        [0, 1, 2],
        [0, 1, 3],
        [0, 1, 4],
        [0, 2, 3],
        [0, 2, 4],
        [0, 3, 4],
        [1, 2, 3],
        [1, 2, 4],
        [1, 3, 4],
        [2, 3, 4],
    ];
    COMBOS.get(index).copied()
}

fn take_two_different_combo(index: usize) -> Option<[usize; 2]> {
    const COMBOS: [[usize; 2]; 10] = [
        [0, 1],
        [0, 2],
        [0, 3],
        [0, 4],
        [1, 2],
        [1, 3],
        [1, 4],
        [2, 3],
        [2, 4],
        [3, 4],
    ];
    COMBOS.get(index).copied()
}

fn colors_label<const N: usize>(colors: &[usize; N]) -> String {
    colors
        .iter()
        .map(|idx| color_short(*idx))
        .collect::<Vec<_>>()
        .join(" ")
}

fn short_card_label(card: &CardInput) -> String {
    format!(
        "T{} {} {}P",
        card.tier.unwrap_or(0),
        card.bonus_color
            .as_ref()
            .and_then(|color| COLORS.iter().position(|known| known == color))
            .map(color_short)
            .unwrap_or("?"),
        card.points.unwrap_or(0)
    )
}

fn short_card_reason(card: &CardInput) -> String {
    format!(
        "{} cost {}",
        short_card_label(card),
        card_cost_vector(card).iter().sum::<i64>()
    )
}

#[allow(dead_code)]
fn label_action_id(action: i64) -> String {
    match action {
        0..=11 => format!("购买 T{} 第{}张", action / 4 + 1, action % 4 + 1),
        12..=23 => {
            let slot = action - 12;
            format!("预定 T{} 第{}张", slot / 4 + 1, slot % 4 + 1)
        }
        24..=26 => format!("预定 T{} 牌堆", action - 23),
        27..=29 => format!("购买预定牌 {}", action - 26),
        30..=39 => "拿三种宝石".to_string(),
        40..=49 => "拿两种宝石".to_string(),
        50..=54 => "拿一种宝石".to_string(),
        55..=59 => "拿两个同色宝石".to_string(),
        _ => "执行最高价值合法动作".to_string(),
    }
}

unsafe fn apply_splendor_snapshot_to_dinoboard(
    api: &DinoBoardApi,
    session: *mut c_void,
    snapshot: &Value,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let schema = snapshot
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema != "gemhud-dinoboard-splendor-public-snapshot-v1" {
        return Err(format!("unsupported snapshot schema: {schema}"));
    }
    let players = snapshot
        .get("players")
        .and_then(Value::as_array)
        .ok_or_else(|| "snapshot missing players".to_string())?;
    let num_players = snapshot
        .get("num_players")
        .and_then(value_to_i64)
        .unwrap_or(players.len() as i64)
        .clamp(2, 4) as usize;
    if players.len() < num_players {
        return Err(format!(
            "snapshot has {} player records but num_players is {num_players}",
            players.len()
        ));
    }

    let card_pool_json = owned_string((api.splendor_card_pool_json)(), api.string_free);
    let card_pool: Vec<DinoCardDef> = serde_json::from_str(&card_pool_json)
        .map_err(|err| format!("invalid DinoBoard card pool JSON: {err}"))?;
    let nobles_json = owned_string((api.splendor_nobles_json)(), api.string_free);
    let noble_defs: Vec<DinoNobleDef> = serde_json::from_str(&nobles_json)
        .map_err(|err| format!("invalid DinoBoard nobles JSON: {err}"))?;
    let mut used_cards = HashSet::new();

    set_int_field(
        api,
        session,
        "current_player",
        &[],
        snapshot
            .get("current_player")
            .and_then(value_to_i64)
            .unwrap_or(0)
            .clamp(0, num_players as i64 - 1),
    )?;
    set_int_field(
        api,
        session,
        "first_player",
        &[],
        snapshot
            .get("first_player")
            .and_then(value_to_i64)
            .unwrap_or(0)
            .clamp(0, num_players as i64 - 1),
    )?;
    set_int_field(
        api,
        session,
        "plies",
        &[],
        snapshot
            .get("plies")
            .and_then(value_to_i64)
            .unwrap_or(0)
            .max(0),
    )?;
    set_int_field(api, session, "final_round_remaining", &[], -1)?;
    set_int_field(
        api,
        session,
        "stage",
        &[],
        snapshot
            .get("stage")
            .and_then(value_to_i64)
            .unwrap_or(0)
            .clamp(0, 2),
    )?;
    set_int_field(
        api,
        session,
        "pending_returns",
        &[],
        snapshot
            .get("pending_returns")
            .and_then(value_to_i64)
            .unwrap_or(0)
            .max(0),
    )?;
    set_int_field(api, session, "pending_nobles_size", &[], 0)?;
    for slot in 0..(num_players + 1) {
        set_int_field(api, session, "pending_noble_slots", &[slot], -1)?;
    }
    set_int_field(api, session, "winner", &[], -1)?;
    set_int_field(api, session, "terminal", &[], 0)?;
    set_int_field(api, session, "shared_victory", &[], 0)?;

    let bank = value_i64_array(snapshot.get("bank"), 6);
    for color in 0..6 {
        set_int_field(api, session, "bank", &[color], bank[color])?;
    }

    let market = snapshot
        .get("market")
        .and_then(Value::as_array)
        .ok_or_else(|| "snapshot missing market".to_string())?;
    for tier_idx in 0..3 {
        let row = market
            .get(tier_idx)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let row_len = row.len().min(4);
        set_int_field(api, session, "tableau_size", &[tier_idx], row_len as i64)?;
        for slot in 0..4 {
            let cid = if slot < row_len {
                match_card_def(
                    row.get(slot).unwrap(),
                    Some(tier_idx),
                    &card_pool,
                    &mut used_cards,
                )?
            } else {
                -1
            };
            set_int_field(api, session, "tableau", &[tier_idx, slot], cid)?;
        }
    }

    let deck_sizes = value_i64_array(snapshot.get("deck_sizes"), 3);
    for tier_idx in 0..3 {
        set_int_field(
            api,
            session,
            "deck_sizes",
            &[tier_idx],
            deck_sizes[tier_idx].max(0),
        )?;
    }

    let empty_reserved = Vec::new();
    for player_idx in 0..num_players {
        let player = &players[player_idx];
        let points = player
            .get("points")
            .and_then(value_to_i64)
            .unwrap_or(0)
            .max(0);
        set_int_field(api, session, "scores", &[player_idx], 0)?;
        set_int_field(api, session, "player_points", &[player_idx], points)?;
        set_int_field(
            api,
            session,
            "player_cards_count",
            &[player_idx],
            player
                .get("cards_count")
                .and_then(value_to_i64)
                .unwrap_or(0)
                .max(0),
        )?;
        set_int_field(
            api,
            session,
            "player_nobles_count",
            &[player_idx],
            player
                .get("nobles_count")
                .and_then(value_to_i64)
                .unwrap_or(0)
                .max(0),
        )?;

        let tokens = value_i64_array(player.get("tokens"), 6);
        for color in 0..6 {
            set_int_field(
                api,
                session,
                "player_gems",
                &[player_idx, color],
                tokens[color],
            )?;
        }
        let bonuses = value_i64_array(player.get("bonuses"), 5);
        for color in 0..5 {
            set_int_field(
                api,
                session,
                "player_bonuses",
                &[player_idx, color],
                bonuses[color],
            )?;
        }

        let reserved = player
            .get("reserved")
            .and_then(Value::as_array)
            .unwrap_or(&empty_reserved);
        let reserved_size = reserved.len().min(3);
        set_int_field(
            api,
            session,
            "reserved_size",
            &[player_idx],
            reserved_size as i64,
        )?;
        for slot in 0..3 {
            let mut cid = -1;
            let mut visible = false;
            if slot < reserved_size {
                let reserved_card = &reserved[slot];
                visible = reserved_card
                    .get("visible")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if snapshot_card_has_identity(reserved_card) {
                    cid = match_card_def(reserved_card, None, &card_pool, &mut used_cards)?;
                } else {
                    warnings.push(format!(
                        "reserved card p{} slot{} is hidden; DinoBoard keeps it unknown",
                        player_idx + 1,
                        slot + 1
                    ));
                }
            }
            set_int_field(api, session, "reserved", &[player_idx, slot], cid)?;
            set_int_field(
                api,
                session,
                "reserved_visible",
                &[player_idx, slot],
                if visible { 1 } else { 0 },
            )?;
            for viewer in 0..num_players {
                let can_see = cid >= 0 && (visible || viewer == player_idx);
                set_viz_field(
                    api,
                    session,
                    "reserved",
                    &[player_idx, slot],
                    viewer,
                    can_see,
                )?;
            }
        }
    }

    let empty_nobles = Vec::new();
    let nobles = snapshot
        .get("nobles")
        .and_then(Value::as_array)
        .unwrap_or(&empty_nobles);
    let noble_count = nobles.len().min(num_players + 1);
    set_int_field(api, session, "nobles_size", &[], noble_count as i64)?;
    for slot in 0..(num_players + 1) {
        let nid = if slot < noble_count {
            match_noble_def(&nobles[slot], &noble_defs)?
        } else {
            -1
        };
        set_int_field(api, session, "nobles", &[slot], nid)?;
    }

    rebuild_views(api, session)?;
    Ok(())
}

unsafe fn set_int_field(
    api: &DinoBoardApi,
    session: *mut c_void,
    field: &str,
    indices: &[usize],
    value: i64,
) -> Result<(), String> {
    let field_c =
        CString::new(field).map_err(|_| format!("field name contains NUL byte: {field}"))?;
    let indices_c: Vec<c_int> = indices.iter().map(|idx| *idx as c_int).collect();
    let mut err: *mut c_char = ptr::null_mut();
    let ok = (api.session_set_int_field)(
        session,
        field_c.as_ptr(),
        if indices_c.is_empty() {
            ptr::null()
        } else {
            indices_c.as_ptr()
        },
        indices_c.len() as c_int,
        value as c_int,
        &mut err,
    );
    if ok == 0 {
        Err(take_error(err, api.string_free))
    } else {
        Ok(())
    }
}

unsafe fn set_viz_field(
    api: &DinoBoardApi,
    session: *mut c_void,
    field: &str,
    indices: &[usize],
    viewer: usize,
    visible: bool,
) -> Result<(), String> {
    let field_c =
        CString::new(field).map_err(|_| format!("field name contains NUL byte: {field}"))?;
    let indices_c: Vec<c_int> = indices.iter().map(|idx| *idx as c_int).collect();
    let mut err: *mut c_char = ptr::null_mut();
    let ok = (api.session_set_viz_field)(
        session,
        field_c.as_ptr(),
        if indices_c.is_empty() {
            ptr::null()
        } else {
            indices_c.as_ptr()
        },
        indices_c.len() as c_int,
        viewer as c_int,
        if visible { 1 } else { 0 },
        &mut err,
    );
    if ok == 0 {
        Err(take_error(err, api.string_free))
    } else {
        Ok(())
    }
}

unsafe fn rebuild_views(api: &DinoBoardApi, session: *mut c_void) -> Result<(), String> {
    let mut err: *mut c_char = ptr::null_mut();
    let ok = (api.session_rebuild_views)(session, &mut err);
    if ok == 0 {
        Err(take_error(err, api.string_free))
    } else {
        Ok(())
    }
}

fn match_card_def(
    card: &Value,
    tier_index: Option<usize>,
    card_pool: &[DinoCardDef],
    used_cards: &mut HashSet<i64>,
) -> Result<i64, String> {
    let tier = snapshot_card_tier(card, tier_index)?;
    let points = card
        .get("points")
        .and_then(value_to_i64)
        .unwrap_or(0)
        .max(0);
    let color = card
        .get("bonus_color")
        .and_then(Value::as_str)
        .ok_or_else(|| "snapshot card missing bonus_color".to_string())?;
    let bonus = bonus_index(color).ok_or_else(|| {
        format!("snapshot card uses non-base bonus color '{color}', cannot map to DinoBoard")
    })?;
    let cost = value_i64_array(card.get("cost"), 5);

    let mut fallback_used = None;
    for def in card_pool {
        if def.tier == tier && def.bonus == bonus && def.points == points && def.cost == cost {
            if !used_cards.contains(&def.id) {
                used_cards.insert(def.id);
                return Ok(def.id);
            }
            fallback_used = Some(def.id);
        }
    }
    if let Some(id) = fallback_used {
        return Ok(id);
    }
    Err(format!(
        "could not map snapshot card tier={tier} bonus={bonus} points={points} cost={cost:?}"
    ))
}

fn match_noble_def(noble: &Value, noble_defs: &[DinoNobleDef]) -> Result<i64, String> {
    let requirements = value_i64_array(noble.get("requirements"), 5);
    noble_defs
        .iter()
        .find(|def| def.requirements == requirements)
        .map(|def| def.id)
        .ok_or_else(|| format!("could not map noble requirements={requirements:?}"))
}

fn snapshot_card_tier(card: &Value, tier_index: Option<usize>) -> Result<i64, String> {
    if let Some(tier_index) = tier_index {
        return Ok(tier_index as i64 + 1);
    }
    let raw = card
        .get("tier")
        .and_then(value_to_i64)
        .ok_or_else(|| "snapshot card missing tier".to_string())?;
    if (1..=3).contains(&raw) {
        Ok(raw)
    } else if (0..=2).contains(&raw) {
        Ok(raw + 1)
    } else {
        Err(format!("snapshot card has invalid tier {raw}"))
    }
}

fn snapshot_card_has_identity(card: &Value) -> bool {
    card.get("bonus_color").and_then(Value::as_str).is_some()
        && card.get("cost").and_then(Value::as_array).is_some()
        && card.get("tier").and_then(value_to_i64).is_some()
}

fn bonus_index(color: &str) -> Option<i64> {
    match color.trim().to_ascii_lowercase().as_str() {
        "white" => Some(0),
        "blue" => Some(1),
        "green" => Some(2),
        "red" => Some(3),
        "black" => Some(4),
        _ => None,
    }
}

fn append_snapshot_warnings(snapshot: &Value, warnings: &mut Vec<String>) {
    if snapshot
        .get("supported")
        .and_then(Value::as_bool)
        .unwrap_or(true)
        == false
    {
        warnings.push(
            "Mapped BGA state is not base-Splendor-only; expansion rules are outside the current DinoBoard base model."
                .to_string(),
        );
    }
    if let Some(items) = snapshot.get("warnings").and_then(Value::as_array) {
        for item in items.iter().filter_map(Value::as_str).take(4) {
            warnings.push(format!("BGA snapshot: {item}"));
        }
    }
}

fn score_card_with_snapshot(card: &CardInput, snapshot: &Value) -> CardValue {
    let mut scored = score_card(card);
    let current = snapshot
        .get("current_player")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as usize;
    let Some(player) = current_snapshot_player(snapshot) else {
        scored
            .reasons
            .push("snapshot missing active player".to_string());
        return scored;
    };

    let gems = value_i64_array(player.get("tokens"), 6);
    let bonuses = value_i64_array(player.get("bonuses"), 5);
    let cost = card_cost_vector(card);
    let (deficit, gold_used) = payment_deficit(&cost, &bonuses, &gems);
    let bank = value_i64_array(snapshot.get("bank"), 6);
    let self_status = purchase_status_for_player(card, player, &bank, Some(current));
    let opponent_status = best_opponent_purchase_status(card, snapshot, &bank, current);
    let self_noble = noble_route_score_for_bonuses(card, snapshot, &bonuses);
    let opponent_noble = best_opponent_noble_route_score(card, snapshot, current);
    let cost_total: i64 = cost.iter().sum();
    let points = card.points.unwrap_or(0).max(0);

    scored.method = "bga-state-aware-heuristic-v1".to_string();
    scored.confidence = scored.confidence.max(0.9);
    if self_status.can_buy_now {
        scored.value += 0.17;
        if gold_used > 0 {
            scored
                .reasons
                .push(format!("buyable now using {gold_used} gold"));
        } else {
            scored.reasons.push("buyable now".to_string());
        }
    } else if self_status.turns_to_buy == 1 {
        scored.value += 0.10;
        scored
            .reasons
            .push(format!("{deficit} token short; about 1 turn"));
    } else if self_status.turns_to_buy == 2 {
        scored.value += 0.06;
        scored
            .reasons
            .push(format!("{deficit} tokens short; about 2 turns"));
    } else {
        let penalty = (self_status.turns_to_buy as f64 * 0.025).min(0.16);
        scored.value -= penalty;
        scored.reasons.push(format!(
            "{deficit} tokens short; about {} turns",
            self_status.turns_to_buy
        ));
    }

    if let Some(status) = &opponent_status {
        if status.can_buy_now {
            scored.value += 0.08;
            scored
                .reasons
                .push("opponent can buy now +0.5 pressure".to_string());
        } else if status.turns_to_buy <= 1 {
            scored.value += 0.05;
            scored
                .reasons
                .push("opponent can reach in 1 turn".to_string());
        }
        if status.turns_to_buy < self_status.turns_to_buy {
            scored.value += 0.07;
            scored
                .reasons
                .push("opponent reaches earlier +0.5 contest".to_string());
        } else if self_status.turns_to_buy < status.turns_to_buy {
            scored.value += 0.04;
            scored.reasons.push("we reach earlier".to_string());
        }
    }

    if self_noble > 0.0 {
        scored.value += self_noble * 0.12;
        scored
            .reasons
            .push(format!("helps our noble route +{self_noble:.2}"));
    }
    if opponent_noble > 0.0 {
        scored.value += opponent_noble * 0.09;
        scored
            .reasons
            .push(format!("blocks opponent noble route +{opponent_noble:.2}"));
    }
    if points >= 3 && cost_total <= 7 {
        scored.value += 0.10;
        scored
            .reasons
            .push("high points for low cost +1.0".to_string());
    } else if points > 0 && cost_total > 0 {
        scored.value += ((points as f64 / cost_total as f64) * 0.10).min(0.08);
        scored.reasons.push("prestige efficiency".to_string());
    }

    scored.value = clamp_f64(scored.value, 0.0, 1.0);
    scored.label = value_label(scored.value).to_string();
    scored.self_status = Some(self_status);
    scored.opponent_status = opponent_status;
    scored.reasons.truncate(10);
    scored
}

fn current_snapshot_player(snapshot: &Value) -> Option<&Value> {
    let current = snapshot
        .get("current_player")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as usize;
    snapshot
        .get("players")
        .and_then(Value::as_array)
        .and_then(|players| players.get(current))
}

fn value_i64_array(value: Option<&Value>, len: usize) -> Vec<i64> {
    let mut out = vec![0; len];
    if let Some(items) = value.and_then(Value::as_array) {
        for (idx, item) in items.iter().take(len).enumerate() {
            out[idx] = value_to_i64(item).unwrap_or(0).max(0);
        }
    }
    out
}

fn card_cost_vector(card: &CardInput) -> Vec<i64> {
    COLORS
        .iter()
        .map(|color| {
            card.cost
                .get(*color)
                .and_then(value_to_i64)
                .unwrap_or(0)
                .max(0)
        })
        .collect()
}

fn payment_deficit(cost: &[i64], bonuses: &[i64], gems: &[i64]) -> (i64, i64) {
    let mut deficit = 0;
    let mut gold_left = gems.get(5).copied().unwrap_or(0).max(0);
    let mut gold_used = 0;
    for color in 0..COLORS.len() {
        let need = (cost.get(color).copied().unwrap_or(0)
            - bonuses.get(color).copied().unwrap_or(0))
        .max(0);
        let short = (need - gems.get(color).copied().unwrap_or(0)).max(0);
        let covered = short.min(gold_left);
        gold_left -= covered;
        gold_used += covered;
        deficit += short - covered;
    }
    (deficit, gold_used)
}

fn payment_gap_by_color(cost: &[i64], bonuses: &[i64], gems: &[i64]) -> (Vec<i64>, i64) {
    let mut deficits: Vec<i64> = (0..COLORS.len())
        .map(|color| {
            let need = (cost.get(color).copied().unwrap_or(0)
                - bonuses.get(color).copied().unwrap_or(0))
            .max(0);
            (need - gems.get(color).copied().unwrap_or(0)).max(0)
        })
        .collect();
    let mut gold_left = gems.get(5).copied().unwrap_or(0).max(0);
    let mut gold_used = 0;
    while gold_left > 0 {
        let Some((idx, amount)) = deficits
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|(_, amount)| *amount)
        else {
            break;
        };
        if amount <= 0 {
            break;
        }
        deficits[idx] -= 1;
        gold_left -= 1;
        gold_used += 1;
    }
    (deficits, gold_used)
}

fn purchase_status_for_player(
    card: &CardInput,
    player: &Value,
    bank: &[i64],
    player_index: Option<usize>,
) -> CardPurchaseStatus {
    let cost = card_cost_vector(card);
    let bonuses = value_i64_array(player.get("bonuses"), 5);
    let gems = value_i64_array(player.get("tokens"), 6);
    let (deficits, gold_used) = payment_gap_by_color(&cost, &bonuses, &gems);
    let token_deficit: i64 = deficits.iter().sum();
    let turns_to_buy = min_turns_to_cover_deficits(&deficits, bank);
    let can_buy_now = token_deficit == 0;
    CardPurchaseStatus {
        can_buy_now,
        turns_to_buy,
        token_deficit,
        gold_used,
        player_index,
        label: if can_buy_now {
            "now".to_string()
        } else {
            format!("{turns_to_buy}T / -{token_deficit}")
        },
    }
}

fn min_turns_to_cover_deficits(deficits: &[i64], _bank: &[i64]) -> i64 {
    let total: i64 = deficits.iter().sum();
    if total <= 0 {
        return 0;
    }
    // Conservative display estimate: one action can cover up to three different
    // colors, but we do not assume repeated two-of-a-kind takes for one color.
    // After one same-color take the bank usually drops below four, and future
    // refills depend on other players.
    let mut turns = (total + 2) / 3;
    for deficit in deficits.iter().copied().filter(|deficit| *deficit > 0) {
        turns = turns.max(deficit);
    }
    turns.max(1)
}

fn best_opponent_purchase_status(
    card: &CardInput,
    snapshot: &Value,
    bank: &[i64],
    current: usize,
) -> Option<CardPurchaseStatus> {
    snapshot
        .get("players")
        .and_then(Value::as_array)?
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != current)
        .map(|(idx, player)| purchase_status_for_player(card, player, bank, Some(idx)))
        .min_by_key(|status| (status.turns_to_buy, status.token_deficit))
}

fn noble_route_score_for_bonuses(card: &CardInput, snapshot: &Value, bonuses: &[i64]) -> f64 {
    let Some(color) = card
        .bonus_color
        .as_ref()
        .and_then(|color| COLORS.iter().position(|known| known == color))
    else {
        return 0.0;
    };
    snapshot
        .get("nobles")
        .and_then(Value::as_array)
        .map(|nobles| {
            nobles
                .iter()
                .map(|noble| {
                    let req = value_i64_array(noble.get("requirements"), 5);
                    if req.get(color).copied().unwrap_or(0)
                        <= bonuses.get(color).copied().unwrap_or(0)
                    {
                        return 0.0;
                    }
                    let before: i64 = req
                        .iter()
                        .enumerate()
                        .map(|(idx, needed)| {
                            (needed - bonuses.get(idx).copied().unwrap_or(0)).max(0)
                        })
                        .sum();
                    if before <= 0 {
                        return 0.0;
                    }
                    let after = before - 1;
                    if after == 0 {
                        1.0
                    } else if before <= 3 {
                        0.75
                    } else if before <= 5 {
                        0.45
                    } else {
                        0.20
                    }
                })
                .fold(0.0, f64::max)
        })
        .unwrap_or(0.0)
}

fn best_opponent_noble_route_score(card: &CardInput, snapshot: &Value, current: usize) -> f64 {
    snapshot
        .get("players")
        .and_then(Value::as_array)
        .map(|players| {
            players
                .iter()
                .enumerate()
                .filter(|(idx, _)| *idx != current)
                .map(|(_, player)| {
                    let bonuses = value_i64_array(player.get("bonuses"), 5);
                    noble_route_score_for_bonuses(card, snapshot, &bonuses)
                })
                .fold(0.0, f64::max)
        })
        .unwrap_or(0.0)
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
        self_status: None,
        opponent_status: None,
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
            session_set_int_field: load_symbol(module, "dinoboard_session_set_int_field")?,
            session_set_viz_field: load_symbol(module, "dinoboard_session_set_viz_field")?,
            session_rebuild_views: load_symbol(module, "dinoboard_session_rebuild_views")?,
            session_decide_json: load_symbol(module, "dinoboard_session_decide_json")?,
            splendor_card_pool_json: load_symbol(module, "dinoboard_splendor_card_pool_json")?,
            splendor_nobles_json: load_symbol(module, "dinoboard_splendor_nobles_json")?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_color_deficit_counts_one_per_turn() {
        assert_eq!(
            min_turns_to_cover_deficits(&[7, 0, 0, 0, 0], &[4, 4, 4, 4, 4, 5]),
            7
        );
        assert_eq!(
            min_turns_to_cover_deficits(&[2, 0, 0, 0, 0], &[4, 4, 4, 4, 4, 5]),
            2
        );
    }

    #[test]
    fn mixed_color_deficit_allows_three_colors_per_turn() {
        assert_eq!(
            min_turns_to_cover_deficits(&[1, 1, 1, 0, 0], &[4, 4, 4, 4, 4, 5]),
            1
        );
        assert_eq!(
            min_turns_to_cover_deficits(&[2, 2, 2, 0, 0], &[4, 4, 4, 4, 4, 5]),
            2
        );
        assert_eq!(
            min_turns_to_cover_deficits(&[3, 3, 0, 0, 0], &[4, 4, 4, 4, 4, 5]),
            3
        );
    }
}
