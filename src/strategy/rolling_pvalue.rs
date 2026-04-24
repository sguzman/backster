use anyhow::Result;
use rand::Rng;
use rand_distr::{Distribution, Normal, StudentT};
use tracing::info;

use crate::backtest::{BacktestContext, Bar};
use crate::wolfram::data::RollingFitRow;

use super::Strategy;

pub struct RollingPvaluePredictor {
    window: usize,
    holding_period_bars: usize,
    enter_threshold: f64,
    exit_threshold: f64,
    normalize_weights: bool,
    min_total_weight: f64,
    force_trade_each_bar: bool,
    fits: Vec<Option<RollingFitRow>>,
    idx: usize,
    holding_bars: usize,
}

impl RollingPvaluePredictor {
    pub fn new(
        window: usize,
        holding_period_bars: usize,
        enter_threshold: f64,
        exit_threshold: f64,
        normalize_weights: bool,
        min_total_weight: f64,
        force_trade_each_bar: bool,
        fits: Vec<Option<RollingFitRow>>,
    ) -> Self {
        Self {
            window,
            holding_period_bars: holding_period_bars.max(1),
            enter_threshold,
            exit_threshold,
            normalize_weights,
            min_total_weight,
            force_trade_each_bar,
            fits,
            idx: 0,
            holding_bars: 0,
        }
    }
}

impl Strategy for RollingPvaluePredictor {
    fn name(&self) -> &'static str {
        "RollingPvaluePredictor"
    }

    fn on_bar(&mut self, ctx: &mut BacktestContext, bar: &Bar) -> Result<()> {
        // Current bar index (0-based). For WSTP-provided fits, fits.len() should match bars.len().
        let bar_index = self.idx;
        let is_last_bar = bar_index + 1 >= self.fits.len();

        // If we're in a position, enforce the configured holding period (unless in forced daily mode,
        // which closes positions every bar and re-opens based on the prediction).
        if !self.force_trade_each_bar {
            if ctx.position.is_some() {
                self.holding_bars += 1;
                if self.holding_bars >= self.holding_period_bars {
                    ctx.exit(bar.close, bar.ts)?;
                    self.holding_bars = 0;
                }
            }
        }

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
        // (random draws from the fitted log-return distribution) weighted by p-values.
        let mut total_weight = 0.0;
        let mut weighted = 0.0;

        let mut sample = |p: Option<f64>, v: Option<f64>| {
            let p = p
                .filter(|p| p.is_finite())
                .map(|p| p.max(0.0))
                .unwrap_or(0.0);
            let v = v.filter(|v| v.is_finite()).unwrap_or(0.0);
            total_weight += p;
            weighted += p * v;
        };

        let rng = ctx.rng();

        // Normal
        let normal_draw = match (fit.normal_mu, fit.normal_sigma) {
            (Some(mu), Some(sigma)) if sigma.is_finite() && sigma > 0.0 && mu.is_finite() => {
                Normal::new(mu, sigma).ok().map(|d| d.sample(rng))
            }
            _ => None,
        };
        sample(fit.normal_p, normal_draw);

        // StudentT
        let student_draw = match (fit.student_t_nu, fit.student_t_mu, fit.student_t_sigma) {
            (Some(nu), Some(mu), Some(sigma))
                if nu.is_finite()
                    && nu > 0.0
                    && mu.is_finite()
                    && sigma.is_finite()
                    && sigma > 0.0 =>
            {
                StudentT::new(nu)
                    .ok()
                    .map(|d| mu + sigma * d.sample(rng))
            }
            _ => None,
        };
        sample(fit.student_t_p, student_draw);

        // Laplace: inverse CDF
        let laplace_draw = match (fit.laplace_mu, fit.laplace_sigma) {
            (Some(mu), Some(b)) if mu.is_finite() && b.is_finite() && b > 0.0 => {
                let u: f64 = rng.random_range(0.0..1.0);
                let x = if u < 0.5 {
                    mu + b * (2.0 * u).ln()
                } else {
                    mu - b * (2.0 * (1.0 - u)).ln()
                };
                Some(x)
            }
            _ => None,
        };
        sample(fit.laplace_p, laplace_draw);

        // Logistic: inverse CDF
        let logistic_draw = match (fit.logistic_mu, fit.logistic_beta) {
            (Some(mu), Some(s)) if mu.is_finite() && s.is_finite() && s > 0.0 => {
                let u: f64 = rng.random_range(0.0..1.0);
                Some(mu + s * (u / (1.0 - u)).ln())
            }
            _ => None,
        };
        sample(fit.logistic_p, logistic_draw);

        // Cauchy
        let cauchy_draw = match (fit.cauchy_alpha, fit.cauchy_beta) {
            (Some(x0), Some(gamma)) if x0.is_finite() && gamma.is_finite() && gamma > 0.0 => {
                let u: f64 = rng.random_range(0.0..1.0);
                Some(x0 + gamma * (std::f64::consts::PI * (u - 0.5)).tan())
            }
            _ => None,
        };
        sample(fit.cauchy_p, cauchy_draw);

        if total_weight < self.min_total_weight {
            return Ok(());
        }

        let predicted_log_return = if self.normalize_weights && total_weight > 0.0 {
            weighted / total_weight
        } else {
            weighted
        };

        // Anchor is the current close; predictive is a 1-step ahead price using log-return.
        let predicted_next_close = bar.close * predicted_log_return.exp();

        if ctx.log_strategy() {
            info!(
                ts = %bar.ts.to_rfc3339(),
                close = bar.close,
                pred_lr = predicted_log_return,
                pred_next = predicted_next_close,
                total_weight = total_weight,
                weighted = weighted,
                normal_p = fit.normal_p,
                normal_x = normal_draw,
                student_t_p = fit.student_t_p,
                student_t_x = student_draw,
                laplace_p = fit.laplace_p,
                laplace_x = laplace_draw,
                logistic_p = fit.logistic_p,
                logistic_x = logistic_draw,
                cauchy_p = fit.cauchy_p,
                cauchy_x = cauchy_draw,
                "strategy_rolling_pvalue"
            );
        }

        if self.force_trade_each_bar {
            // Forced "daily" mode:
            // - Close any open position at this bar's close (realizes P&L for the last day).
            // - Open a new 1-bar position for the next day (except on the final bar),
            //   selecting LONG vs SHORT based on the predicted next close.
            if ctx.position.is_some() {
                ctx.exit(bar.close, bar.ts)?;
            }
            if !is_last_bar && ctx.position.is_none() {
                let qty = ctx.cash / bar.close;
                if qty > 0.0 {
                    if predicted_next_close >= bar.close {
                        ctx.enter_long(qty, bar.close, bar.ts)?;
                    } else {
                        ctx.enter_short(qty, bar.close, bar.ts)?;
                    }
                }
            }
            self.holding_bars = 0;
            return Ok(());
        }

        if ctx.position.is_none() {
            if predicted_next_close > bar.close * (1.0 + self.enter_threshold) {
                let qty = ctx.cash / bar.close;
                if qty > 0.0 {
                    ctx.enter_long(qty, bar.close, bar.ts)?;
                    self.holding_bars = 0;
                }
            }
        } else if predicted_next_close < bar.close * (1.0 - self.exit_threshold) {
            ctx.exit(bar.close, bar.ts)?;
            self.holding_bars = 0;
        }

        Ok(())
    }
}
