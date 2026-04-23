use anyhow::Result;
use polars::prelude::*;
use std::path::Path;
use std::fs::File;

pub struct DataLoader;

impl DataLoader {
    pub fn load_csv<P: AsRef<Path>>(path: P) -> Result<DataFrame> {
        let df = CsvReader::new(File::open(path)?)
            .has_header(true)
            .finish()?;
        Ok(df)
    }

    pub fn to_returns(df: &DataFrame, column: &str) -> Result<DataFrame> {
        // Calculate log returns
        let series = df.column(column)?;
        // ln() is used for natural log in Polars
        let log_prices = series.cast(&DataType::Float64)?.ln()?;
        // diff(n, null_behavior) - in 0.41 it might be just diff(n)
        let returns = log_prices.diff(1, NullBehavior::Ignore)?;
        
        let mut out_df = df.clone();
        out_df.with_column(Series::new("returns", returns))?;
        Ok(out_df)
    }
}
