use anyhow::{Context, Result};

use crate::config::{DataConfig, OptimizerConfig};
use crate::optimizer::{optimize_trades, OptimizeInput};
use crate::wolfram::WolframSessionConfig;
use crate::wolfram::data::fetch_close_series;

pub fn run_optimize(cfg: &OptimizerConfig, data: &DataConfig) -> Result<()> {
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

    println!(
        "Optimizer done: final_cash={:.4}, trades_used={}",
        res.final_cash, res.trades_used
    );

    Ok(())
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
