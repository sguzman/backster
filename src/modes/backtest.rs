use anyhow::{Context, Result};
use std::path::Path;

use crate::backtest::{BacktestEngine, Bar};
use crate::config::{BacktestConfig, DataConfig, StrategyConfig};
use crate::data::loader::DataLoader;
use crate::strategy::Strategy;
use crate::strategy::rolling_pvalue::RollingPvaluePredictor;
use crate::wolfram::{WolframSession, WolframSessionConfig};

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
        DataConfig::Csv { path } => load_bars_from_csv(path),
        DataConfig::Wolfram {
            expr,
            output_csv,
            kernel,
        } => {
            export_csv_from_wolfram(expr, output_csv, kernel.as_deref())?;
            load_bars_from_csv(output_csv)
        }
    }
}

fn export_csv_from_wolfram(expr: &str, output_csv: &Path, kernel: Option<&str>) -> Result<()> {
    let mut cfg = WolframSessionConfig::default();
    if let Some(k) = kernel {
        cfg.kernel = k.to_string();
    }
    let mut sess = WolframSession::connect(cfg)?;

    let path = output_csv
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Non-UTF8 output_csv path not supported"))?;
    let escaped = path.replace('\\', "\\\\").replace('\"', "\\\"");
    let code = format!("Export[\"{escaped}\", ({expr}), \"CSV\"]");

    let res = sess.eval_to_string(&code)?;
    if res == "$Failed" {
        anyhow::bail!("Wolfram export returned $Failed");
    }
    Ok(())
}

fn load_bars_from_csv(path: &Path) -> Result<Vec<Bar>> {
    // Expect a standard OHLCV bar file. Minimal requirement is `ts` and `close`.
    let df = DataLoader::load_csv(path)?;

    let ts_col = df
        .column("ts")
        .or_else(|_| df.column("ts_event"))
        .context("Missing `ts` or `ts_event` column")?;
    let close_col = df.column("close").context("Missing `close` column")?;

    let open_col = df.column("open").ok();
    let high_col = df.column("high").ok();
    let low_col = df.column("low").ok();
    let volume_col = df.column("volume").ok();

    let mut out = Vec::with_capacity(df.height());
    for i in 0..df.height() {
        let ts = DataLoader::get_ts(ts_col, i).context("Bad timestamp")?;
        let close = DataLoader::get_f64(close_col, i).context("Bad close")?;
        let open = open_col
            .as_ref()
            .and_then(|c| DataLoader::get_f64(c, i).ok())
            .unwrap_or(close);
        let high = high_col
            .as_ref()
            .and_then(|c| DataLoader::get_f64(c, i).ok())
            .unwrap_or(close);
        let low = low_col
            .as_ref()
            .and_then(|c| DataLoader::get_f64(c, i).ok())
            .unwrap_or(close);
        let volume = volume_col
            .as_ref()
            .and_then(|c| DataLoader::get_f64(c, i).ok())
            .unwrap_or(0.0);

        out.push(Bar {
            ts,
            open,
            high,
            low,
            close,
            volume,
        });
    }

    Ok(out)
}
