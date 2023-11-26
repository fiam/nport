use std::{
    borrow::Cow,
    sync::{atomic::AtomicI64, Arc},
};

use opentelemetry::metrics::{Counter, UpDownCounter};

#[derive(Default)]
pub struct CountedMetric {
    total: Option<Counter<u64>>,
    current: Option<UpDownCounter<i64>>,
    // OTEL metrics don't support retrieving the value of a counter, so we have to keep track of it
    count: AtomicI64,
}

impl CountedMetric {
    pub fn new(
        meter: &opentelemetry::metrics::Meter,
        total_name: impl Into<Cow<'static, str>>,
        total_desc: impl Into<Cow<'static, str>>,
        current_name: impl Into<Cow<'static, str>>,
        current_desc: impl Into<Cow<'static, str>>,
    ) -> Self {
        let total = meter
            .u64_counter(total_name)
            .with_description(total_desc)
            .init();
        let current = meter
            .i64_up_down_counter(current_name)
            .with_description(current_desc)
            .init();
        Self {
            total: Some(total),
            current: Some(current),
            ..Self::default()
        }
    }
    fn add(&self, value: i64) {
        if value > 0 {
            if let Some(c) = self.total.as_ref() {
                c.add(value as u64, &[])
            }
        }
        if let Some(c) = self.current.as_ref() {
            c.add(value, &[])
        }
        self.count
            .fetch_add(value, std::sync::atomic::Ordering::Release);
    }
    pub fn inc(&self) {
        self.add(1)
    }

    pub fn dec(&self) {
        self.add(-1)
    }

    pub fn get(&self) -> i64 {
        self.count.load(std::sync::atomic::Ordering::Acquire)
    }
}

#[derive(Default, Clone)]
pub struct Stats {
    http_requests_total: Option<Counter<u64>>,
    client_connections: Arc<CountedMetric>,
    http_hostnames: Arc<CountedMetric>,
    tcp_ports: Arc<CountedMetric>,
}

impl Stats {
    pub fn new(meter: &opentelemetry::metrics::Meter) -> Self {
        let http_requests_total = meter
            .u64_counter("http_requests_total")
            .with_description("Number of HTTP requests received")
            .init();

        let client_connections = Arc::new(CountedMetric::new(
            meter,
            "client_connections_total",
            "Number of received client connections",
            "client_connections_current",
            "Number of active client connections",
        ));

        let http_hostnames = Arc::new(CountedMetric::new(
            meter,
            "http_hostnames_total",
            "Number of opened HTTP hostnames",
            "http_hostnames_current",
            "Number of active HTTP hostnames",
        ));

        let tcp_ports = Arc::new(CountedMetric::new(
            meter,
            "tcp_ports_total",
            "Number of opened TCP ports",
            "tcp_ports_current",
            "Number of active TCP ports",
        ));

        Self {
            http_requests_total: Some(http_requests_total),
            client_connections,
            http_hostnames,
            tcp_ports,
        }
    }

    pub fn http_request(&self) {
        if let Some(c) = self.http_requests_total.as_ref() {
            c.add(1, &[])
        }
    }

    pub fn client_connections(&self) -> &CountedMetric {
        &self.client_connections
    }

    pub fn http_hostnames(&self) -> &CountedMetric {
        &self.http_hostnames
    }

    pub fn tcp_ports(&self) -> &CountedMetric {
        &self.tcp_ports
    }
}
