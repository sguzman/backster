use anyhow::Result;

use crate::backtest::{BacktestContext, Bar};
use crate::wolfram::data::RollingFitRow;

use super::Strategy;

pub struct RollingPvaluePredictor {
    window: usize,
    enter_threshold: f64,
    exit_threshold: f64,
    normalize_weights: bool,
    min_total_weight: f64,
    fits: Vec<Option<RollingFitRow>>,
    idx: usize,
}

impl RollingPvaluePredictor {
    pub fn new(
        window: usize,
        enter_threshold: f64,
        exit_threshold: f64,
        normalize_weights: bool,
        min_total_weight: f64,
        fits: Vec<Option<RollingFitRow>>,
    ) -> Self {
        Self {
            window,
            enter_threshold,
            exit_threshold,
            normalize_weights,
            min_total_weight,
            fits,
            idx: 0,
        }
    }
}

impl Strategy for RollingPvaluePredictor {
    fn name(&self) -> &'static str {
        "RollingPvaluePredictor"
    }

    fn on_bar(&mut self, ctx: &mut BacktestContext, bar: &Bar) -> Result<()> {
        if self.idx >= self.fits.len() {
            anyhow::bail!("Strategy received more bars than fit rows");
        }
        let fit = match &self.fits[self.idx] {
            Some(f) => f,
            None => {
                self.idx += 1;
                return Ok(());
            }
        };
        self.idx += 1;

        // Predictive value is a linear combination of the distribution "values"
        // (mean/median of the fitted return distribution) weighted by p-values.
        let mut total_weight = 0.0;
        let mut weighted = 0.0;
        for (p, v) in [
            (fit.normal_p, fit.normal_value),
            (fit.student_t_p, fit.student_t_value),
            (fit.laplace_p, fit.laplace_value),
            (fit.logistic_p, fit.logistic_value),
            (fit.cauchy_p, fit.cauchy_value),
        ] {
            let p = p
                .filter(|p| p.is_finite())
                .map(|p| p.max(0.0))
                .unwrap_or(0.0);
            let v = v.filter(|v| v.is_finite()).unwrap_or(0.0);
            total_weight += p;
            weighted += p * v;
        }
        if total_weight < self.min_total_weight {
            return Ok(());
        }

        let predicted_log_return = if self.normalize_weights && total_weight > 0.0 {
            weighted / total_weight
        } else {
            weighted
        };

        // Anchor is the current close; predictive is an additive step sized by expected log-return.
        let predicted_next_close = bar.close * (1.0 + predicted_log_return);

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
