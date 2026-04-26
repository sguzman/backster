use anyhow::Result;
use statrs::distribution::{Cauchy, ContinuousCDF, Laplace, Normal, StudentsT};
use statrs::statistics::Median;
use rando::stats::{mean, std_dev, median, kurtosis, quantile};

use crate::stats::ks::ks_p_value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistFamily {
    Normal,
    StudentsT,
    Laplace,
    Logistic,
    Cauchy,
}

impl DistFamily {
    pub const ALL: [DistFamily; 5] = [
        DistFamily::Normal,
        DistFamily::StudentsT,
        DistFamily::Laplace,
        DistFamily::Logistic,
        DistFamily::Cauchy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DistFamily::Normal => "Normal",
            DistFamily::StudentsT => "StudentT",
            DistFamily::Laplace => "Laplace",
            DistFamily::Logistic => "Logistic",
            DistFamily::Cauchy => "Cauchy",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FitPoint {
    pub family: DistFamily,
    pub p_value: f64,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct RollingFitOutput {
    pub points: Vec<FitPoint>,
    pub weighted_value: f64,
    pub total_weight: f64,
    pub best_family: Option<DistFamily>,
    pub best_p_value: f64,
}

pub fn compute_window_fit(sample: &[f64], normalize_weights: bool) -> Result<RollingFitOutput> {
    anyhow::ensure!(sample.len() >= 8, "Need at least 8 samples for fit");

    let mut points = Vec::new();
    let mut best_family = None;
    let mut best_p = -1.0;

    for fam in DistFamily::ALL {
        if let Some((p, v)) = fit_family(sample, fam)? {
            if p > best_p {
                best_p = p;
                best_family = Some(fam);
            }
            points.push(FitPoint {
                family: fam,
                p_value: p,
                value: v,
            });
        } else {
            points.push(FitPoint {
                family: fam,
                p_value: 0.0,
                value: 0.0,
            });
        }
    }

    let total_weight: f64 = points.iter().map(|p| p.p_value.max(0.0)).sum();
    let mut weighted_value: f64 = points
        .iter()
        .map(|p| p.p_value.max(0.0) * p.value)
        .sum();
    if normalize_weights && total_weight > 0.0 {
        weighted_value /= total_weight;
    }

    Ok(RollingFitOutput {
        points,
        weighted_value,
        total_weight,
        best_family,
        best_p_value: best_p.max(0.0),
    })
}

fn fit_family(sample: &[f64], fam: DistFamily) -> Result<Option<(f64, f64)>> {
    let xs: Vec<f64> = sample.iter().copied().filter(|x| x.is_finite()).collect();
    if xs.len() < 8 {
        return Ok(None);
    }

    match fam {
        DistFamily::Normal => {
            let mu = mean(&xs);
            let sigma = std_dev(&xs);
            if !(sigma.is_finite() && sigma > 0.0) {
                return Ok(None);
            }
            let dist = Normal::new(mu, sigma)?;
            let (_, p) = ks_p_value(&xs, |x| dist.cdf(x))?;
            Ok(Some((p, mu)))
        }
        DistFamily::StudentsT => {
            // Heuristic df estimator from sample excess kurtosis (clamped).
            let mu = mean(&xs);
            let sigma = std_dev(&xs);
            if !(sigma.is_finite() && sigma > 0.0) {
                return Ok(None);
            }
            let ex_kurt = kurtosis(&xs) - 3.0;
            let df = if ex_kurt.is_finite() && ex_kurt > 1e-9 {
                (6.0 / ex_kurt + 4.0).clamp(2.5, 80.0)
            } else {
                10.0
            };
            let scale = sigma * ((df - 2.0) / df).sqrt();
            if !(scale.is_finite() && scale > 0.0) {
                return Ok(None);
            }
            let dist = StudentsT::new(mu, scale, df)?;
            let (_, p) = ks_p_value(&xs, |x| dist.cdf(x))?;
            Ok(Some((p, mu)))
        }
        DistFamily::Laplace => {
            let med = median(&xs);
            let b = xs.iter().map(|x| (x - med).abs()).sum::<f64>() / xs.len() as f64;
            if !(b.is_finite() && b > 0.0) {
                return Ok(None);
            }
            let dist = Laplace::new(med, b)?;
            let (_, p) = ks_p_value(&xs, |x| dist.cdf(x))?;
            Ok(Some((p, med)))
        }
        DistFamily::Logistic => {
            let mu = mean(&xs);
            let sigma = std_dev(&xs);
            if !(sigma.is_finite() && sigma > 0.0) {
                return Ok(None);
            }
            let s = sigma * (3.0_f64).sqrt() / std::f64::consts::PI;
            if !(s.is_finite() && s > 0.0) {
                return Ok(None);
            }
            let dist = Logistic { mu, s };
            let (_, p) = ks_p_value(&xs, |x| dist.cdf(x))?;
            Ok(Some((p, dist.mean())))
        }
        DistFamily::Cauchy => {
            let med = median(&xs);
            let q1 = quantile(&xs, 0.25);
            let q3 = quantile(&xs, 0.75);
            let gamma = (q3 - q1).abs() / 2.0;
            if !(gamma.is_finite() && gamma > 0.0) {
                return Ok(None);
            }
            let dist = Cauchy::new(med, gamma)?;
            let (_, p) = ks_p_value(&xs, |x| dist.cdf(x))?;
            let value = dist.median();
            Ok(Some((p, value)))
        }
    }
}


#[derive(Debug, Clone, Copy)]
struct Logistic {
    mu: f64,
    s: f64,
}

impl Logistic {
    fn cdf(self, x: f64) -> f64 {
        let z = (x - self.mu) / self.s;
        1.0 / (1.0 + (-z).exp())
    }

    fn mean(self) -> f64 {
        self.mu
    }
}
