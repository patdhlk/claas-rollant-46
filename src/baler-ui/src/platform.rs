// SPDX-License-Identifier: GPL-3.0-only
//! Slint integration for the CR1140: a framebuffer-matching `Xrgb8888`
//! `TargetPixel` and a minimal software-rendering `Platform` driven by our own
//! super-loop in `main` (no system event loop, no GPU, no winit).
//!
//! Adapted from UpTux `cr1140-slint/src/{platform,pixel}.rs`.

#![cfg(feature = "device")]

use slint::platform::software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, TargetPixel};
use slint::platform::{Platform, WindowAdapter};
use slint::PlatformError;
use std::rc::Rc;
use std::time::Instant;

/// A `TargetPixel` matching the CR1140 framebuffer layout (xRGB8888). Stored as
/// `0x00RRGGBB` so `u32::to_le_bytes()` yields `[B, G, R, 0x00]` — identical to
/// `cr1140_hal::display::Surface`'s convention, so no per-pixel conversion is
/// needed when blitting.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct Xrgb8888(pub u32);

impl Xrgb8888 {
    #[inline]
    fn channels(self) -> (u32, u32, u32) {
        ((self.0 >> 16) & 0xff, (self.0 >> 8) & 0xff, self.0 & 0xff)
    }
}

impl TargetPixel for Xrgb8888 {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let inv = 255 - color.alpha as u32;
        let (dr, dg, db) = self.channels();
        let r = color.red as u32 + (dr * inv) / 255;
        let g = color.green as u32 + (dg * inv) / 255;
        let b = color.blue as u32 + (db * inv) / 255;
        self.0 = (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Xrgb8888((red as u32) << 16 | (green as u32) << 8 | blue as u32)
    }
}

/// Minimal Slint `Platform`: one window, software-rendered, no event loop.
pub struct FbPlatform {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl FbPlatform {
    pub fn new(window: Rc<MinimalSoftwareWindow>) -> Self {
        Self {
            window,
            start: Instant::now(),
        }
    }
}

impl Platform for FbPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        self.start.elapsed()
    }
}
