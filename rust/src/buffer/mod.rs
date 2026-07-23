use std::collections::VecDeque;

use crate::encoder::EncodedPacket;

pub struct ReplayBuffer {
    packets: VecDeque<EncodedPacket>,
    max_frames: usize,
}

impl ReplayBuffer {
    pub fn new(fps: usize, seconds: usize, _width: usize, _height: usize) -> Self {
        Self {
            packets: VecDeque::new(),
            max_frames: fps * seconds,
        }
    }

    pub fn push(&mut self, packet: EncodedPacket) {
        self.packets.push_back(packet);

        while self.packets.len() > self.max_frames {
            self.packets.pop_front();
        }
    }

    pub fn get_last_seconds(&self, seconds: usize, fps: usize) -> Vec<EncodedPacket> {
        let count = seconds * fps;

        self.packets
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.packets.len()
    }

    pub fn clear(&mut self) {
        self.packets.clear();
    }
}
