use std::collections::VecDeque;

use crate::encoder::EncodedPacket;

pub struct AudioBuffer {
    packets: VecDeque<EncodedPacket>,
    max_duration: i64,
    sample_rate: i64,
}

impl AudioBuffer {
    pub fn new(seconds: usize, sample_rate: u32) -> Self {
        Self {
            packets: VecDeque::new(),
            max_duration: seconds as i64 * sample_rate as i64,
            sample_rate: sample_rate as i64,
        }
    }

    pub fn push(&mut self, packet: EncodedPacket) {
        let newest = packet.pts;

        self.packets.push_back(packet);

        while let Some(front) = self.packets.front() {
            if newest - front.pts > self.max_duration {
                self.packets.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn get_from_us(&self, start_us: i64) -> Vec<EncodedPacket> {
        let cutoff = start_us * self.sample_rate / 1_000_000;

        self.packets
            .iter()
            .filter(|p| p.pts >= cutoff)
            .cloned()
            .collect()
    }

    pub fn clear(&mut self) {
        self.packets.clear();
    }
}
