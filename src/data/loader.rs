use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use polars::prelude::*;
use std::path::Path;

pub struct DataLoader;

impl DataLoader {
    pub fn load_csv<P: AsRef<Path>>(path: P) -> Result<DataFrame> {
        let df = CsvReadOptions::default()
            .with_has_header(true)
            .try_into_reader_with_file_path(Some(path.as_ref().to_path_buf()))?
            .finish()?;
        Ok(df)
    }

    pub fn to_returns(df: &DataFrame, column: &str) -> Result<DataFrame> {
        let series = df.column(column)?.cast(&DataType::Float64)?;
        let prices = series.f64()?;

        let mut rets: Vec<Option<f64>> = Vec::with_capacity(prices.len());
        let mut prev_log: Option<f64> = None;
        for p in prices.into_iter() {
            match p {
                Some(p) => {
                    let lp = p.ln();
                    let r = prev_log.map(|prev| lp - prev);
                    rets.push(r);
                    prev_log = Some(lp);
                }
                None => {
                    rets.push(None);
                    prev_log = None;
                }
            }
        }

        let returns = Series::new("returns".into(), rets);
        let mut out_df = df.clone();
        out_df.with_column(returns.into())?;
        Ok(out_df)
    }

    pub fn get_f64(col: &Column, idx: usize) -> Result<f64> {
        let s = col.as_materialized_series();
        match s.dtype() {
            DataType::Float64 => s
                .f64()
                .context("Expected Float64")?
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("Null at row {idx}")),
            DataType::Float32 => Ok(s
                .f32()
                .context("Expected Float32")?
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("Null at row {idx}"))? as f64),
            DataType::Int64 => Ok(s
                .i64()
                .context("Expected Int64")?
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("Null at row {idx}"))? as f64),
            DataType::UInt64 => Ok(s
                .u64()
                .context("Expected UInt64")?
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("Null at row {idx}"))? as f64),
            DataType::Int32 => Ok(s
                .i32()
                .context("Expected Int32")?
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("Null at row {idx}"))? as f64),
            DataType::UInt32 => Ok(s
                .u32()
                .context("Expected UInt32")?
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("Null at row {idx}"))? as f64),
            other => anyhow::bail!("Unsupported numeric dtype for get_f64: {other:?}"),
        }
    }

    pub fn get_ts(col: &Column, idx: usize) -> Result<DateTime<Utc>> {
        let s = col.as_materialized_series();
        match s.dtype() {
            DataType::Datetime(tu, _) => {
                let v = s
                    .datetime()
                    .context("Expected Datetime")?
                    .phys
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("Null timestamp at row {idx}"))?;
                Ok(match tu {
                    TimeUnit::Nanoseconds => Utc.timestamp_nanos(v),
                    TimeUnit::Microseconds => Utc.timestamp_micros(v).single().context("Bad ts")?,
                    TimeUnit::Milliseconds => Utc
                        .timestamp_millis_opt(v)
                        .single()
                        .context("Bad ts")?,
                })
            }
            DataType::Int64 => {
                let v = s
                    .i64()
                    .context("Expected Int64")?
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("Null timestamp at row {idx}"))?;
                parse_epoch(v)
            }
            DataType::UInt64 => {
                let v = s
                    .u64()
                    .context("Expected UInt64")?
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("Null timestamp at row {idx}"))?;
                parse_epoch(v as i64)
            }
            DataType::String => {
                let v = s
                    .str()
                    .context("Expected String")?
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("Null timestamp at row {idx}"))?;
                let dt = DateTime::parse_from_rfc3339(v)
                    .context("Failed parsing RFC3339 timestamp")?;
                Ok(dt.with_timezone(&Utc))
            }
            other => anyhow::bail!("Unsupported timestamp dtype: {other:?}"),
        }
    }
}

fn parse_epoch(v: i64) -> Result<DateTime<Utc>> {
    // Heuristic: seconds, millis, micros, nanos.
    let av = v.unsigned_abs() as u64;
    let dt = if av < 10_000_000_000 {
        // seconds
        Utc.timestamp_opt(v, 0).single()
    } else if av < 10_000_000_000_000 {
        // millis
        Utc.timestamp_millis_opt(v).single()
    } else if av < 10_000_000_000_000_000 {
        // micros
        Utc.timestamp_micros(v).single()
    } else {
        // nanos
        Some(Utc.timestamp_nanos(v))
    };
    dt.context("Invalid epoch timestamp")
}
