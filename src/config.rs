use serde::{Deserialize, Serialize};

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
    /// Each bar/day: draw `z ~ Normal(0, 1)` and invest `|z|` fraction of cash (clamped),
    /// going long if `z >= 0` and short if `z < 0`.
    NormalNoiseInvestor {
        /// Clamp the absolute fraction of cash invested each day. Must be in (0, 1].
        #[serde(default = "default_max_abs_fraction")]
        max_abs_fraction: f64,
        /// Skip entries when the invested cash would be below this threshold.
        #[serde(default = "default_min_trade_cash")]
        min_trade_cash: f64,
    },
    AdHocDistributionPredictor {
        enter_threshold: f64,
        exit_threshold: f64,
        #[serde(default)]
        normalize_weights: bool,
        #[serde(default = "default_min_total_weight")]
        min_total_weight: f64,
        #[serde(default)]
        force_trade_each_bar: bool,
        #[serde(default)]
        use_ad_test: bool,
    },
    AdHocNormalPredictor {
        enter_threshold: f64,
        exit_threshold: f64,
        #[serde(default)]
        normalize_weights: bool,
        #[serde(default = "default_min_total_weight")]
        min_total_weight: f64,
        #[serde(default)]
        force_trade_each_bar: bool,
        #[serde(default)]
        use_ad_test: bool,
    },
    FlexiblePipelinePredictor {
        enter_threshold: f64,
        exit_threshold: f64,
        #[serde(default)]
        force_trade_each_bar: bool,
        pipeline: Vec<PipelineStep>,
    },
}

fn default_min_total_weight() -> f64 {
    0.0
}

fn default_max_abs_fraction() -> f64 {
    1.0
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PipelineStep {
    LogReturns { 
        name: String, 
        window: usize 
    },
    FitDistributions { 
        name: String, 
        input: String, 
        families: Vec<DistFamilyConfig>, 
        #[serde(default)]
        test: TestKind 
    },
    Sample { 
        name: String, 
        input: String 
    },
    Aggregate { 
        name: String, 
        method: AggregationMethod, 
        values: String, 
        weights: Option<String> 
    },
    WolframEval {
        name: String,
        expr: String,
        input: Option<String>,
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TestKind {
    #[default]
    Ks,
    Ad,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum AggregationMethod {
    Mean,
    WeightedMean,
    Median,
    Sum,
    WeightedSum,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum DistFamilyConfig {
    Normal,
    StudentsT,
    Laplace,
    Logistic,
    Cauchy,
}

fn default_min_trade_cash() -> f64 {
    1.0
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
