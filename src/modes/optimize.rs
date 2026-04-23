use anyhow::{Context, Result};

use crate::config::{DataConfig, OptimizerConfig};
use crate::data::loader::DataLoader;
use crate::optimizer::{optimize_trades, OptimizeInput};
use crate::wolfram::{WolframSession, WolframSessionConfig};

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
        DataConfig::Csv { path } => {
            let df = DataLoader::load_csv(path)?;
            let close_col = df.column("close").context("Missing `close` column")?;
            let mut out = Vec::with_capacity(df.height());
            for i in 0..df.height() {
                out.push(DataLoader::get_f64(close_col, i)?);
            }
            Ok(out)
        }
        DataConfig::Wolfram {
            expr,
            output_csv,
            kernel,
        } => {
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

            let df = DataLoader::load_csv(output_csv)?;
            let close_col = df.column("close").context("Missing `close` column")?;
            let mut out = Vec::with_capacity(df.height());
            for i in 0..df.height() {
                out.push(DataLoader::get_f64(close_col, i)?);
            }
            Ok(out)
        }
    }
}

