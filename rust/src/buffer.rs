use std::collections::VecDeque;

pub struct ReplayBuffer {
    max_frames: usize,
    width: usize,
    height: usize,
    frames: VecDeque<Vec<u8>>,
}

impl ReplayBuffer {
    pub fn new(framerate: usize, buffer_seconds: usize, width: usize, height: usize) -> Self {
        let max_frames = framerate * buffer_seconds;
        Self {
            max_frames,
            width,
            height,
            frames: VecDeque::with_capacity(max_frames),
        }
    }

    pub fn push(&mut self, frame: Vec<u8>) {
        if self.frames.len() >= self.max_frames {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }

    pub fn get_last_frames(&self, count: usize) -> Vec<Vec<u8>> {
        let actual_count = count.min(self.frames.len());
        if actual_count == 0 {
            return Vec::new();
        }
        let start = self.frames.len() - actual_count;
        self.frames.iter().skip(start).cloned().collect()
    }

    pub fn len(&self) -> usize { self.frames.len() }
    pub fn is_empty(&self) -> bool { self.frames.is_empty() }
    pub fn frame_size(&self) -> usize { self.width * self.height * 4 }
}
