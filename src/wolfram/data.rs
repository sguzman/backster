use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};

use crate::backtest::Bar;
use crate::wolfram::{WolframSession, WolframSessionConfig};

fn unescape_wolfram_stringish(s: &str) -> String {
    // When Wolfram returns a string through certain WSTP paths, it can come back
    // in an InputForm-like escaped representation (e.g. `\012` for LF, `\"` for `"`).
    // Decode the subset we observe so the payload becomes valid JSON again.
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        let Some(n) = it.peek().copied() else {
            out.push('\\');
            break;
        };

        match n {
            '"' => {
                it.next();
                out.push('"');
            }
            '\\' => {
                it.next();
                out.push('\\');
            }
            '0'..='7' => {
                let mut oct = String::new();
                for _ in 0..3 {
                    if let Some(d) = it.peek().copied() {
                        if matches!(d, '0'..='7') {
                            oct.push(d);
                            it.next();
                        } else {
                            break;
                        }
                    }
                }
                if let Ok(v) = u32::from_str_radix(&oct, 8) {
                    if let Some(c) = char::from_u32(v) {
                        out.push(c);
                    }
                }
            }
            _ => {
                // Leave unknown escapes as-is.
                out.push('\\');
                out.push(n);
                it.next();
            }
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct RollingFitRow {
    pub ts: DateTime<Utc>,
    pub normal_p: Option<f64>,
    pub normal_mu: Option<f64>,
    pub normal_sigma: Option<f64>,
    pub student_t_p: Option<f64>,
    pub student_t_nu: Option<f64>,
    pub student_t_mu: Option<f64>,
    pub student_t_sigma: Option<f64>,
    pub laplace_p: Option<f64>,
    pub laplace_mu: Option<f64>,
    pub laplace_sigma: Option<f64>,
    pub logistic_p: Option<f64>,
    pub logistic_mu: Option<f64>,
    pub logistic_beta: Option<f64>,
    pub cauchy_p: Option<f64>,
    pub cauchy_alpha: Option<f64>,
    pub cauchy_beta: Option<f64>,
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

    let script_bytes = crate::cache::read_file_bytes(&math1_script_path())?;
    let cache_key = crate::cache::blake3_hex(&[
        b"wolfram_financial_v1",
        symbol.as_bytes(),
        start.as_bytes(),
        end.as_bytes(),
        field.as_bytes(),
        resolution.as_bytes(),
        window.to_string().as_bytes(),
        &script_bytes,
    ]);

    let json = if let Some(hit) = crate::cache::read_cached_json(&cache_key)? {
        hit
    } else {
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
        let json = unescape_wolfram_stringish(&json);
        crate::cache::write_cached_json(&cache_key, &json)?;
        json
    };

    #[derive(serde::Deserialize)]
    struct Payload {
        close: Vec<(f64, f64)>,
        fit: Vec<Vec<Option<f64>>>,
    }

    let payload: Payload = serde_json::from_str(&json).with_context(|| {
        let prefix: String = json.chars().take(200).collect();
        format!("Failed to parse JSON payload from Wolfram (prefix={prefix:?})")
    })?;

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
        // [ts_ms,
        //   pN, muN, sigmaN,
        //   pT, nuT, muT, sigmaT,
        //   pL, muL, sigmaL,
        //   pLog, muLog, betaLog,
        //   pC, alphaC, betaC
        // ]
        anyhow::ensure!(
            row.len() == 17,
            "Bad fit row length: expected 17, got {}",
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
            normal_mu: row[2],
            normal_sigma: row[3],
            student_t_p: row[4],
            student_t_nu: row[5],
            student_t_mu: row[6],
            student_t_sigma: row[7],
            laplace_p: row[8],
            laplace_mu: row[9],
            laplace_sigma: row[10],
            logistic_p: row[11],
            logistic_mu: row[12],
            logistic_beta: row[13],
            cauchy_p: row[14],
            cauchy_alpha: row[15],
            cauchy_beta: row[16],
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

    let script_bytes = crate::cache::read_file_bytes(&math1_script_path())?;
    let expr_is_probably_random = {
        let e = expr.to_ascii_lowercase();
        e.contains("random")
            || e.contains("randomvari")
            || e.contains("randomreal")
            || e.contains("randominteger")
            || e.contains("randomchoice")
            || e.contains("randomvariat")
            || e.contains("seedrandom")
    };
    let cache_key = crate::cache::blake3_hex(&[
        b"wolfram_expr_v1",
        expr.as_bytes(),
        window.to_string().as_bytes(),
        &script_bytes,
    ]);

    let json = if !expr_is_probably_random {
        if let Some(hit) = crate::cache::read_cached_json(&cache_key)? {
            hit
        } else {
            let mut sess = WolframSession::connect(cfg.clone())?;
            sess.load_file(&math1_script_path())
                .context("Failed to load scripts/math1.wls into Wolfram kernel")?;
            let wl = format!("BacksterExprCloseAndFitJSON[{},{}]", wl_string_lit(expr), window);
            let json = sess.eval_to_string_expr(&wl)?;
            let json = unescape_wolfram_stringish(&json);
            crate::cache::write_cached_json(&cache_key, &json)?;
            json
        }
    } else {
        // Never cache expressions that probably include randomness.
        let mut sess = WolframSession::connect(cfg.clone())?;
        sess.load_file(&math1_script_path())
            .context("Failed to load scripts/math1.wls into Wolfram kernel")?;
        let wl = format!("BacksterExprCloseAndFitJSON[{},{}]", wl_string_lit(expr), window);
        let json = sess.eval_to_string_expr(&wl)?;
        unescape_wolfram_stringish(&json)
    };

    #[derive(serde::Deserialize)]
    struct Payload {
        close: Vec<(f64, f64)>,
        fit: Vec<Vec<Option<f64>>>,
    }

    let payload: Payload = serde_json::from_str(&json).with_context(|| {
        let prefix: String = json.chars().take(200).collect();
        format!("Failed to parse JSON payload from Wolfram (prefix={prefix:?})")
    })?;

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
            row.len() == 17,
            "Bad fit row length: expected 17, got {}",
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
            normal_mu: row[2],
            normal_sigma: row[3],
            student_t_p: row[4],
            student_t_nu: row[5],
            student_t_mu: row[6],
            student_t_sigma: row[7],
            laplace_p: row[8],
            laplace_mu: row[9],
            laplace_sigma: row[10],
            logistic_p: row[11],
            logistic_mu: row[12],
            logistic_beta: row[13],
            cauchy_p: row[14],
            cauchy_alpha: row[15],
            cauchy_beta: row[16],
        });
    }

    Ok((bars, fits))
}
