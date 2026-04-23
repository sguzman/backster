use anyhow::{Context, Result};

use crate::backtest::{BacktestEngine, Bar};
use crate::config::{BacktestConfig, DataConfig, StrategyConfig};
use crate::strategy::Strategy;
use crate::strategy::rolling_pvalue::RollingPvaluePredictor;
use crate::wolfram::WolframSessionConfig;
use crate::wolfram::data::fetch_close_bars;

pub fn run_backtest(cfg: &BacktestConfig, data: &DataConfig) -> Result<()> {
    let bars = load_bars(data).context("Failed to load bars")?;

    let mut strategy: Box<dyn Strategy> = match &cfg.strategy {
        StrategyConfig::RollingPvaluePredictor {
            enter_threshold,
            exit_threshold,
            normalize_weights,
            min_total_weight,
        } => Box::new(RollingPvaluePredictor::new(
            cfg.window,
            *enter_threshold,
            *exit_threshold,
            *normalize_weights,
            *min_total_weight,
        )),
    };

    if bars.is_empty() {
        anyhow::bail!("No bars loaded");
    }

    let engine = BacktestEngine::new(cfg.starting_cash);
    let out = engine.run(&bars, strategy.as_mut())?;
    let last_close = bars.last().map(|b| b.close).unwrap_or(0.0);

    println!(
        "Backtest done: trades={}, realized_pnl={:.4}, final_cash={:.4}, final_equity={:.4}",
        out.stats.trades,
        out.stats.realized_pnl,
        out.cash,
        out.equity(last_close)
    );

    Ok(())
}

fn load_bars(data: &DataConfig) -> Result<Vec<Bar>> {
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
            fetch_close_bars(&kcfg, symbol, start, end, field, resolution)
        }
    }
}
