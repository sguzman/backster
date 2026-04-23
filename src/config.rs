use serde::Deserialize;
use std::path::PathBuf;

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
    Csv { path: PathBuf },
    Wolfram {
        /// Wolfram Language expression that evaluates to a 2D table exportable to CSV.
        expr: String,
        /// Where to write the exported CSV on disk.
        output_csv: PathBuf,
        /// Optional kernel path/name (defaults to `WolframKernel`).
        kernel: Option<String>,
    },
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

