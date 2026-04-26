use anyhow::Result;
use rando::distrs::FittedDistribution;

/// Anderson-Darling statistic + p-value approximation using `rando`.
pub fn ad_p_value(sample: &[f64], cdf: impl Fn(f64) -> f64) -> Result<(f64, f64)> {
    struct CdfWrapper<F>(F);
    impl<F: Fn(f64) -> f64> FittedDistribution for CdfWrapper<F> {
        fn name(&self) -> &'static str { "CustomCDF" }
        fn params(&self) -> Vec<f64> { vec![] }
        fn pdf(&self, _x: f64) -> f64 { 0.0 }
        fn cdf(&self, x: f64) -> f64 { (self.0)(x) }
        fn inv_cdf(&self, _p: f64) -> f64 { 0.0 }
    }

    let res = rando::hypo_tests::anderson_darling_test(sample, &CdfWrapper(cdf))?;
    Ok((res.statistic, res.p_value))
}
