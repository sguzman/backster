use anyhow::Result;

use crate::config::{DataConfig, OptimizerConfig};
use crate::optimizer::{optimize_trades, OptimizeInput};
use crate::wolfram::WolframSessionConfig;
use crate::wolfram::data::fetch_close_series;
use tracing::info;

pub fn run_optimize(cfg: &OptimizerConfig, data: &DataConfig, quiet: bool) -> Result<f64> {
    if !cfg.know_future {
        anyhow::bail!("optimize mode with know_future=false is not implemented yet");
    }

    let closes = load_close_series(data)?;
    let res = optimize_trades(OptimizeInput {
        prices: &closes,
        starting_cash: cfg.starting_cash,
        max_trades: cfg.max_trades,
        allow_long: cfg.allow_long,
        allow_short: cfg.allow_short,
    })?;

    let pct_return = if cfg.starting_cash.abs() > 0.0 {
        100.0 * (res.final_cash - cfg.starting_cash) / cfg.starting_cash
    } else {
        0.0
    };

    if !quiet {
        info!(
            final_cash = res.final_cash,
            trades_used = res.trades_used,
            return_pct = pct_return,
            "Optimizer done"
        );
    }

    Ok(pct_return)
}

fn load_close_series(data: &DataConfig) -> Result<Vec<f64>> {
    match data {
        DataConfig::WolframFinancial {
            symbol,
            start,
            end,
            field,
            resolution,
            kernel,
        } => {
            let mut kcfg = WolframSessionConfig::default();
            if let Some(k) = kernel {
                kcfg.kernel = k.to_string();
            }
            fetch_close_series(&kcfg, symbol, start, end, field, resolution)
        }
        DataConfig::WolframExpr { expr, kernel } => {
            let mut kcfg = WolframSessionConfig::default();
            if let Some(k) = kernel {
                kcfg.kernel = k.to_string();
            }
            crate::wolfram::data::fetch_expr_close_series(&kcfg, expr)
        }
    }
}
