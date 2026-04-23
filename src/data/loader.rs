use anyhow::Result;
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

        let returns = Series::new("returns", rets);
        let mut out_df = df.clone();
        out_df.with_column(returns)?;
        Ok(out_df)
    }
}
