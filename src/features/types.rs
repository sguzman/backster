use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSeries<T> {
    pub name: String,
    pub instrument_id: String,
    pub timestamps: Vec<DateTime<Utc>>,
    pub values: Vec<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionFitSnapshot {
    pub timestamp: DateTime<Utc>,
    pub normal_p: f64,
    pub student_t_p: f64,
    pub laplace_p: f64,
    pub logistic_p: f64,
    pub cauchy_p: f64,
    pub best_fit: String,
}
