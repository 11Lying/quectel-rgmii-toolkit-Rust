use crate::{at::At, parser};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Signal {
    time: u64,
    values: [i16; 5],
}

#[derive(Clone, Copy, Default)]
struct Traffic {
    time: u64,
    received: u64,
    sent: u64,
    elapsed_ms: u32,
}

impl Traffic {
    fn rates(&self) -> (Option<f64>, Option<f64>) {
        if self.elapsed_ms == 0 {
            return (None, None);
        }
        let seconds = self.elapsed_ms as f64 / 1000.0;
        (
            Some(self.received as f64 / seconds),
            Some(self.sent as f64 / seconds),
        )
    }
}

#[derive(Default)]
struct TrafficSampler {
    last: Option<Instant>,
    previous: Option<(Instant, u64, u64)>,
}

impl TrafficSampler {
    fn sample(
        &mut self,
        stamp: Option<Instant>,
        time: u64,
        counters: Option<(u64, u64)>,
    ) -> Option<Traffic> {
        let stamp = stamp?;
        if self.last.is_some_and(|last| stamp <= last) {
            return None;
        }
        self.last = Some(stamp);
        let mut point = Traffic {
            time,
            ..Traffic::default()
        };
        if let Some((received, sent)) = counters {
            if let Some((old_stamp, old_received, old_sent)) = self.previous.take() {
                let elapsed = stamp.duration_since(old_stamp).as_millis();
                if (1..=15000).contains(&elapsed) && received >= old_received && sent >= old_sent {
                    point.received = received - old_received;
                    point.sent = sent - old_sent;
                    point.elapsed_ms = elapsed as u32;
                }
            }
            self.previous = Some((stamp, received, sent));
        }
        Some(point)
    }
}

struct Ring<T: Copy, const N: usize> {
    values: [T; N],
    next: usize,
    count: usize,
}

impl<T: Copy, const N: usize> Ring<T, N> {
    fn new(empty: T) -> Self {
        Self {
            values: [empty; N],
            next: 0,
            count: 0,
        }
    }

    fn add(&mut self, value: T) {
        self.values[self.next] = value;
        self.next = (self.next + 1) % N;
        self.count = (self.count + 1).min(N);
    }

    fn iter(&self) -> impl Iterator<Item = &T> {
        (0..self.count).map(move |index| &self.values[(self.next + N - self.count + index) % N])
    }

    fn last(&self) -> Option<&T> {
        (self.count > 0).then(|| &self.values[(self.next + N - 1) % N])
    }
}

struct History {
    generation: u64,
    signal: Ring<Signal, 60>,
    traffic: Ring<Traffic, 60>,
    sampler: TrafficSampler,
}

pub struct Monitor {
    stream: String,
    history: Mutex<History>,
    mock: bool,
}

#[derive(serde::Deserialize)]
pub struct Cursor {
    stream: String,
    generation: u64,
    signal: u64,
    traffic: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn round(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

impl Monitor {
    pub fn new(mock: bool) -> Arc<Self> {
        Arc::new(Self {
            stream: format!("{:016x}", rand::random::<u64>()),
            history: Mutex::new(History {
                generation: 0,
                signal: Ring::new(Signal {
                    time: 0,
                    values: [i16::MIN; 5],
                }),
                traffic: Ring::new(Traffic::default()),
                sampler: TrafficSampler::default(),
            }),
            mock,
        })
    }

    pub fn start(self: &Arc<Self>, at: At) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let time = now();
                let mut values = [i16::MIN; 5];
                let mut counters = None;
                let mut stamp = None;
                if let Ok((raw, sampled)) = at.dashboard_sample().await {
                    stamp = sampled;
                    let data = parser::dashboard(&raw);
                    if !raw.contains("ERROR") && parser::text(&data, "nr_rx_human") != "-" {
                        counters = data["nr_rx_bytes"]
                            .as_u64()
                            .zip(data["nr_tx_bytes"].as_u64());
                    }
                    for (index, (key, min, max)) in [
                        ("rsrpLTE", -160.0, -20.0),
                        ("rsrpNR", -160.0, -20.0),
                        ("sinrLTE", -30.0, 60.0),
                        ("sinrNR", -30.0, 60.0),
                        ("temperature", -40.0, 150.0),
                    ]
                    .iter()
                    .enumerate()
                    {
                        if let Ok(value) = parser::text(&data, key).parse::<f64>()
                            && value.is_finite()
                            && value >= *min
                            && value <= *max
                        {
                            values[index] = (value * 10.0).round() as i16;
                        }
                    }
                }
                let mut history = this.history.lock().unwrap();
                history.signal.add(Signal { time, values });
                if let Some(point) = history.sampler.sample(stamp, time, counters) {
                    history.traffic.add(point);
                }
            }
        });
    }
    pub fn traffic_rates(&self) -> Value {
        let history = self.history.lock().unwrap();
        let last = history
            .traffic
            .last()
            .filter(|point| now().saturating_sub(point.time) <= 15000);
        let (download, upload) = last.map(Traffic::rates).unwrap_or((None, None));
        let format = |rate: Option<f64>| {
            rate.map(|value| format!("{}/s", parser::human_bytes(value)))
                .unwrap_or_else(|| "-".into())
        };
        json!({
            "traffic_rates": true,
            "trafficSampleTime": last.map(|point| point.time),
            "nr_dl_speed": format(download),
            "nr_ul_speed": format(upload),
        })
    }

    pub fn snapshot(&self) -> Value {
        self.snapshot_since(None)
    }

    pub fn snapshot_since(&self, cursor: Option<&Cursor>) -> Value {
        let time = now();
        let cutoff = time.saturating_sub(300000);
        let history = self.history.lock().unwrap();
        let ends = (
            history.signal.last().map_or(0, |point| point.time),
            history.traffic.last().map_or(0, |point| point.time),
        );
        let cursor = cursor.filter(|value| {
            value.stream == self.stream
                && value.generation == history.generation
                && value.signal <= ends.0
                && value.traffic <= ends.1
        });
        let after = cursor.map_or((0, 0), |value| (value.signal, value.traffic));
        let signal: Vec<_> = history
            .signal
            .iter()
            .filter(|point| point.time > cutoff && point.time > after.0)
            .map(|point| {
                let mut value = json!({
                    "time": point.time,
                    "status": if point.values.iter().any(|value| *value != i16::MIN) {
                        "ok"
                    } else {
                        "unavailable"
                    },
                });
                for (index, key) in ["rsrpLTE", "rsrpNR", "sinrLTE", "sinrNR", "temperature"]
                    .iter()
                    .enumerate()
                {
                    value[key] = json!(
                        (point.values[index] != i16::MIN)
                            .then_some(point.values[index] as f64 / 10.0)
                    );
                }
                value
            })
            .collect();
        let traffic: Vec<_> = history
            .traffic
            .iter()
            .filter(|point| point.time > cutoff && point.time > after.1)
            .map(|point| {
                let (download, upload) = point.rates();
                json!({ "time": point.time, "download": download, "upload": upload })
            })
            .collect();
        let (mut received_bytes, mut sent_bytes) = (0u64, 0u64);
        for point in history.traffic.iter().filter(|point| {
            point.elapsed_ms > 0 && point.time.saturating_sub(point.elapsed_ms as u64) > cutoff
        }) {
            received_bytes = received_bytes.saturating_add(point.received);
            sent_bytes = sent_bytes.saturating_add(point.sent);
        }
        let total = received_bytes as f64 + sent_bytes as f64;
        let download_share = (total > 0.0).then(|| round(100.0 * received_bytes as f64 / total));
        let traffic_summary = json!({
            "downloadBytes": received_bytes,
            "uploadBytes": sent_bytes,
            "downloadShare": download_share,
            "uploadShare": download_share.map(|share| round(100.0 - share)),
        });
        json!({
            "delta": cursor.is_some(),
            "cursor": {
                "stream": self.stream,
                "generation": history.generation,
                "signal": ends.0,
                "traffic": ends.1,
            },
            "generation": history.generation,
            "serverTime": time,
            "mock": self.mock,
            "signal": signal,
            "traffic": traffic,
            "trafficSummary": traffic_summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_ignores_duplicate_samples_and_preserves_real_zero() {
        let start = Instant::now();
        let mut sampler = TrafficSampler::default();
        let sample = |seconds| Some(start + Duration::from_secs(seconds));
        assert_eq!(
            sampler
                .sample(sample(0), 0, Some((100, 200)))
                .unwrap()
                .rates(),
            (None, None)
        );
        assert_eq!(
            sampler
                .sample(sample(5), 5000, Some((600, 450)))
                .unwrap()
                .rates(),
            (Some(100.0), Some(50.0))
        );
        assert!(sampler.sample(sample(5), 7000, Some((600, 450))).is_none());
        assert!(sampler.sample(sample(3), 8000, Some((400, 300))).is_none());
        assert_eq!(
            sampler
                .sample(sample(10), 10000, Some((600, 450)))
                .unwrap()
                .rates(),
            (Some(0.0), Some(0.0))
        );
    }
}
