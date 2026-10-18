use ffmpeg_next as ffmpeg;
use ffmpeg_next::filter;
use ffmpeg_sys_next as sys;

use super::drm_hw::DrmHardware;

pub struct VaapiGraph {
    graph: filter::Graph,
    drm: DrmHardware,
}

unsafe impl Send for VaapiGraph {}

impl VaapiGraph {
    pub fn new(
        drm_path: &str,
        input_width: u32,
        input_height: u32,
        output_width: u32,
        output_height: u32,
        framerate: u32,
    ) -> Result<Self, String> {
        let drm = DrmHardware::new(drm_path, input_width, input_height)?;
        let mut graph = filter::Graph::new();

        let buffer = filter::find("buffer").ok_or("buffer filter missing")?;
        let buffersink = filter::find("buffersink").ok_or("buffersink filter missing")?;

        let args = format!(
            "video_size={}x{}:pix_fmt=drm_prime:time_base=1/1000000:pixel_aspect=1/1:frame_rate={}/1",
            input_width, input_height, framerate
        );

        let source_ptr = unsafe {
            let name = std::ffi::CString::new("in").unwrap();
            sys::avfilter_graph_alloc_filter(graph.as_mut_ptr(), buffer.as_ptr(), name.as_ptr())
        };

        if source_ptr.is_null() {
            return Err("avfilter_graph_alloc_filter failed".into());
        }

        unsafe {
            let params = sys::av_buffersrc_parameters_alloc();
            if params.is_null() {
                return Err("av_buffersrc_parameters_alloc failed".into());
            }

            (*params).format = sys::AVPixelFormat::AV_PIX_FMT_DRM_PRIME as i32;
            (*params).width = input_width as i32;
            (*params).height = input_height as i32;
            (*params).time_base = sys::AVRational {
                num: 1,
                den: 1_000_000,
            };
            (*params).frame_rate = sys::AVRational {
                num: framerate as i32,
                den: 1,
            };
            (*params).hw_frames_ctx = drm.frames_ctx();

            let ret = sys::av_buffersrc_parameters_set(source_ptr, params);
            sys::av_free(params as *mut libc::c_void);

            if ret < 0 {
                return Err(format!("av_buffersrc_parameters_set failed {}", ret));
            }

            let ret = sys::avfilter_init_str(source_ptr, std::ptr::null());
            if ret < 0 {
                return Err(format!("avfilter_init_str failed {}", ret));
            }
        }

        graph
            .add(&buffersink, "out", "")
            .map_err(|err| format!("buffersink add failed: {err}"))?;

        let spec = format!(
            "hwmap=derive_device=vaapi,scale_vaapi=w={}:h={}:format=nv12",
            output_width, output_height
        );

        graph
            .output("in", 0)
            .and_then(|parser| parser.input("out", 0))
            .and_then(|parser| parser.parse(&spec))
            .map_err(|err| format!("graph parse failed: {err}"))?;

        graph
            .validate()
            .map_err(|err| format!("graph validate failed: {err}"))?;

        Ok(Self { graph, drm })
    }

    pub fn drm_frames_ctx(&self) -> *mut sys::AVBufferRef {
        self.drm.frames_ctx()
    }

    pub fn push(&mut self, frame: &ffmpeg::util::frame::video::Video) -> Result<(), String> {
        let mut context = self.graph.get("in").ok_or("graph source missing")?;
        context
            .source()
            .add(frame)
            .map_err(|err| format!("graph push failed: {err}"))
    }

    pub fn pull(
        &mut self,
        out: &mut ffmpeg::util::frame::video::Video,
    ) -> Result<(), ffmpeg::Error> {
        let mut context = self.graph.get("out").ok_or(ffmpeg::Error::InvalidData)?;
        context.sink().frame(out)
    }

    pub fn hw_frames_ctx(&mut self) -> *mut sys::AVBufferRef {
        let context = self.graph.get("out");
        match context {
            Some(context) => unsafe { sys::av_buffersink_get_hw_frames_ctx(context.as_ptr()) },
            None => std::ptr::null_mut(),
        }
    }
}
