// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Windows Graphics Capture, kept local and limited to an explicitly selected window.
use anyhow::{Context, Result};
use windows::{
    Graphics::{
        Capture::*,
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{Direct3D::D3D_DRIVER_TYPE_HARDWARE, Direct3D11::*, Dxgi::IDXGIDevice},
        System::{
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
            WinRT::{
                Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
                Graphics::Capture::IGraphicsCaptureItemInterop,
            },
        },
    },
    core::Interface,
};

pub fn now() -> Result<f64> {
    let mut count = 0;
    let mut frequency = 0;
    unsafe {
        QueryPerformanceCounter(&mut count)?;
        QueryPerformanceFrequency(&mut frequency)?;
    }
    Ok(count as f64 / frequency as f64)
}

pub struct Pixels {
    pub time: f64,
    pub received: f64,
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
}
impl Pixels {
    pub fn crop(&self, x: usize, y: usize, width: usize, height: usize) -> Result<Self> {
        anyhow::ensure!(
            width > 0
                && height > 0
                && x.checked_add(width).is_some_and(|end| end <= self.width)
                && y.checked_add(height).is_some_and(|end| end <= self.height),
            "preview crop outside captured display"
        );
        let mut bgra = Vec::with_capacity(width * height * 4);
        for row in y..y + height {
            let offset = (row * self.width + x) * 4;
            bgra.extend_from_slice(&self.bgra[offset..offset + width * 4]);
        }
        Ok(Self {
            time: self.time,
            received: self.received,
            width,
            height,
            bgra,
        })
    }
    /// Same chart-readiness sampling as bench.swift: central rows, RGB spread > 60.
    pub fn color_fraction(&self) -> f64 {
        let mut colored = 0usize;
        for gy in 30..108 {
            for gx in 0..192 {
                let x = gx * self.width / 192;
                let y = gy * self.height / 120;
                let offset = (y * self.width + x) * 4;
                let rgb = &self.bgra[offset..offset + 3];
                colored += usize::from(rgb.iter().max().unwrap() - rgb.iter().min().unwrap() > 60);
            }
        }
        colored as f64 / (192 * 78) as f64
    }
    pub fn sample(&self) -> Result<crate::analysis::Sample> {
        Ok(crate::analysis::Sample {
            time: self.time,
            received: self.received,
            grid: crate::analysis::grid(&self.bgra, self.width, self.height, self.width * 4)?,
        })
    }
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let mut rgba = self.bgra.clone();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        image::save_buffer(
            path,
            &rgba,
            self.width as u32,
            self.height as u32,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }
}

pub struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    staging: Option<ID3D11Texture2D>,
    size: (u32, u32),
}

#[cfg(test)]
mod preview_tests {
    use super::Pixels;

    #[test]
    fn preview_contains_only_requested_rows_and_preserves_capture_time() {
        let frame = Pixels {
            time: 1.25,
            received: 1.3,
            width: 4,
            height: 4,
            bgra: (0u8..16)
                .flat_map(|value| [value, value, value, 255])
                .collect(),
        };
        let crop = frame.crop(1, 1, 2, 2).unwrap();
        assert_eq!(
            crop.bgra
                .chunks_exact(4)
                .map(|pixel| pixel[0])
                .collect::<Vec<_>>(),
            vec![5, 6, 9, 10]
        );
        assert_eq!(
            (crop.width, crop.height, crop.time, crop.received),
            (2, 2, 1.25, 1.3)
        );
        assert!(frame.crop(3, 3, 2, 2).is_err());
        assert!(frame.crop(usize::MAX, 0, 2, 1).is_err());
        assert!(frame.crop(0, 0, 0, 1).is_err());
    }
}
impl Capture {
    pub fn window(hwnd: HWND) -> Result<Self> {
        anyhow::ensure!(
            GraphicsCaptureSession::IsSupported()?,
            "Windows Graphics Capture unavailable"
        );
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(hwnd)? };
        Self::item(item)
    }
    pub fn monitor() -> Result<Self> {
        use windows::Win32::{
            Foundation::POINT,
            Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint},
        };
        let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(monitor)? };
        Self::item(item)
    }
    fn item(item: GraphicsCaptureItem) -> Result<Self> {
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
        }
        let device = device.context("D3D device missing")?;
        let context = context.context("D3D context missing")?;
        let dxgi: IDXGIDevice = device.cast()?;
        let winrt: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            item.Size()?,
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        session.SetIsCursorCaptureEnabled(false)?;
        // No permission prompt or system setting change to suppress the capture border.
        // If the OS refuses, retain its default border and record that during preflight.
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture()?;
        Ok(Self {
            pool,
            session,
            device,
            context,
            staging: None,
            size: (0, 0),
        })
    }
    pub fn next(&mut self) -> Result<Option<Pixels>> {
        let frame = match self.pool.TryGetNextFrame() {
            Ok(frame) => frame,
            // WinRT returns S_OK with a null interface when the queue is empty;
            // windows-rs represents this as an Error(S_OK).
            Err(error) if error.code().0 == 0 || error.code().0 == 0x8000000Au32 as i32 => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        let result = (|| -> Result<Pixels> {
            let time = frame.SystemRelativeTime()?.Duration as f64 / 10_000_000.0;
            let received = now()?;
            let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
            let source: ID3D11Texture2D = unsafe { access.GetInterface()? };
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                source.GetDesc(&mut desc);
            }
            let content = frame.ContentSize()?;
            anyhow::ensure!(
                content.Width > 0 && content.Height > 0,
                "empty capture frame"
            );
            // Window geometry is fixed before capture. Abort on resize to keep the grid comparable.
            if self.staging.is_some() {
                anyhow::ensure!(
                    self.size == (desc.Width, desc.Height),
                    "window resized during capture"
                );
            }
            if self.staging.is_none() {
                self.size = (desc.Width, desc.Height);
                desc.Usage = D3D11_USAGE_STAGING;
                desc.BindFlags = 0;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                desc.MiscFlags = 0;
                unsafe {
                    self.device
                        .CreateTexture2D(&desc, None, Some(&mut self.staging))?;
                }
            }
            let staging = self.staging.as_ref().context("staging texture missing")?;
            let width = content.Width as usize;
            let height = content.Height as usize;
            anyhow::ensure!(
                width <= desc.Width as usize && height <= desc.Height as usize,
                "capture surface smaller than content"
            );
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            unsafe {
                self.context.CopyResource(staging, &source);
                self.context
                    .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            }
            let mut bgra = vec![0u8; width * height * 4];
            for y in 0..height {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                        bgra.as_mut_ptr().add(y * width * 4),
                        width * 4,
                    );
                }
            }
            unsafe {
                self.context.Unmap(staging, 0);
            }
            Ok(Pixels {
                time,
                received,
                width,
                height,
                bgra,
            })
        })();
        frame.Close()?;
        result.map(Some)
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
