use crate::utils::{write_metric, Metric, MetricFamily};
use solana_streamer::streamer::StreamerReceiveStatsTotal;
use std::io;
use std::sync::{Arc, Mutex};

pub type PrometheusCollector = Arc<Mutex<MetricsCollector>>;

pub struct MetricsCollector {
    tpu_receiver_stats: Option<StreamerReceiveStatsTotal>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            tpu_receiver_stats: None,
        }
    }

    pub fn save_tpu_receiver_stats(&mut self, stats: StreamerReceiveStatsTotal) {
        self.tpu_receiver_stats = Some(stats)
    }

    pub fn write_metrics<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        if self.tpu_receiver_stats.is_none() {
            return Ok(());
        }

        let tpu_metrics = self.tpu_receiver_stats.as_ref().unwrap();

        write_metric(
            out,
            &MetricFamily {
                name: "solana_validator_tpu_packets_count_total",
                help: "Packets received by Transaction Processing Unit",
                type_: "counter",
                metrics: vec![Metric::new(tpu_metrics.packets_count_total as u64)],
            },
        )?;

        Ok(())
    }
}
