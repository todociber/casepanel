use std::{collections::BTreeMap, time::Instant};
use windows::{
    Win32::{
        NetworkManagement::{IpHelper::*, Ndis::IfOperStatusUp},
        System::Power::{PowerGetActiveScheme, PowerReadFriendlyName},
    },
    core::GUID,
};

#[derive(Default)]
pub struct SystemReadings {
    pub down_mb_s: Option<f64>,
    pub up_mb_s: Option<f64>,
    pub power_plan: Option<String>,
}

#[derive(Default)]
struct NetworkRates {
    previous: BTreeMap<u64, (u64, u64)>,
    sampled: bool,
}
impl NetworkRates {
    fn sample(&mut self, counters: BTreeMap<u64, (u64, u64)>, seconds: f64) -> Option<(f64, f64)> {
        let mut down = 0u64;
        let mut up = 0u64;
        for (id, (rx, tx)) in &counters {
            if let Some((old_rx, old_tx)) = self.previous.get(id)
                && rx >= old_rx
                && tx >= old_tx
            {
                down = down.saturating_add(rx - old_rx);
                up = up.saturating_add(tx - old_tx);
            }
        }
        let valid = self.sampled && seconds.is_finite() && seconds > 0.;
        self.previous = counters;
        self.sampled = true;
        valid.then_some((
            down as f64 / seconds / 1_000_000.,
            up as f64 / seconds / 1_000_000.,
        ))
    }
}

#[derive(Default)]
pub struct SystemTelemetry {
    network: NetworkRates,
    last_network: Option<Instant>,
    plan: Option<(GUID, String)>,
}
impl SystemTelemetry {
    pub fn poll(&mut self, network: bool, power: bool) -> SystemReadings {
        let mut result = SystemReadings::default();
        if network {
            let now = Instant::now();
            if let Some(counters) = network_counters() {
                let seconds = self
                    .last_network
                    .map_or(0., |t| now.duration_since(t).as_secs_f64());
                if let Some((down, up)) = self.network.sample(counters, seconds) {
                    result.down_mb_s = Some(down);
                    result.up_mb_s = Some(up);
                }
                self.last_network = Some(now);
            } else {
                self.reset_network();
            }
        } else {
            self.reset_network();
        }
        if power {
            result.power_plan = self.power_plan();
        }
        result
    }
    fn reset_network(&mut self) {
        self.network = NetworkRates::default();
        self.last_network = None;
    }
    fn power_plan(&mut self) -> Option<String> {
        unsafe {
            let mut allocated = std::ptr::null_mut();
            if PowerGetActiveScheme(None, &mut allocated).0 != 0 || allocated.is_null() {
                return None;
            }
            let guid = *allocated;
            crate::platform::LocalFree(allocated.cast());
            if let Some((old, name)) = &self.plan
                && *old == guid
            {
                return Some(name.clone());
            }
            let mut buffer = [0u16; 256];
            let mut size = std::mem::size_of_val(&buffer) as u32;
            if PowerReadFriendlyName(
                None,
                Some(&guid),
                None,
                None,
                Some(buffer.as_mut_ptr().cast()),
                &mut size,
            )
            .0 != 0
            {
                return None;
            }
            let len = buffer.iter().position(|v| *v == 0).unwrap_or(buffer.len());
            let name = String::from_utf16_lossy(&buffer[..len]);
            if name.trim().is_empty() {
                return None;
            }
            self.plan = Some((guid, name.clone()));
            Some(name)
        }
    }
}

fn network_counters() -> Option<BTreeMap<u64, (u64, u64)>> {
    unsafe {
        let mut table = std::ptr::null_mut();
        if GetIfTable2(&mut table).0 != 0 || table.is_null() {
            return None;
        }
        let count = (*table).NumEntries as usize;
        if count > 4096 {
            FreeMibTable(table.cast());
            return None;
        }
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_IF_ROW2>(),
            count,
        );
        let counters = rows
            .iter()
            // HardwareInterface is bit 0. Exclude virtual/VPN adapters to avoid double counting.
            .filter(|r| {
                r.OperStatus == IfOperStatusUp
                    && r.InterfaceAndOperStatusFlags._bitfield & 1 != 0
                    && r.Type != IF_TYPE_SOFTWARE_LOOPBACK
                    && r.Type != IF_TYPE_TUNNEL
            })
            .map(|r| (r.InterfaceLuid.Value, (r.InOctets, r.OutOctets)))
            .collect();
        FreeMibTable(table.cast());
        Some(counters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_rates_handle_elapsed_time_reconnects_and_counter_resets() {
        let mut rates = NetworkRates::default();
        assert!(
            rates
                .sample(BTreeMap::from([(1, (10_000_000, 20_000_000))]), 1.)
                .is_none()
        );
        assert_eq!(
            rates.sample(BTreeMap::from([(1, (14_000_000, 22_000_000))]), 2.),
            Some((2., 1.))
        );
        assert_eq!(
            rates.sample(
                BTreeMap::from([(1, (2, 4)), (2, (999_000_000, 999_000_000))]),
                1.
            ),
            Some((0., 0.))
        );
        assert_eq!(rates.sample(BTreeMap::new(), 1.), Some((0., 0.)));
        assert_eq!(
            rates.sample(BTreeMap::from([(1, (50_000_000, 50_000_000))]), 1.),
            Some((0., 0.))
        );
    }
}
