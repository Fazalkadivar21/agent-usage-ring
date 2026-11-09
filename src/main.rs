//! claude-usage-ring: polls Claude's OAuth usage endpoint and writes a small
//! JSON snapshot that a Quickshell widget watches.
//!
//! The access token is read from ~/.claude/.credentials.json on every poll
//! (Claude Code refreshes that file itself). The token is never logged.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA_HEADER: &str = "oauth-2025-04-20";
const DEFAULT_INTERVAL_S: u64 = 90;
const MIN_INTERVAL_S: u64 = 60;
const MAX_BACKOFF_S: u64 = 900;
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------- CLI

#[derive(Debug, Clone)]
enum Mode {
    Live,
    /// Cycle through a list of canned scenarios.
    MockCycle,
    /// Fixed remaining percentages (5h, 7d).
    MockFixed(f64, f64),
}

#[derive(Debug, Clone)]
struct Config {
    mode: Mode,
    output: PathBuf,
    credentials: PathBuf,
    interval: u64,
    once: bool,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn default_output() -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".cache"));
    cache.join("claude-usage").join("usage.json")
}

fn default_credentials() -> PathBuf {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"));
    dir.join(".credentials.json")
}

const USAGE: &str = "\
claude-usage-ring - Claude usage limits -> JSON for a Quickshell bar widget

USAGE:
    claude-usage-ring [OPTIONS]

OPTIONS:
    -o, --output <PATH>        JSON output file
                               (default: $XDG_CACHE_HOME/claude-usage/usage.json)
    -c, --credentials <PATH>   Claude credentials file
                               (default: ~/.claude/.credentials.json)
    -i, --interval <SECS>      Poll interval (default 90, minimum 60 for live mode)
        --once                 Fetch once, write the file, print the JSON, exit
        --mock                 Cycle through fake scenarios (no network); default
                               interval 4s. Exercises colour thresholds + stale state
        --mock-values <A,B>    Fixed fake remaining percentages: 5-hour,weekly
    -h, --help                 Show this help
";

fn parse_args() -> Result<Config, String> {
    let mut mode = Mode::Live;
    let mut output = None;
    let mut credentials = None;
    let mut interval: Option<u64> = None;
    let mut once = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "-o" | "--output" => output = Some(PathBuf::from(value(&arg)?)),
            "-c" | "--credentials" => credentials = Some(PathBuf::from(value(&arg)?)),
            "-i" | "--interval" => {
                interval = Some(value(&arg)?.parse().map_err(|_| "invalid --interval".to_string())?)
            }
            "--once" => once = true,
            "--mock" => mode = Mode::MockCycle,
            "--mock-values" => {
                let v = value(&arg)?;
                let (a, b) = v.split_once(',').ok_or("--mock-values expects A,B")?;
                let a: f64 = a.trim().parse().map_err(|_| "bad --mock-values")?;
                let b: f64 = b.trim().parse().map_err(|_| "bad --mock-values")?;
                mode = Mode::MockFixed(a.clamp(0.0, 100.0), b.clamp(0.0, 100.0));
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}\n\n{USAGE}")),
        }
    }

    let interval = match (&mode, interval) {
        (Mode::Live, Some(i)) if i < MIN_INTERVAL_S => {
            log(&format!("interval {i}s too low, clamping to {MIN_INTERVAL_S}s"));
            MIN_INTERVAL_S
        }
        (Mode::Live, Some(i)) => i,
        (Mode::Live, None) => DEFAULT_INTERVAL_S,
        (_, Some(i)) => i.max(1),
        (_, None) => 4,
    };

    Ok(Config {
        mode,
        output: output.unwrap_or_else(default_output),
        credentials: credentials.unwrap_or_else(default_credentials),
        interval,
        once,
    })
}

fn log(msg: &str) {
    eprintln!("[{}] {msg}", Utc::now().format("%Y-%m-%dT%H:%M:%SZ"));
}

// ---------------------------------------------------------------- API types

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<OAuth>,
}

#[derive(Deserialize)]
struct OAuth {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    /// Unix milliseconds.
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct ApiWindow {
    /// Percent USED (0-100).
    utilization: Option<f64>,
    resets_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Debug)]
struct ApiUsage {
    five_hour: Option<ApiWindow>,
    seven_day: Option<ApiWindow>,
}

// ---------------------------------------------------------------- output types

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Window {
    used_pct: f64,
    remaining_pct: f64,
    resets_at: Option<DateTime<Utc>>,
}

impl Window {
    fn from_api(w: &ApiWindow) -> Option<Self> {
        let used = w.utilization?.clamp(0.0, 100.0);
        Some(Window { used_pct: used, remaining_pct: 100.0 - used, resets_at: w.resets_at })
    }
    fn from_remaining(remaining: f64, resets_at: DateTime<Utc>) -> Self {
        Window { used_pct: 100.0 - remaining, remaining_pct: remaining, resets_at: Some(resets_at) }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Snapshot {
    schema: u32,
    source: String,
    /// "ok" (fresh), "stale" (showing older data after an error), "error" (no data at all).
    status: String,
    error: Option<String>,
    /// When the data in five_hour/seven_day was fetched.
    fetched_at: Option<DateTime<Utc>>,
    /// When this file was written (daemon heartbeat).
    updated_at: DateTime<Utc>,
    next_poll_at: DateTime<Utc>,
    poll_interval_s: u64,
    five_hour: Option<Window>,
    seven_day: Option<Window>,
}

// ---------------------------------------------------------------- fetching

enum FetchError {
    /// Back off at least this long (from Retry-After).
    RateLimited(String, Option<u64>),
    Other(String),
}

impl FetchError {
    fn message(&self) -> &str {
        match self {
            FetchError::RateLimited(m, _) | FetchError::Other(m) => m,
        }
    }
}

async fn read_token(path: &Path) -> Result<String, FetchError> {
    let raw = tokio::fs::read(path)
        .await
        .map_err(|e| FetchError::Other(format!("cannot read {}: {e}", path.display())))?;
    let creds: Credentials = serde_json::from_slice(&raw)
        .map_err(|e| FetchError::Other(format!("cannot parse credentials file: {e}")))?;
    let oauth = creds
        .oauth
        .ok_or_else(|| FetchError::Other("no claudeAiOauth entry (not logged in with a Claude subscription?)".into()))?;
    if let Some(exp) = oauth.expires_at {
        if exp <= Utc::now().timestamp_millis() {
            return Err(FetchError::Other(
                "OAuth token expired; run Claude Code once so it refreshes the token".into(),
            ));
        }
    }
    oauth
        .access_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| FetchError::Other("credentials file has no accessToken".into()))
}

async fn fetch(client: &reqwest::Client, cfg: &Config) -> Result<(Window, Window), FetchError> {
    let token = read_token(&cfg.credentials).await?;
    let resp = client
        .get(USAGE_URL)
        .bearer_auth(&token)
        .header("anthropic-beta", BETA_HEADER)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| FetchError::Other(format!("network error: {}", e.without_url())))?;
    drop(token);

    let status = resp.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        return Err(FetchError::RateLimited("rate limited by API (HTTP 429)".into(), retry));
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(FetchError::Other(format!(
            "auth rejected (HTTP {}); token may need a refresh by Claude Code",
            status.as_u16()
        )));
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(160).collect();
        return Err(FetchError::Other(format!("HTTP {}: {snippet}", status.as_u16())));
    }

    let usage: ApiUsage = resp
        .json()
        .await
        .map_err(|e| FetchError::Other(format!("unexpected response shape: {e}")))?;
    let five = usage.five_hour.as_ref().and_then(Window::from_api);
    let seven = usage.seven_day.as_ref().and_then(Window::from_api);
    match (five, seven) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(FetchError::Other("response missing five_hour/seven_day utilization".into())),
    }
}

// ---------------------------------------------------------------- mock

/// (5h remaining, 7d remaining, simulate error)
const MOCK_SCENARIOS: &[(f64, f64, bool)] = &[
    (100.0, 100.0, false),
    (80.0, 62.0, false),
    (51.0, 49.0, false), // around the green/yellow boundary
    (35.0, 26.0, false),
    (24.0, 18.0, false), // orange
    (11.0, 9.0, false),  // orange / red
    (6.0, 3.0, false),   // red
    (0.0, 0.0, false),
    (64.0, 30.0, true), // stale
];

fn mock_windows(rem5: f64, rem7: f64) -> (Window, Window) {
    let now = Utc::now();
    (
        Window::from_remaining(rem5, now + ChronoDuration::minutes(133)),
        Window::from_remaining(rem7, now + ChronoDuration::hours(76)),
    )
}

// ---------------------------------------------------------------- output

async fn write_atomic(path: &Path, snap: &Snapshot) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let mut body = serde_json::to_vec_pretty(snap).expect("serializable");
    body.push(b'\n');
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, &body).await?;
    tokio::fs::rename(&tmp, path).await
}

async fn load_previous(path: &Path) -> Option<Snapshot> {
    let raw = tokio::fs::read(path).await.ok()?;
    let snap: Snapshot = serde_json::from_slice(&raw).ok()?;
    (snap.schema == SCHEMA_VERSION && snap.source == "api" && snap.fetched_at.is_some()).then_some(snap)
}

// ---------------------------------------------------------------- main loop

struct State {
    last_good: Option<(Window, Window, DateTime<Utc>)>,
    failures: u32,
    mock_index: usize,
}

/// Returns the snapshot to write and the delay until the next poll.
async fn tick(client: &reqwest::Client, cfg: &Config, st: &mut State) -> (Snapshot, u64) {
    let now = Utc::now();
    let source = if matches!(cfg.mode, Mode::Live) { "api" } else { "mock" };

    let result: Result<(Window, Window), FetchError> = match cfg.mode {
        Mode::Live => fetch(client, cfg).await,
        Mode::MockFixed(a, b) => Ok(mock_windows(a, b)),
        Mode::MockCycle => {
            let (a, b, fail) = MOCK_SCENARIOS[st.mock_index % MOCK_SCENARIOS.len()];
            st.mock_index += 1;
            if fail {
                // keep previous windows but report an error -> stale
                st.last_good = Some((mock_windows(a, b).0, mock_windows(a, b).1, now - ChronoDuration::minutes(7)));
                Err(FetchError::Other("mock: simulated network error".into()))
            } else {
                Ok(mock_windows(a, b))
            }
        }
    };

    let (status, error, delay) = match result {
        Ok((five, seven)) => {
            if st.failures > 0 && matches!(cfg.mode, Mode::Live) {
                log("fetch recovered");
            }
            st.failures = 0;
            st.last_good = Some((five, seven, now));
            ("ok", None, cfg.interval)
        }
        Err(e) => {
            st.failures += 1;
            let mut delay = if matches!(cfg.mode, Mode::Live) {
                let factor = 1u64 << (st.failures - 1).min(4);
                (cfg.interval * factor).min(MAX_BACKOFF_S)
            } else {
                cfg.interval
            };
            if let FetchError::RateLimited(_, Some(retry)) = &e {
                delay = delay.max(*retry).min(3600);
            }
            log(&format!("fetch failed ({}): {}; next try in {delay}s", st.failures, e.message()));
            let status = if st.last_good.is_some() { "stale" } else { "error" };
            (status, Some(e.message().to_string()), delay)
        }
    };

    let (five, seven, fetched_at) = match &st.last_good {
        Some((a, b, t)) => (Some(a.clone()), Some(b.clone()), Some(*t)),
        None => (None, None, None),
    };

    let snap = Snapshot {
        schema: SCHEMA_VERSION,
        source: source.into(),
        status: status.into(),
        error,
        fetched_at,
        updated_at: now,
        next_poll_at: now + ChronoDuration::seconds(delay as i64),
        poll_interval_s: cfg.interval,
        five_hour: five,
        seven_day: seven,
    };
    (snap, delay)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cfg = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    let client = reqwest::Client::builder()
        .user_agent(concat!("claude-usage-ring/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("http client");

    let mut st = State { last_good: None, failures: 0, mock_index: 0 };

    // Seed with the last good live value so a restart without network still shows data (as stale).
    if matches!(cfg.mode, Mode::Live) {
        if let Some(prev) = load_previous(&cfg.output).await {
            if let (Some(a), Some(b), Some(t)) = (prev.five_hour, prev.seven_day, prev.fetched_at) {
                st.last_good = Some((a, b, t));
            }
        }
    }

    log(&format!(
        "starting: mode={:?} interval={}s output={}",
        cfg.mode,
        cfg.interval,
        cfg.output.display()
    ));

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("sigterm handler");

    loop {
        let (snap, delay) = tick(&client, &cfg, &mut st).await;
        if let Err(e) = write_atomic(&cfg.output, &snap).await {
            log(&format!("cannot write {}: {e}", cfg.output.display()));
        }
        if cfg.once {
            println!("{}", serde_json::to_string_pretty(&snap).unwrap());
            std::process::exit(if snap.status == "ok" { 0 } else { 1 });
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(delay)) => {}
            _ = tokio::signal::ctrl_c() => { log("interrupted, exiting"); break; }
            _ = sigterm.recv() => { log("SIGTERM, exiting"); break; }
        }
    }
}
