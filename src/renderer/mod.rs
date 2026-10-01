//! Renderers: Flutter draws offscreen on the GPU, then the frame is copied
//! into an AERA slot.
//!
//! - [`gl`], the default: GLES through EGL's Mesa surfaceless platform (Zink
//!   on Turnip on the phone, softpipe or llvmpipe on a PC).
//! - [`vk`], with `--vulkan` as in flutter-pi: Vulkan straight on Turnip (or
//!   lavapipe on a PC).
//!
//! Either draws with Skia, or Impeller with `--enable-impeller` (the
//! engine's own switch, as on GTK). Skia on GL repaints and copies only what
//! changed ([`damage`]); the rest copy whole frames. Planned: dma-buf slots
//! imported as render targets once AERA offers them (demo#1 item 9).

pub mod damage;
pub mod gl;
pub mod vk;

/// The renderer this process uses.
pub enum Renderer {
    Gl(gl::Gl),
    Vk(vk::Vk),
}

impl Renderer {
    pub fn gl(&self) -> &gl::Gl {
        match self {
            Renderer::Gl(g) => g,
            Renderer::Vk(_) => unreachable!("GL callback on the Vulkan renderer"),
        }
    }

    pub fn vk(&self) -> &vk::Vk {
        match self {
            Renderer::Vk(v) => v,
            Renderer::Gl(_) => unreachable!("Vulkan callback on the GL renderer"),
        }
    }

    /// Frames from now on are `width` × `height` (a later SURFACE). False
    /// if the renderer cannot follow.
    pub fn resize(&self, width: u32, height: u32) -> bool {
        match self {
            Renderer::Gl(g) => {
                g.resize(width, height);
                true
            }
            Renderer::Vk(v) => v.resize(width, height),
        }
    }

    /// The size the renderer currently draws at.
    pub fn size(&self) -> (u32, u32) {
        match self {
            Renderer::Gl(g) => g.size(),
            Renderer::Vk(v) => v.size(),
        }
    }
}

/// Swaps R and B in place, for drivers that cannot read back BGRA.
pub fn rgba_to_bgra(rows: &mut [u8], width: usize, stride: usize, height: usize) {
    for row in rows.chunks_mut(stride).take(height) {
        for px in row[..width * 4].chunks_exact_mut(4) {
            px.swap(0, 2);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn swizzle_respects_stride() {
        let mut b = vec![1, 2, 3, 4, 9, 9, 9, 9, 5, 6, 7, 8, 9, 9, 9, 9];
        super::rgba_to_bgra(&mut b, 1, 8, 2);
        assert_eq!(b, vec![3, 2, 1, 4, 9, 9, 9, 9, 7, 6, 5, 8, 9, 9, 9, 9]);
    }
}
