use anyhow::{Context, Result};

use crate::backtest::{BacktestEngine, BacktestLogConfig, Bar};
use crate::config::{BacktestConfig, DataConfig, StrategyConfig};
use crate::strategy::Strategy;
use crate::strategy::rolling_pvalue::RollingPvaluePredictor;
use crate::wolfram::WolframSessionConfig;
use crate::wolfram::data::fetch_close_bars_with_rolling_fit;
use tracing::info;

pub fn run_backtest(cfg: &BacktestConfig, data: &DataConfig) -> Result<()> {
    let (bars, fits) = load_bars_and_features(cfg, data).context("Failed to load data/features")?;

    let mut strategy: Box<dyn Strategy> = match &cfg.strategy {
        StrategyConfig::RollingPvaluePredictor {
            enter_threshold,
            exit_threshold,
            normalize_weights,
            min_total_weight,
            force_trade_each_bar,
        } => Box::new(RollingPvaluePredictor::new(
            cfg.window,
            cfg.holding_period_bars,
            *enter_threshold,
            *exit_threshold,
            *normalize_weights,
            *min_total_weight,
            *force_trade_each_bar,
            fits,
        )),
    };

    if bars.is_empty() {
        anyhow::bail!("No bars loaded");
    }

    let engine = BacktestEngine::new(
        cfg.starting_cash,
        BacktestLogConfig {
            log_bars: cfg.log_bars,
            log_trades: cfg.log_trades,
            log_strategy: cfg.log_bars,
            trade_resolution: cfg.trade_resolution.clone(),
        },
    );
    let out = engine.run(&bars, strategy.as_mut(), cfg.seed)?;
    let last_close = bars.last().map(|b| b.close).unwrap_or(0.0);
    let final_equity = out.equity(last_close);
    let pct_return = if cfg.starting_cash.abs() > 0.0 {
        100.0 * (final_equity - cfg.starting_cash) / cfg.starting_cash
    } else {
        0.0
    };

    info!(
        seed = out.seed,
        starting_cash = cfg.starting_cash,
        trades = out.stats.trades,
        realized_pnl = out.stats.realized_pnl,
        final_cash = out.cash,
        final_equity = final_equity,
        return_pct = pct_return,
        "Backtest done"
    );

    Ok(())
}

fn load_bars_and_features(
    cfg: &BacktestConfig,
    data: &DataConfig,
) -> Result<(Vec<Bar>, Vec<Option<crate::wolfram::data::RollingFitRow>>)> {
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
            match cfg.strategy {
                StrategyConfig::RollingPvaluePredictor { .. } => fetch_close_bars_with_rolling_fit(
                    &kcfg,
                    symbol,
                    start,
                    end,
                    field,
                    resolution,
                    cfg.window,
                ),
            }
        }
        DataConfig::WolframExpr { expr, kernel } => {
            let mut kcfg = WolframSessionConfig::default();
            if let Some(k) = kernel {
                kcfg.kernel = k.to_string();
            }
            match cfg.strategy {
                StrategyConfig::RollingPvaluePredictor { .. } => crate::wolfram::data::fetch_expr_close_bars_with_rolling_fit(
                    &kcfg,
                    expr,
                    cfg.window,
                ),
            }
        }
    }
}
