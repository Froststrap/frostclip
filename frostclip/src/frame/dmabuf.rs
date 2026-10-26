use std::os::fd::{AsRawFd, OwnedFd};

use super::{VideoFormat, VideoFrame};

const DMA_BUF_IOCTL_SYNC: u64 = 0x4008_6200;
const SYNC_START_READ: u64 = 1;
const SYNC_END_READ: u64 = 5;
const MOD_LINEAR: u64 = 0;
const MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

#[derive(Debug)]
pub struct DmaBufFrame {
    pub fd: OwnedFd,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub offset: u32,
    pub modifier: u64,
    pub fourcc: u32,
    pub format: VideoFormat,
    pub timestamp: u64,
}

impl DmaBufFrame {
    pub fn into_cpu(self) -> Result<VideoFrame, std::io::Error> {
        if self.modifier != MOD_LINEAR && self.modifier != MOD_INVALID {
            return Err(std::io::Error::other(format!(
                "cannot CPU-map dma-buf with modifier 0x{:x}",
                self.modifier
            )));
        }

        let width = self.width as usize;
        let height = self.height as usize;
        if width == 0 || height == 0 {
            return Err(std::io::Error::other("dma-buf frame has zero size"));
        }

        let row_bytes = width * 4;
        let stride = self.stride as usize;
        let offset = self.offset as usize;
        let map_len = offset + stride * (height - 1) + row_bytes;
        let raw_fd = self.fd.as_raw_fd();

        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                raw_fd,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }

        let start = SYNC_START_READ;
        let start_result = unsafe { libc::ioctl(raw_fd, DMA_BUF_IOCTL_SYNC as _, &start) };
        if start_result < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::munmap(ptr, map_len) };
            return Err(err);
        }

        let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, map_len) };
        let swap = self.format == VideoFormat::Bgra;
        let mut out = vec![0u8; row_bytes * height];

        for y in 0..height {
            let src_start = offset + y * stride;
            let src = &bytes[src_start..src_start + row_bytes];
            let dst = &mut out[y * row_bytes..(y + 1) * row_bytes];

            if swap {
                for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            } else {
                dst.copy_from_slice(src);
            }
        }

        let end = SYNC_END_READ;
        unsafe {
            libc::ioctl(raw_fd, DMA_BUF_IOCTL_SYNC as _, &end);
            libc::munmap(ptr, map_len);
        }

        Ok(VideoFrame::Cpu {
            data: out,
            width: self.width,
            height: self.height,
            stride: row_bytes as u32,
            format: VideoFormat::Rgba,
            timestamp: self.timestamp,
        })
    }
}
