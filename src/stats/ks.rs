use anyhow::Result;
use rando::distrs::FittedDistribution;

/// Two-sided Kolmogorov–Smirnov statistic + asymptotic p-value approximation.
///
/// Note: This is the classic KS p-value for a fully-specified distribution. When
/// distribution parameters are estimated from the same sample, the p-values are
/// optimistic (i.e. too large). For research/prototyping this is often acceptable,
/// but treat it as a heuristic rather than a strict hypothesis test.
pub fn ks_p_value(sample: &[f64], cdf: impl Fn(f64) -> f64) -> Result<(f64, f64)> {
    struct CdfWrapper<F>(F);
    impl<F: Fn(f64) -> f64> FittedDistribution for CdfWrapper<F> {
        fn name(&self) -> &'static str { "CustomCDF" }
        fn params(&self) -> Vec<f64> { vec![] }
        fn pdf(&self, _x: f64) -> f64 { 0.0 }
        fn cdf(&self, x: f64) -> f64 { (self.0)(x) }
        fn inv_cdf(&self, _p: f64) -> f64 { 0.0 }
    }

    let res = rando::hypo_tests::kolmogorov_smirnov_test(sample, &CdfWrapper(cdf))?;
    Ok((res.statistic, res.p_value))
}

