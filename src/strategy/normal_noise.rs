use anyhow::Result;
use rand_distr::{Distribution, Normal};
use tracing::info;

use crate::backtest::{BacktestContext, Bar};

use super::Strategy;

pub struct NormalNoiseInvestor {
    max_abs_fraction: f64,
    min_trade_cash: f64,
    total_bars: usize,
    idx: usize,
}

impl NormalNoiseInvestor {
    pub fn new(max_abs_fraction: f64, min_trade_cash: f64, total_bars: usize) -> Result<Self> {
        anyhow::ensure!(
            max_abs_fraction.is_finite() && max_abs_fraction > 0.0 && max_abs_fraction <= 1.0,
            "max_abs_fraction must be in (0, 1]"
        );
        anyhow::ensure!(min_trade_cash.is_finite() && min_trade_cash >= 0.0, "bad min_trade_cash");
        Ok(Self {
            max_abs_fraction,
            min_trade_cash,
            total_bars,
            idx: 0,
        })
    }
}

impl Strategy for NormalNoiseInvestor {
    fn name(&self) -> &'static str {
        "NormalNoiseInvestor"
    }

    fn on_bar(&mut self, ctx: &mut BacktestContext, bar: &Bar) -> Result<()> {
        let is_last_bar = self.idx + 1 >= self.total_bars;
        self.idx += 1;

        // Realize previous day P&L.
        if ctx.position.is_some() {
            ctx.exit(bar.close, bar.ts)?;
        }

        if is_last_bar {
            return Ok(());
        }

        let mut rng = ctx.rng();
        let z: f64 = Normal::new(0.0, 1.0)
            .expect("Normal(0,1) must be valid")
            .sample(&mut rng);
        let frac = z.clamp(-self.max_abs_fraction, self.max_abs_fraction);
        let invest_cash = frac.abs() * ctx.cash;

        if ctx.log_strategy() {
            info!(
                ts = %bar.ts.to_rfc3339(),
                close = bar.close,
                z = z,
                frac = frac,
                invest_cash = invest_cash,
                cash = ctx.cash,
                "strategy_normal_noise"
            );
        }

        if invest_cash < self.min_trade_cash || invest_cash <= 0.0 {
            return Ok(());
        }

        let qty = invest_cash / bar.close;
        if qty <= 0.0 || !qty.is_finite() {
            return Ok(());
        }

        if frac >= 0.0 {
            ctx.enter_long(qty, bar.close, bar.ts)?;
        } else {
            ctx.enter_short(qty, bar.close, bar.ts)?;
        }

        Ok(())
    }
}
