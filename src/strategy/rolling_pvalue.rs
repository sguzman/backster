use anyhow::Result;

use crate::backtest::{BacktestContext, Bar};
use crate::stats::rolling_fit::compute_window_fit;

use super::Strategy;

pub struct RollingPvaluePredictor {
    window: usize,
    enter_threshold: f64,
    exit_threshold: f64,
    normalize_weights: bool,
    min_total_weight: f64,
    closes: Vec<f64>,
}

impl RollingPvaluePredictor {
    pub fn new(
        window: usize,
        enter_threshold: f64,
        exit_threshold: f64,
        normalize_weights: bool,
        min_total_weight: f64,
    ) -> Self {
        Self {
            window,
            enter_threshold,
            exit_threshold,
            normalize_weights,
            min_total_weight,
            closes: Vec::new(),
        }
    }

    fn log_returns(&self) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.closes.len().saturating_sub(1));
        for w in self.closes.windows(2) {
            let a = w[0];
            let b = w[1];
            if a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite() {
                out.push((b.ln() - a.ln()) as f64);
            }
        }
        out
    }
}

impl Strategy for RollingPvaluePredictor {
    fn name(&self) -> &'static str {
        "RollingPvaluePredictor"
    }

    fn on_bar(&mut self, ctx: &mut BacktestContext, bar: &Bar) -> Result<()> {
        self.closes.push(bar.close);
        if self.closes.len() < self.window + 2 {
            return Ok(());
        }

        let rets = self.log_returns();
        if rets.len() < self.window {
            return Ok(());
        }
        let sample = &rets[rets.len() - self.window..];

        let fit = compute_window_fit(sample, self.normalize_weights)?;
        if fit.total_weight < self.min_total_weight {
            return Ok(());
        }

        let predicted_log_return = fit.weighted_value;
        let predicted_next_close = bar.close * predicted_log_return.exp();

        if ctx.position.is_none() {
            if predicted_next_close > bar.close * (1.0 + self.enter_threshold) {
                let qty = ctx.cash / bar.close;
                if qty > 0.0 {
                    ctx.enter_long(qty, bar.close, bar.ts)?;
                }
            }
        } else if predicted_next_close < bar.close * (1.0 - self.exit_threshold) {
            ctx.exit(bar.close, bar.ts)?;
        }

        Ok(())
    }
}

