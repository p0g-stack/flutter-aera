//! Renderers: Flutter draws offscreen on the GPU, then the frame is copied
//! into an AERA slot.
//!
//! Now: [`gl`] (Skia on GLES through EGL's Mesa surfaceless platform: Zink on
//! Turnip on the phone, llvmpipe on a PC) with a full-frame readback.
//! Planned, in this order: async pack-buffer readback with damage-only
//! copies, Vulkan (`vk.rs`, straight on Turnip), Impeller, then dma-buf
//! slots imported as render targets once AERA offers them (demo#1 item 9).

pub mod gl;

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
