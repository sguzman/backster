use std::collections::HashMap;
use anyhow::{Result, anyhow};
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use tracing::{info, warn};

use crate::backtest::{BacktestContext, Bar, Side};
use crate::strategy::Strategy;
use crate::config::{PipelineStep, DistFamilyConfig, TestKind, AggregationMethod, ExecutionMode};
use crate::stats::ks::ks_p_value;
use crate::stats::ad::ad_p_value;

use rando::distrs::{NormalFit, StudentTFit, LaplaceFit, LogisticFit, CauchyFit, DistributionFit, FittedDistribution};

pub struct FlexiblePipelinePredictor {
    execution_mode: ExecutionMode,
    enter_threshold: f64,
    exit_threshold: f64,
    force_trade_each_bar: bool,
    pipeline: Vec<PipelineStep>,
    bars: Vec<Bar>,
    current_index: usize,

    results: HashMap<String, StepData>,
    last_close: Option<f64>,
    rng: ChaCha8Rng,
}

#[derive(Clone)]
enum StepData {
    Returns(Vec<f64>),
    Fits(Vec<FitInfo>),
    Samples(Vec<f64>),
    Scalar(f64),
}

#[derive(Clone)]
struct FitInfo {
    family: DistFamilyConfig,
    p_value: f64,
    sample: f64,
}

impl FlexiblePipelinePredictor {
    pub fn new(
        execution_mode: ExecutionMode,
        enter_threshold: f64,
        exit_threshold: f64,
        force_trade_each_bar: bool,
        pipeline: Vec<PipelineStep>,
    ) -> Self {
        Self {
            execution_mode,
            enter_threshold,
            exit_threshold,
            force_trade_each_bar,
            pipeline,
            bars: Vec::new(),
            current_index: 0,
            results: HashMap::new(),
            last_close: None,
            rng: ChaCha8Rng::seed_from_u64(42),
        }
    }

    fn run_step(&mut self, step: &PipelineStep, bar: &Bar) -> Result<()> {
        match step {
            PipelineStep::LogReturns { name, window } => {
                let mut returns = if let Some(StepData::Returns(r)) = self.results.get(name) {
                    r.clone()
                } else {
                    Vec::new()
                };

                if let Some(prev_close) = self.last_close {
                    let lr = (bar.close / prev_close).ln();
                    if lr.is_finite() {
                        returns.push(lr);
                        if returns.len() > *window {
                            returns.remove(0);
                        }
                    }
                }
                self.results.insert(name.clone(), StepData::Returns(returns));
            }
            PipelineStep::FitDistributions { name, input, families, test } => {
                let data = match self.results.get(input) {
                    Some(StepData::Returns(r)) => r,
                    _ => return Ok(()), // Data not ready
                };

                // Only fit if we have enough data (heuristically 10+ points)
                if data.len() < 10 {
                    return Ok(());
                }

                let mut fits = Vec::new();
                for &fam in families {
                    let fit_info = match fam {
                        DistFamilyConfig::Normal => self.fit_one::<NormalFit>(data, fam, *test)?,
                        DistFamilyConfig::StudentsT => self.fit_one::<StudentTFit>(data, fam, *test)?,
                        DistFamilyConfig::Laplace => self.fit_one::<LaplaceFit>(data, fam, *test)?,
                        DistFamilyConfig::Logistic => self.fit_one::<LogisticFit>(data, fam, *test)?,
                        DistFamilyConfig::Cauchy => self.fit_one::<CauchyFit>(data, fam, *test)?,
                    };
                    if let Some(fi) = fit_info {
                        fits.push(fi);
                    }
                }
                self.results.insert(name.clone(), StepData::Fits(fits));
            }
            PipelineStep::Sample { name, input } => {
                let fits = match self.results.get(input) {
                    Some(StepData::Fits(f)) => f,
                    _ => return Ok(()),
                };
                let samples = fits.iter().map(|f| f.sample).collect();
                self.results.insert(name.clone(), StepData::Samples(samples));
            }
            PipelineStep::Aggregate { name, method, values, weights } => {
                let val_data = match self.results.get(values) {
                    Some(StepData::Samples(s)) => s.clone(),
                    Some(StepData::Fits(f)) => f.iter().map(|fi| fi.sample).collect(),
                    _ => return Ok(()),
                };

                let res = match method {
                    AggregationMethod::Sum => val_data.iter().sum::<f64>(),
                    AggregationMethod::Mean => {
                        if val_data.is_empty() { 0.0 } else { val_data.iter().sum::<f64>() / val_data.len() as f64 }
                    }
                    AggregationMethod::Median => {
                        let mut v = val_data.clone();
                        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        if v.is_empty() { 0.0 } else { v[v.len() / 2] }
                    }
                    AggregationMethod::WeightedMean => {
                        let weight_data: Vec<f64> = if let Some(w_name) = weights {
                            match self.results.get(w_name) {
                                Some(StepData::Fits(f)) => f.iter().map(|fi| fi.p_value).collect(),
                                _ => return Ok(()),
                            }
                        } else {
                            return Err(anyhow!("WeightedMean requires weights"));
                        };

                        let total_w: f64 = weight_data.iter().sum::<f64>();
                        if total_w > 0.0 {
                            val_data.iter().zip(weight_data.iter()).map(|(v, w)| v * w).sum::<f64>() / total_w
                        } else {
                            0.0
                        }
                    }
                    AggregationMethod::WeightedSum => {
                        let weight_data: Vec<f64> = if let Some(w_name) = weights {
                            match self.results.get(w_name) {
                                Some(StepData::Fits(f)) => f.iter().map(|fi| fi.p_value).collect(),
                                _ => return Ok(()),
                            }
                        } else {
                            return Err(anyhow!("WeightedSum requires weights"));
                        };

                        val_data.iter().zip(weight_data.iter()).map(|(v, w)| v * w).sum::<f64>()
                    }
                };
                self.results.insert(name.clone(), StepData::Scalar(res));
            }
            PipelineStep::Scale { name, input, factor } => {
                let val = match self.results.get(input) {
                    Some(StepData::Scalar(s)) => *s,
                    _ => return Ok(()),
                };
                self.results.insert(name.clone(), StepData::Scalar(val * factor));
            }
            PipelineStep::Lookahead { name, input, shift } => {
                // This is a bit tricky because we might need to calculate the input for a future index.
                // However, if we just want an oracle, we can look ahead at the returns.
                // For simplicity, we'll only allow Lookahead on scalars for now.
                // To actually look ahead, we'd need to evaluate the whole pipeline for that index.
                
                // Special case: if input is "market_return", we look ahead at self.bars
                if input == "market_return" {
                    let idx = (self.current_index as isize + shift) as usize;
                    if idx < self.bars.len() && idx > 0 {
                        let curr = self.bars[idx].close;
                        let prev = self.bars[idx-1].close;
                        let ret = (curr / prev).ln();
                        self.results.insert(name.clone(), StepData::Scalar(ret));
                    } else {
                        self.results.insert(name.clone(), StepData::Scalar(0.0));
                    }
                } else {
                    // General case: Not yet fully supported for arbitrary pipeline steps
                    warn!("Lookahead only supported for 'market_return' currently");
                }
            }
            PipelineStep::Sign { name, input } => {
                let val = match self.results.get(input) {
                    Some(StepData::Scalar(s)) => *s,
                    _ => return Ok(()),
                };
                let res = if val > 0.0 { 1.0 } else if val < 0.0 { -1.0 } else { 0.0 };
                self.results.insert(name.clone(), StepData::Scalar(res));
            }
            PipelineStep::WolframEval { .. } => {
                warn!("WolframEval step not yet implemented in FlexiblePipelinePredictor");
            }
        }
        Ok(())
    }

    fn fit_one<F: DistributionFit>(&self, data: &[f64], family: DistFamilyConfig, test: TestKind) -> Result<Option<FitInfo>> {
        if let Ok(dist) = F::fit(data) {
            let p = match test {
                TestKind::Ks => ks_p_value(data, |x| dist.cdf(x))?.1,
                TestKind::Ad => ad_p_value(data, |x| dist.cdf(x))?.1,
            };
            if p.is_finite() && p > 0.0 {
                let sample = dist.sample();
                if sample.is_finite() {
                    return Ok(Some(FitInfo { family, p_value: p, sample }));
                }
            }
        }
        Ok(None)
    }
}

impl Strategy for FlexiblePipelinePredictor {
    fn name(&self) -> &'static str {
        "FlexiblePipelinePredictor"
    }

    fn on_data(&mut self, bars: &[Bar]) -> Result<()> {
        self.bars = bars.to_vec();
        Ok(())
    }

    fn on_start(&mut self, ctx: &mut BacktestContext) -> Result<()> {
        self.rng = ChaCha8Rng::seed_from_u64(ctx.seed);
        self.results.clear();
        self.last_close = None;
        Ok(())
    }

    fn on_bar(&mut self, ctx: &mut BacktestContext, bar: &Bar) -> Result<()> {
        // Find current index if not tracked (fallback)
        if self.bars.is_empty() {
             // should not happen with on_data
        } else {
            // Update index based on timestamp match
            if let Some(pos) = self.bars.iter().position(|b| b.ts == bar.ts) {
                self.current_index = pos;
            }
        }

        let steps = self.pipeline.clone();
        for step in &steps {
            self.run_step(step, bar)?;
        }

        self.last_close = Some(bar.close);

        // Final prediction is expected to be named "prediction" by convention or take the last scalar
        let predicted_lr = match self.results.get("prediction") {
            Some(StepData::Scalar(s)) => *s,
            _ => 0.0,
        };

        // Trading logic
        match self.execution_mode {
            ExecutionMode::Threshold => {
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
            }
            ExecutionMode::Linear => {
                // Exit previous position first (daily reset as described by user)
                if ctx.position.is_some() {
                    ctx.exit(bar.close, bar.ts)?;
                }

                let equity = ctx.equity(bar.close);
                let target_value = predicted_lr * equity;
                let target_qty = (target_value.abs() / bar.close).floor();

                if target_qty > 0.0 {
                    if predicted_lr > 0.0 {
                        let final_qty = if ctx.allow_margin() {
                            target_qty
                        } else {
                            let max_qty = (ctx.cash / bar.close).floor();
                            target_qty.min(max_qty)
                        };
                        if final_qty > 0.0 {
                            ctx.enter_long(final_qty, bar.close, bar.ts)?;
                        }
                    } else if predicted_lr < 0.0 {
                        let final_qty = if ctx.allow_margin() {
                            target_qty
                        } else {
                            let max_qty = (ctx.cash / bar.close).floor();
                            target_qty.min(max_qty)
                        };
                        if final_qty > 0.0 {
                            ctx.enter_short(final_qty, bar.close, bar.ts)?;
                        }
                    }
                }
            }
        }

        if ctx.log_strategy() {
            info!(
                ts = %bar.ts.to_rfc3339(),
                pred_lr = predicted_lr,
                "strategy_pipeline"
            );
        }

        Ok(())
    }
}
