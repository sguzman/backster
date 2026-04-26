use std::collections::VecDeque;
use anyhow::Result;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use tracing::info;

use crate::backtest::{BacktestContext, Bar};
use crate::backtest::engine::Side;
use crate::strategy::Strategy;
use crate::stats::ks::ks_p_value;
use crate::stats::ad::ad_p_value;

use rando::distrs::{
    NormalFit, StudentTFit, LaplaceFit, LogisticFit, CauchyFit, 
    DistributionFit, FittedDistribution
};

pub struct AdHocDistributionPredictor {
    window_size: usize,
    holding_period: usize,
    enter_threshold: f64,
    exit_threshold: f64,
    normalize_weights: bool,
    min_total_weight: f64,
    force_trade_each_bar: bool,
    use_ad_test: bool,

    returns_window: VecDeque<f64>,
    rng: ChaCha8Rng,
    last_close: Option<f64>,
}

impl AdHocDistributionPredictor {
    pub fn new(
        window_size: usize,
        holding_period: usize,
        enter_threshold: f64,
        exit_threshold: f64,
        normalize_weights: bool,
        min_total_weight: f64,
        force_trade_each_bar: bool,
        use_ad_test: bool,
    ) -> Self {
        Self {
            window_size,
            holding_period,
            enter_threshold,
            exit_threshold,
            normalize_weights,
            min_total_weight,
            force_trade_each_bar,
            use_ad_test,
            returns_window: VecDeque::with_capacity(window_size),
            rng: ChaCha8Rng::seed_from_u64(42),
            last_close: None,
        }
    }
}

impl Strategy for AdHocDistributionPredictor {
    fn name(&self) -> &'static str {
        "AdHocDistributionPredictor"
    }

    fn on_start(&mut self, ctx: &mut BacktestContext) -> Result<()> {
        self.rng = ChaCha8Rng::seed_from_u64(ctx.seed);
        self.returns_window.clear();
        self.last_close = None;
        Ok(())
    }

    fn on_bar(&mut self, ctx: &mut BacktestContext, bar: &Bar) -> Result<()> {
        // 1. Update window
        if let Some(prev_close) = self.last_close {
            let lr = (bar.close / prev_close).ln();
            if lr.is_finite() {
                self.returns_window.push_back(lr);
                if self.returns_window.len() > self.window_size {
                    self.returns_window.pop_front();
                }
            }
        }
        self.last_close = Some(bar.close);

        if self.returns_window.len() < self.window_size {
            return Ok(());
        }

        let data: Vec<f64> = self.returns_window.iter().copied().collect();

        // 2. Perform fits and calculate p-values
        let mut weighted_sample = 0.0;
        let mut total_p = 0.0;

        // Note: rando's Fit::fit returns Result<FittedType>
        // We wrap them in Box<dyn FittedDistribution> for uniform processing if possible,
        // but simple manual calls are safer for type checking.
        
        macro_rules! process_dist {
            ($fit_type:ty) => {
                if let Ok(dist) = <$fit_type as DistributionFit>::fit(&data) {
                    let p = if self.use_ad_test {
                        ad_p_value(&data, |x| dist.cdf(x))?.1
                    } else {
                        ks_p_value(&data, |x| dist.cdf(x))?.1
                    };

                    if p.is_finite() && p > 0.0 {
                        let s = dist.sample();
                        if s.is_finite() {
                            weighted_sample += s * p;
                            total_p += p;
                        }
                    }
                }
            };
        }

        process_dist!(NormalFit);
        process_dist!(StudentTFit);
        process_dist!(LaplaceFit);
        process_dist!(LogisticFit);
        process_dist!(CauchyFit);

        if total_p < self.min_total_weight || total_p == 0.0 {
            return Ok(());
        }

        let predicted_lr = if self.normalize_weights {
            weighted_sample / total_p
        } else {
            weighted_sample
        };

        // 3. Trading logic
        // We use a fixed qty of 1.0 for now, or based on available cash.
        // Let's use 1.0 share if price allows, or better, invest available cash.
        let trade_qty = (ctx.cash / bar.close).floor();

        if self.force_trade_each_bar {
            if ctx.position.is_some() {
                ctx.exit(bar.close, bar.ts)?;
            }
            if trade_qty > 0.0 {
                if predicted_lr > self.enter_threshold {
                    ctx.enter_long(trade_qty, bar.close, bar.ts)?;
                } else if predicted_lr < -self.enter_threshold {
                    ctx.enter_short(trade_qty, bar.close, bar.ts)?;
                }
            }
        } else {
            if ctx.position.is_none() {
                if trade_qty > 0.0 {
                    if predicted_lr > self.enter_threshold {
                        ctx.enter_long(trade_qty, bar.close, bar.ts)?;
                    } else if predicted_lr < -self.enter_threshold {
                        ctx.enter_short(trade_qty, bar.close, bar.ts)?;
                    }
                }
            } else {
                let pos_side = ctx.position.as_ref().unwrap().side;
                if (pos_side == Side::Long && predicted_lr < self.exit_threshold) ||
                   (pos_side == Side::Short && predicted_lr > -self.exit_threshold) {
                    ctx.exit(bar.close, bar.ts)?;
                }
            }
        }

        if ctx.log_strategy() {
            info!(
                ts = %bar.ts.to_rfc3339(),
                pred_lr = predicted_lr,
                total_p = total_p,
                "strategy_adhoc"
            );
        }

        Ok(())
    }
}
