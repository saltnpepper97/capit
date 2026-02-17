// Author: Dustin Pilgrim
// License: MIT

use std::fs::File;
use std::os::fd::AsFd;

use memmap2::MmapMut;
use tempfile::tempfile;

use wayland_client::protocol::{wl_buffer, wl_shm, wl_shm_pool};
use wayland_client::{QueueHandle};

use super::app::App;

pub struct ShmBuffer {
    pub _file: File,
    pub mmap: MmapMut,
    _pool: wl_shm_pool::WlShmPool,
    pub buffer: wl_buffer::WlBuffer,
    pub width: i32,
    pub height: i32,
    pub busy: bool,
}

impl ShmBuffer {
    pub fn new(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<App>,
        width: i32,
        height: i32,
    ) -> Result<Self, String> {
        let width = width.max(1);
        let height = height.max(1);
        let stride = width * 4;
        let size = (stride * height) as u64;

        let file = tempfile().map_err(|e| format!("tempfile: {e}"))?;
        file.set_len(size).map_err(|e| format!("set_len: {e}"))?;

        let mmap = unsafe { MmapMut::map_mut(&file).map_err(|e| format!("mmap: {e}"))? };

        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());

        Ok(Self {
            _file: file,
            mmap,
            _pool: pool,
            buffer,
            width,
            height,
            busy: false,
        })
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.mmap[..]
    }
}
