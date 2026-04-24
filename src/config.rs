use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Backtest,
    Optimize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub mode: Mode,
    pub data: DataConfig,
    pub backtest: Option<BacktestConfig>,
    pub optimizer: Option<OptimizerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum DataConfig {
    WolframFinancial {
        /// Asset identifier passed to Wolfram `FinancialData`, e.g. `"AAPL"`, `"BTC/USD"`, etc.
        symbol: String,
        /// ISO date string, e.g. `"2020-01-01"`.
        start: String,
        /// ISO date string, e.g. `"2024-01-01"`.
        end: String,
        /// FinancialData property to retrieve (default: "Close").
        #[serde(default = "default_field")]
        field: String,
        /// Time resolution hint (currently only `"day"` is supported).
        #[serde(default = "default_resolution")]
        resolution: String,
        /// Optional kernel path/name (defaults to env `WOLFRAMKERNEL` or `WolframKernel`).
        kernel: Option<String>,
    },
    WolframExpr {
        /// Wolfram Language expression that evaluates to a `TimeSeries` or list of `{DateObject, value}`.
        expr: String,
        /// Optional kernel path/name (defaults to env `WOLFRAMKERNEL` or `WolframKernel`).
        kernel: Option<String>,
    },
}

fn default_field() -> String {
    "Close".to_string()
}

fn default_resolution() -> String {
    "day".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct BacktestConfig {
    pub starting_cash: f64,
    pub window: usize,
    #[serde(default = "default_trade_resolution")]
    pub trade_resolution: String,
    #[serde(default = "default_holding_period_bars")]
    pub holding_period_bars: usize,
    #[serde(default = "default_log_bars")]
    pub log_bars: bool,
    #[serde(default = "default_log_trades")]
    pub log_trades: bool,
    #[serde(default = "default_seed")]
    pub seed: u64,
    pub strategy: StrategyConfig,
}

fn default_trade_resolution() -> String {
    "bar".to_string()
}

fn default_holding_period_bars() -> usize {
    1
}

fn default_log_bars() -> bool {
    true
}

fn default_log_trades() -> bool {
    true
}

fn default_seed() -> u64 {
    1
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StrategyConfig {
    RollingPvaluePredictor {
        enter_threshold: f64,
        exit_threshold: f64,
        #[serde(default)]
        normalize_weights: bool,
        #[serde(default = "default_min_total_weight")]
        min_total_weight: f64,
        /// If true, forces exactly one round-trip per bar (exit then re-enter),
        /// yielding consistent "daily" trading when your data resolution is daily.
        #[serde(default)]
        force_trade_each_bar: bool,
    },
}

fn default_min_total_weight() -> f64 {
    0.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizerConfig {
    pub starting_cash: f64,
    pub max_trades: usize,
    #[serde(default)]
    pub allow_long: bool,
    #[serde(default)]
    pub allow_short: bool,
    #[serde(default)]
    pub know_future: bool,
}
