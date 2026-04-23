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
    pub strategy: StrategyConfig,
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
