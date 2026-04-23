use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};

use crate::backtest::Bar;
use crate::wolfram::{WolframSession, WolframSessionConfig};

#[derive(Debug, Clone)]
pub struct RollingFitRow {
    pub ts: DateTime<Utc>,
    pub normal_p: Option<f64>,
    pub normal_value: Option<f64>,
    pub student_t_p: Option<f64>,
    pub student_t_value: Option<f64>,
    pub laplace_p: Option<f64>,
    pub laplace_value: Option<f64>,
    pub logistic_p: Option<f64>,
    pub logistic_value: Option<f64>,
    pub cauchy_p: Option<f64>,
    pub cauchy_value: Option<f64>,
}

pub fn fetch_close_series(
    cfg: &WolframSessionConfig,
    symbol: &str,
    start: &str,
    end: &str,
    field: &str,
    resolution: &str,
) -> Result<Vec<f64>> {
    let bars = fetch_close_bars(cfg, symbol, start, end, field, resolution)?;
    Ok(bars.into_iter().map(|b| b.close).collect())
}

pub fn fetch_expr_close_series(cfg: &WolframSessionConfig, expr: &str) -> Result<Vec<f64>> {
    let (bars, _) = fetch_expr_close_bars_with_rolling_fit(cfg, expr, 0)?;
    Ok(bars.into_iter().map(|b| b.close).collect())
}

pub fn fetch_close_bars(
    cfg: &WolframSessionConfig,
    symbol: &str,
    start: &str,
    end: &str,
    field: &str,
    resolution: &str,
) -> Result<Vec<Bar>> {
    let (bars, _) =
        fetch_close_bars_with_rolling_fit(cfg, symbol, start, end, field, resolution, 0)?;
    Ok(bars)
}

fn wl_string_lit(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('\"', "\\\""))
}

fn math1_script_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("math1.wls")
}

pub fn fetch_close_bars_with_rolling_fit(
    cfg: &WolframSessionConfig,
    symbol: &str,
    start: &str,
    end: &str,
    field: &str,
    resolution: &str,
    window: usize,
) -> Result<(Vec<Bar>, Vec<Option<RollingFitRow>>)> {
    anyhow::ensure!(
        resolution.eq_ignore_ascii_case("day") || resolution.eq_ignore_ascii_case("daily"),
        "Unsupported resolution for WolframFinancial: {resolution} (only \"day\" supported currently)"
    );
    anyhow::ensure!(window == 0 || window >= 8, "window must be 0 or >= 8");

    let mut sess = WolframSession::connect(cfg.clone())?;
    sess.load_file(&math1_script_path())
        .context("Failed to load scripts/math1.wls into Wolfram kernel")?;
    let wl = format!(
        "BacksterFinancialDataCloseAndFitJSON[{},{},{},{},{}]",
        wl_string_lit(symbol),
        wl_string_lit(start),
        wl_string_lit(end),
        wl_string_lit(field),
        window
    );
    let json = sess.eval_to_string_expr(&wl)?;

    #[derive(serde::Deserialize)]
    struct Payload {
        close: Vec<(f64, f64)>,
        fit: Vec<Vec<Option<f64>>>,
    }

    let payload: Payload =
        serde_json::from_str(&json).context("Failed to parse JSON payload from Wolfram")?;

    anyhow::ensure!(!payload.close.is_empty(), "Wolfram returned no close rows");

    let mut bars = Vec::with_capacity(payload.close.len());
    let mut index_by_ms = std::collections::HashMap::<i64, usize>::new();
    for (i, (t_ms, close)) in payload.close.into_iter().enumerate() {
        anyhow::ensure!(close.is_finite(), "Non-finite close from Wolfram");
        let ts_i64 = t_ms.round() as i64;
        let ts: DateTime<Utc> = Utc
            .timestamp_millis_opt(ts_i64)
            .single()
            .context("Bad unix millis timestamp from Wolfram")?;
        index_by_ms.insert(ts_i64, i);
        bars.push(Bar {
            ts,
            open: close,
            high: close,
            low: close,
            close,
            volume: 0.0,
        });
    }

    let mut fits: Vec<Option<RollingFitRow>> = vec![None; bars.len()];
    for row in payload.fit {
        // [ts_ms, pN, vN, pT, vT, pL, vL, pLog, vLog, pC, vC]
        anyhow::ensure!(
            row.len() == 11,
            "Bad fit row length: expected 11, got {}",
            row.len()
        );
        let Some(ts_ms_f) = row[0] else {
            continue;
        };
        let ts_ms = ts_ms_f.round() as i64;
        let Some(&idx) = index_by_ms.get(&ts_ms) else {
            continue;
        };
        let ts = bars[idx].ts;
        fits[idx] = Some(RollingFitRow {
            ts,
            normal_p: row[1],
            normal_value: row[2],
            student_t_p: row[3],
            student_t_value: row[4],
            laplace_p: row[5],
            laplace_value: row[6],
            logistic_p: row[7],
            logistic_value: row[8],
            cauchy_p: row[9],
            cauchy_value: row[10],
        });
    }

    Ok((bars, fits))
}

pub fn fetch_expr_close_bars_with_rolling_fit(
    cfg: &WolframSessionConfig,
    expr: &str,
    window: usize,
) -> Result<(Vec<Bar>, Vec<Option<RollingFitRow>>)> {
    anyhow::ensure!(window == 0 || window >= 8, "window must be 0 or >= 8");

    let mut sess = WolframSession::connect(cfg.clone())?;
    sess.load_file(&math1_script_path())
        .context("Failed to load scripts/math1.wls into Wolfram kernel")?;
    let wl = format!(
        "BacksterExprCloseAndFitJSON[{},{}]",
        wl_string_lit(expr),
        window
    );
    let json = sess.eval_to_string_expr(&wl)?;

    #[derive(serde::Deserialize)]
    struct Payload {
        close: Vec<(f64, f64)>,
        fit: Vec<Vec<Option<f64>>>,
    }

    let payload: Payload =
        serde_json::from_str(&json).context("Failed to parse JSON payload from Wolfram")?;

    anyhow::ensure!(!payload.close.is_empty(), "Wolfram returned no close rows");

    let mut bars = Vec::with_capacity(payload.close.len());
    let mut index_by_ms = std::collections::HashMap::<i64, usize>::new();
    for (i, (t_ms, close)) in payload.close.into_iter().enumerate() {
        anyhow::ensure!(close.is_finite(), "Non-finite close from Wolfram");
        let ts_i64 = t_ms.round() as i64;
        let ts: DateTime<Utc> = Utc
            .timestamp_millis_opt(ts_i64)
            .single()
            .context("Bad unix millis timestamp from Wolfram")?;
        index_by_ms.insert(ts_i64, i);
        bars.push(Bar {
            ts,
            open: close,
            high: close,
            low: close,
            close,
            volume: 0.0,
        });
    }

    let mut fits: Vec<Option<RollingFitRow>> = vec![None; bars.len()];
    for row in payload.fit {
        anyhow::ensure!(
            row.len() == 11,
            "Bad fit row length: expected 11, got {}",
            row.len()
        );
        let Some(ts_ms_f) = row[0] else {
            continue;
        };
        let ts_ms = ts_ms_f.round() as i64;
        let Some(&idx) = index_by_ms.get(&ts_ms) else {
            continue;
        };
        let ts = bars[idx].ts;
        fits[idx] = Some(RollingFitRow {
            ts,
            normal_p: row[1],
            normal_value: row[2],
            student_t_p: row[3],
            student_t_value: row[4],
            laplace_p: row[5],
            laplace_value: row[6],
            logistic_p: row[7],
            logistic_value: row[8],
            cauchy_p: row[9],
            cauchy_value: row[10],
        });
    }

    Ok((bars, fits))
}
