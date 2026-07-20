use std::collections::HashMap;
use std::time::Instant;

use sysinfo::Networks;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct InterfaceStats {
    pub received: u64,
    pub transmitted: u64,
    pub total_received: u64,
    pub total_transmitted: u64,
    pub recv_speed: u64,
    pub sent_speed: u64,
    pub mac: String,
}

pub struct NetworkMonitor {
    networks: Networks,
    last_update: Instant,
    pub interfaces: HashMap<String, InterfaceStats>,
    pub total_recv: u64,
    pub total_sent: u64,
    pub total_recv_speed: u64,
    pub total_sent_speed: u64,
}

impl NetworkMonitor {
    pub fn new() -> Self {
        let mut networks = Networks::new_with_refreshed_list();
        networks.refresh();
        Self {
            networks,
            last_update: Instant::now(),
            interfaces: HashMap::new(),
            total_recv: 0,
            total_sent: 0,
            total_recv_speed: 0,
            total_sent_speed: 0,
        }
    }

    pub fn refresh(&mut self) {
        self.networks.refresh();
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64().max(0.001);

        let mut total_recv = 0u64;
        let mut total_sent = 0u64;
        let mut total_recv_speed = 0u64;
        let mut total_sent_speed = 0u64;

        self.interfaces.clear();

        for (name, data) in self.networks.iter() {
            // sysinfo's received()/transmitted() are ALREADY the bytes moved
            // since the previous refresh -- they are not cumulative counters.
            // So they only need dividing by the interval. Subtracting a stored
            // previous sample here (as this used to) computes a delta of a
            // delta, which saturating_sub then clamps to zero whenever traffic
            // is steady or falling: measured ~20x under-reporting.
            // The cumulative counters are total_received()/total_transmitted().
            let interval_recv = data.received();
            let interval_sent = data.transmitted();

            let recv_speed = (interval_recv as f64 / elapsed) as u64;
            let sent_speed = (interval_sent as f64 / elapsed) as u64;

            total_recv += data.total_received();
            total_sent += data.total_transmitted();
            total_recv_speed += recv_speed;
            total_sent_speed += sent_speed;

            self.interfaces.insert(
                name.clone(),
                InterfaceStats {
                    received: interval_recv,
                    transmitted: interval_sent,
                    total_received: data.total_received(),
                    total_transmitted: data.total_transmitted(),
                    recv_speed,
                    sent_speed,
                    mac: data.mac_address().to_string(),
                },
            );
        }

        self.last_update = now;
        self.total_recv = total_recv;
        self.total_sent = total_sent;
        self.total_recv_speed = total_recv_speed;
        self.total_sent_speed = total_sent_speed;
    }

    pub fn is_real_interface(name: &str) -> bool {
        !name.starts_with("lo")
            && !name.starts_with("bridge")
            && !name.starts_with("utun")
            && !name.starts_with("awdl")
            && !name.starts_with("llw")
            && !name.starts_with("ipsec")
            && !name.starts_with("vmnet")
    }

    pub fn active_interfaces(&self) -> Vec<(&String, &InterfaceStats)> {
        let mut v: Vec<_> = self
            .interfaces
            .iter()
            .filter(|(name, _)| Self::is_real_interface(name))
            .filter(|(_, s)| s.recv_speed > 0 || s.sent_speed > 0 || s.received > 0 || s.transmitted > 0)
            .collect();
        v.sort_by(|a, b| {
            (b.1.recv_speed + b.1.sent_speed)
                .cmp(&(a.1.recv_speed + a.1.sent_speed))
        });
        v
    }
}