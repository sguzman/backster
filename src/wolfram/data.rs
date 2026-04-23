use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};

use crate::backtest::Bar;
use crate::wolfram::{WolframSession, WolframSessionConfig};

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

pub fn fetch_close_bars(
    cfg: &WolframSessionConfig,
    symbol: &str,
    start: &str,
    end: &str,
    field: &str,
    resolution: &str,
) -> Result<Vec<Bar>> {
    anyhow::ensure!(
        resolution.eq_ignore_ascii_case("day")
            || resolution.eq_ignore_ascii_case("daily"),
        "Unsupported resolution for WolframFinancial: {resolution} (only \"day\" supported currently)"
    );

    let mut sess = WolframSession::connect(cfg.clone())?;

    // Pull a TimeSeries from FinancialData and emit JSON rows: [[unixMillis, close], ...]
    // This keeps the Mathematica footprint minimal (data only).
    let wl = build_financialdata_json_expr(symbol, start, end, field)?;
    let json = sess.eval_to_string_expr(&wl)?;

    // JSON is of the form [[t, v], [t, v], ...]
    #[derive(serde::Deserialize)]
    struct Row(f64, f64);

    let rows: Vec<Row> = serde_json::from_str(&json).context("Failed to parse JSON from Wolfram")?;
    anyhow::ensure!(!rows.is_empty(), "Wolfram returned no rows");

    let mut out = Vec::with_capacity(rows.len());
    for Row(t_ms, close) in rows {
        anyhow::ensure!(close.is_finite(), "Non-finite close from Wolfram");
        let ts_i64 = t_ms.round() as i64;
        let ts: DateTime<Utc> = Utc
            .timestamp_millis_opt(ts_i64)
            .single()
            .context("Bad unix millis timestamp from Wolfram")?;
        out.push(Bar {
            ts,
            open: close,
            high: close,
            low: close,
            close,
            volume: 0.0,
        });
    }

    Ok(out)
}

fn build_financialdata_json_expr(symbol: &str, start: &str, end: &str, field: &str) -> Result<String> {
    // Basic escaping for WL string literals.
    fn esc(s: &str) -> String {
        s.replace('\\', "\\\\").replace('\"', "\\\"")
    }

    let symbol = esc(symbol);
    let start = esc(start);
    let end = esc(end);
    let field = esc(field);

    // Use DateObject["YYYY-MM-DD"] to avoid locale issues.
    // Convert DateObject -> UnixTime (seconds) -> millis.
    Ok(format!(
        "Module[{{ts, rows}}, \
          ts = TimeConstrained[Quiet@Check[FinancialData[\"{symbol}\", {{DateObject[\"{start}\"], DateObject[\"{end}\"]}}, \"{field}\"], $Failed], 20, $Failed]; \
          If[ts === $Failed, Return[ExportString[{{}}, \"JSON\"]]]; \
          rows = Normal[ts]; \
          rows = Select[rows, MatchQ[#, {{_DateObject, _?NumericQ}}] &]; \
          rows = rows /. {{d_DateObject, v_}} :> {{1000*UnixTime[d], N[v]}}; \
          ExportString[rows, \"JSON\"]\
        ]"
    ))
}
