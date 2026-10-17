use std::os::fd::IntoRawFd;

use ffmpeg_next as ffmpeg;
use ffmpeg_sys_next as sys;

use crate::frame::DmaBufFrame;

unsafe extern "C" fn free_descriptor(_opaque: *mut libc::c_void, data: *mut u8) {
    let descriptor = data as *mut sys::AVDRMFrameDescriptor;
    unsafe {
        for index in 0..(*descriptor).nb_objects as usize {
            let fd = (*descriptor).objects[index].fd;
            if fd >= 0 {
                libc::close(fd);
            }
        }
        sys::av_free(data as *mut libc::c_void);
    }
}

pub fn drm_prime_frame(
    frame: DmaBufFrame,
) -> Result<ffmpeg::util::frame::video::Video, std::io::Error> {
    let width = frame.width;
    let height = frame.height;
    let stride = frame.stride as isize;
    let offset = frame.offset as isize;
    let modifier = frame.modifier;
    let fourcc = frame.fourcc;
    let timestamp = frame.timestamp;
    let object_size = (offset + stride * height as isize) as usize;
    let fd = frame.fd.into_raw_fd();

    unsafe {
        let descriptor = sys::av_mallocz(std::mem::size_of::<sys::AVDRMFrameDescriptor>())
            as *mut sys::AVDRMFrameDescriptor;
        if descriptor.is_null() {
            libc::close(fd);
            return Err(std::io::Error::other(
                "av_mallocz failed for DRM descriptor",
            ));
        }

        (*descriptor).nb_objects = 1;
        (*descriptor).objects[0].fd = fd;
        (*descriptor).objects[0].size = object_size;
        (*descriptor).objects[0].format_modifier = modifier;
        (*descriptor).nb_layers = 1;
        (*descriptor).layers[0].format = fourcc;
        (*descriptor).layers[0].nb_planes = 1;
        (*descriptor).layers[0].planes[0].object_index = 0;
        (*descriptor).layers[0].planes[0].offset = offset;
        (*descriptor).layers[0].planes[0].pitch = stride;

        let buf = sys::av_buffer_create(
            descriptor as *mut u8,
            std::mem::size_of::<sys::AVDRMFrameDescriptor>(),
            Some(free_descriptor),
            std::ptr::null_mut(),
            0,
        );
        if buf.is_null() {
            free_descriptor(std::ptr::null_mut(), descriptor as *mut u8);
            return Err(std::io::Error::other(
                "av_buffer_create failed for DRM descriptor",
            ));
        }

        let mut video = ffmpeg::util::frame::video::Video::empty();
        let raw = video.as_mut_ptr();
        (*raw).format = sys::AVPixelFormat::AV_PIX_FMT_DRM_PRIME as i32;
        (*raw).width = width as i32;
        (*raw).height = height as i32;
        (*raw).data[0] = descriptor as *mut u8;
        (*raw).buf[0] = buf;
        (*raw).pts = timestamp as i64;

        Ok(video)
    }
}
