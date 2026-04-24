use crate::backtest::{BacktestContext, Bar};

pub mod rolling_pvalue;
pub mod normal_noise;

pub trait Strategy {
    fn name(&self) -> &'static str;
    fn on_start(&mut self, _ctx: &mut BacktestContext) -> anyhow::Result<()> {
        Ok(())
    }
    fn on_bar(&mut self, ctx: &mut BacktestContext, bar: &Bar) -> anyhow::Result<()>;
    fn on_finish(&mut self, _ctx: &mut BacktestContext) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct NoopStrategy;

impl Strategy for NoopStrategy {
    fn name(&self) -> &'static str {
        "NoopStrategy"
    }

    fn on_bar(&mut self, _ctx: &mut BacktestContext, _bar: &Bar) -> anyhow::Result<()> {
        Ok(())
    }
}
