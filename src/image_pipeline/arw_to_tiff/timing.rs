use std::time::{Duration, Instant};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StepTiming {
    pub name: String,
    pub duration: Duration,
}

#[derive(Debug, Default)]
pub struct PipelineTimings {
    steps: Vec<StepTiming>,
    step_map: HashMap<String, Duration>,
}

impl PipelineTimings {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            step_map: HashMap::new(),
        }
    }

    pub fn add_step(&mut self, name: impl Into<String>, duration: Duration) {
        let name = name.into();
        self.steps.push(StepTiming {
            name: name.clone(),
            duration,
        });
        *self.step_map.entry(name).or_insert(Duration::ZERO) += duration;
    }

    pub fn total_duration(&self) -> Duration {
        self.steps.iter().map(|s| s.duration).sum()
    }

    pub fn get_step(&self, name: &str) -> Option<Duration> {
        self.step_map.get(name).copied()
    }

    pub fn steps(&self) -> &[StepTiming] {
        &self.steps
    }

    pub fn print_summary(&self) {
        let total = self.total_duration();
        println!("\nPipeline Timing Summary:");
        println!("{:-<60}", "");
        for step in &self.steps {
            let percentage = if total.as_secs_f64() > 0.0 {
                (step.duration.as_secs_f64() / total.as_secs_f64()) * 100.0
            } else {
                0.0
            };
            println!(
                "{:<30} {:>12.3}ms ({:>5.1}%)",
                step.name,
                step.duration.as_secs_f64() * 1000.0,
                percentage
            );
        }
        println!("{:-<60}", "");
        println!(
            "{:<30} {:>12.3}ms",
            "Total",
            total.as_secs_f64() * 1000.0
        );
    }
}

pub struct Timer {
    start: Instant,
    name: String,
}

impl Timer {
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            name: name.into(),
        }
    }

    pub fn stop(self) -> (String, Duration) {
        (self.name, self.start.elapsed())
    }
}
