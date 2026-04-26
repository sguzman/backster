use std::collections::VecDeque;
use anyhow::Result;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use tracing::info;

use crate::backtest::{BacktestContext, Bar, Side};
use crate::strategy::Strategy;
use crate::stats::ks::ks_p_value;
use crate::stats::ad::ad_p_value;

use rando::distrs::{NormalFit, DistributionFit, FittedDistribution};

pub struct AdHocNormalPredictor {
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

impl AdHocNormalPredictor {
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

impl Strategy for AdHocNormalPredictor {
    fn name(&self) -> &'static str {
        "AdHocNormalPredictor"
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

        // 2. Perform Normal fit and calculate p-value
        let mut predicted_lr = 0.0;
        let mut weight = 0.0;

        if let Ok(dist) = NormalFit::fit(&data) {
            let p = if self.use_ad_test {
                ad_p_value(&data, |x| dist.cdf(x))?.1
            } else {
                ks_p_value(&data, |x| dist.cdf(x))?.1
            };

            if p.is_finite() && p > 0.0 {
                let s = dist.sample();
                if s.is_finite() {
                    predicted_lr = if self.normalize_weights { s } else { s * p };
                    weight = p;
                }
            }
        }

        if weight < self.min_total_weight || weight == 0.0 {
            return Ok(());
        }

        // 3. Trading logic
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
                weight = weight,
                "strategy_adhoc_normal"
            );
        }

        Ok(())
    }
}
