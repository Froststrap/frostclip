use std::collections::VecDeque;

use crate::encoder::EncodedPacket;

pub struct ReplayBuffer {
    packets: VecDeque<EncodedPacket>,
    max_duration: i64,
    timebase: i64,
}

impl ReplayBuffer {
    pub fn new(seconds: usize, timebase: i64) -> Self {
        Self {
            packets: VecDeque::new(),
            max_duration: seconds as i64 * timebase,
            timebase,
        }
    }

    pub fn push(&mut self, packet: EncodedPacket) {
        let packet_pts = packet.pts;

        self.packets.push_back(packet);

        while let Some(front) = self.packets.front() {
            if packet_pts - front.pts > self.max_duration {
                self.packets.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn get_last_seconds(&self, seconds: usize) -> Vec<EncodedPacket> {
        if self.packets.is_empty() {
            return Vec::new();
        }

        let last_pts = self.packets.back().unwrap().pts;

        let cutoff = last_pts - (seconds as i64 * self.timebase);

        self.packets
            .iter()
            .filter(|p| p.pts >= cutoff)
            .cloned()
            .collect()
    }

    pub fn timebase(&self) -> i64 {
        self.timebase
    }

    pub fn len(&self) -> usize {
        self.packets.len()
    }

    pub fn clear(&mut self) {
        self.packets.clear();
    }
}
