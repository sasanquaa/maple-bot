//! Thanks https://github.com/NiiightmareXD/windows-capture
//! Thanks https://github.com/obsproject/obs-studio/blob/cfb23a51ff8acad13dc739c31854d9f451e05298/libobs-d3d11/d3d11-subsystem.cpp#L587
//! Thanks https://github.com/obsproject/obs-studio/blob/cfb23a51ff8acad13dc739c31854d9f451e05298/libobs-winrt/winrt-capture.cpp#L244

use std::{cmp::min, mem, ptr, slice, sync::mpsc, time::Duration};

use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
            GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Foundation::{HMODULE, HWND, POINT, RECT},
        Graphics::{
            Direct3D::{
                D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1,
                D3D_FEATURE_LEVEL_11_0,
            },
            Direct3D11::{
                D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                ID3D11Texture2D,
            },
            Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
            Dxgi::{
                Common::{DXGI_FORMAT, DXGI_SAMPLE_DESC},
                IDXGIDevice,
            },
            Gdi::ClientToScreen,
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
            RoGetActivationFactory,
        },
        UI::WindowsAndMessaging::GetClientRect,
    },
    core::{HSTRING, Interface, RuntimeName},
};

use super::{Error, Frame, Handle, HandleCell};

const MAX_FRAME_FAILURE: u32 = 3;

#[derive(Debug)]
struct WgcCaptureInner {
    handle: HWND,
    d3d11_device: ID3D11Device,
    d3d11_context: ID3D11DeviceContext,
    d3d11_texture: Option<ID3D11Texture2D>,
    item: GraphicsCaptureItem,
    item_closed_token: i64,
    d3d_device: IDirect3DDevice,
    session: GraphicsCaptureSession,
    frame_format: DirectXPixelFormat,
    frame_pool: Direct3D11CaptureFramePool,
    frame_last_content_size: SizeInt32,
    frame_arrived_token: i64,
    frame_timeout: u64,
    frame_rx: mpsc::Receiver<Message>,
    consecutive_failure: u32,
}

impl WgcCaptureInner {
    fn grab_with_timeout(&mut self) -> Result<Frame, Error> {
        let message = self
            .frame_rx
            .recv_timeout(Duration::from_millis(self.frame_timeout))
            .map_err(|_| {
                self.consecutive_failure += 1;
                // I don't know man maybe I am just missing something from the docs.
                // I can't find how to handle device loss without a swap chain.
                // Even if there is one, I tried and it still doesn't work.
                // When the game changes resolution or going to cash shop, none of the re-creation
                // stuff below works. It doesn't even get called back. The item closed callback
                // sometimes works and sometimes doesn't. This feels like a hack but it works.
                // I referenced other people code for error handling but this shit still
                // doesn't work.
                if self.consecutive_failure >= MAX_FRAME_FAILURE {
                    Error::WindowNotFound
                } else {
                    Error::FrameNotAvailable
                }
            })?;

        let frame = match message {
            Message::FrameArrived(frame) => frame,
            Message::ItemClosed => return Err(Error::WindowNotFound),
        };
        let frame_content_size = frame.ContentSize().unwrap();
        let surface = frame.Surface()?;
        let surface_texture = surface.cast::<IDirect3DDxgiInterfaceAccess>()?;
        let surface_texture = unsafe { surface_texture.GetInterface::<ID3D11Texture2D>()? };
        let mut surface_desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { surface_texture.GetDesc(&mut surface_desc) };

        let texture_rect = get_client_rect(self.handle, surface_desc.Width, surface_desc.Height)?;
        let texture_width = texture_rect.right - texture_rect.left;
        let texture_height = texture_rect.bottom - texture_rect.top;
        if self.d3d11_texture.as_ref().is_none_or(|texture| {
            let mut texture_desc = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                texture.GetDesc(&raw mut texture_desc);
            };
            texture_desc.Width != texture_width || texture_desc.Height != texture_height
        }) {
            self.d3d11_texture = Some(create_texture_2d(
                &self.d3d11_device,
                texture_width,
                texture_height,
                surface_desc.Format,
            )?);
        }
        self.consecutive_failure = 0;

        let texture = self.d3d11_texture.as_ref().unwrap();
        let mut resource = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.d3d11_context.CopySubresourceRegion(
                texture,
                0,
                0,
                0,
                0,
                &surface_texture,
                0,
                Some(&raw const texture_rect),
            );
        }
        unsafe {
            self.d3d11_context
                .Map(texture, 0, D3D11_MAP_READ, 0, Some(&mut resource))?;
        };
        let buffer = unsafe {
            slice::from_raw_parts::<u8>(
                resource.pData.cast(),
                (texture_height * resource.RowPitch) as usize,
            )
        };
        let vec = if texture_height * 4 != resource.RowPitch {
            let capacity = (texture_width * texture_height * 4) as usize;
            let dst_stride = (texture_width * 4) as usize;
            let mut vec = Vec::<u8>::with_capacity(capacity);
            let vec_ptr = vec.as_mut_ptr();
            for i in 0..texture_height as usize {
                let src_offset = resource.RowPitch as usize * i;
                let dst_offset = dst_stride * i;
                unsafe {
                    ptr::copy_nonoverlapping(
                        buffer.as_ptr().add(src_offset),
                        vec_ptr.add(dst_offset),
                        dst_stride,
                    );
                }
            }
            vec
        } else {
            buffer.to_vec()
        };
        unsafe {
            self.d3d11_context.Unmap(texture, 0);
        };

        if frame_content_size != self.frame_last_content_size {
            self.frame_format = DirectXPixelFormat(surface_desc.Format.0);
            self.frame_pool
                .Recreate(&self.d3d_device, self.frame_format, 1, frame_content_size)?;
            self.frame_last_content_size = frame_content_size;
        }

        Ok(Frame {
            width: texture_width as i32,
            height: texture_height as i32,
            data: vec,
        })
    }
}

impl Drop for WgcCaptureInner {
    fn drop(&mut self) {
        let _ = self.item.RemoveClosed(self.item_closed_token);
        let _ = self.frame_pool.RemoveFrameArrived(self.frame_arrived_token);
        let _ = self.frame_pool.Close();
        let _ = self.session.Close();
    }
}

#[derive(Debug)]
enum Message {
    FrameArrived(Direct3D11CaptureFrame),
    ItemClosed,
}

#[derive(Debug)]
pub struct WgcCapture {
    handle: HandleCell,
    d3d11_device: ID3D11Device,
    d3d11_context: ID3D11DeviceContext,
    d3d_device: IDirect3DDevice,
    frame_timeout: u64,
    inner: Option<WgcCaptureInner>,
}

impl WgcCapture {
    pub fn new(handle: Handle, frame_timeout: u64) -> Result<Self, Error> {
        let (d3d11_device, d3d11_context) = create_d3d11_device()?;
        let d3d_device = create_d3d_device(&d3d11_device)?;
        Ok(Self {
            handle: HandleCell::new(handle),
            d3d11_device,
            d3d11_context,
            d3d_device,
            frame_timeout,
            inner: None,
        })
    }

    pub fn grab(&mut self) -> Result<Frame, Error> {
        if self.inner.is_none() {
            if let Some(handle) = self.handle.as_inner() {
                self.start_capture(handle)?;
            }
        }

        if let Some(inner) = self.inner.as_mut() {
            let result = inner.grab_with_timeout();
            if let Err(Error::WindowNotFound) = result.as_ref() {
                self.stop_capture();
            }
            return result;
        }
        Err(Error::WindowNotFound)
    }

    pub fn stop_capture(&mut self) {
        let _ = self.inner.take();
    }

    fn start_capture(&mut self, handle: HWND) -> Result<(), Error> {
        let (tx, rx) = mpsc::channel::<Message>();
        let frame_format = DirectXPixelFormat::B8G8R8A8UIntNormalized;

        let item = create_graphics_capture_item(handle)?;
        let item_closed_tx = tx.clone();
        let item_closed_token = item.Closed(&TypedEventHandler::new(move |_, _| {
            item_closed_tx.send(Message::ItemClosed).unwrap();
            Ok(())
        }))?;

        let frame_last_content_size = item.Size()?;
        let (session, frame_pool) = create_capture_session(&self.d3d_device, &item, frame_format)?;
        let frame_arrived_token =
            frame_pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, _>::new(
                move |frame_pool, _| {
                    tx.send(Message::FrameArrived(
                        frame_pool.as_ref().unwrap().TryGetNextFrame().unwrap(),
                    ))
                    .unwrap();
                    Ok(())
                },
            ))?;
        session.StartCapture()?;
        let _ = session.SetIsBorderRequired(false);

        self.inner = Some(WgcCaptureInner {
            handle,
            d3d11_device: self.d3d11_device.clone(),
            d3d11_context: self.d3d11_context.clone(),
            d3d_device: self.d3d_device.clone(),
            d3d11_texture: None,
            item,
            item_closed_token,
            session,
            frame_format,
            frame_pool,
            frame_last_content_size,
            frame_arrived_token,
            frame_timeout: self.frame_timeout,
            frame_rx: rx,
            consecutive_failure: 0,
        });
        Ok(())
    }
}

#[inline]
fn get_client_rect(handle: HWND, width: u32, height: u32) -> Result<D3D11_BOX, Error> {
    let mut window_rect = RECT::default();
    let mut client_rect = RECT::default();
    unsafe { GetClientRect(handle, &mut client_rect)? };
    unsafe {
        DwmGetWindowAttribute(
            handle,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut window_rect).cast(),
            mem::size_of::<RECT>() as u32,
        )?
    };

    let rect_width = (client_rect.right - client_rect.left) as u32;
    let rect_height = (client_rect.bottom - client_rect.top) as u32;
    if rect_width == 0 || rect_height == 0 {
        return Err(Error::InvalidWindowSize);
    }
    let mut upper_left = POINT::default();
    unsafe { ClientToScreen(handle, &mut upper_left).ok()? };

    let left = (upper_left.x as u32).saturating_sub(window_rect.left as u32);
    let top = (upper_left.y as u32).saturating_sub(window_rect.top as u32);
    let texture_width = if width > left {
        min(width - left, client_rect.right as u32)
    } else {
        1
    };
    let texture_height = if height > top {
        min(height - top, client_rect.bottom as u32)
    } else {
        1
    };
    Ok(D3D11_BOX {
        left,
        top,
        right: left + texture_width,
        bottom: top + texture_height,
        front: 0,
        back: 1,
    })
}

#[inline]
fn create_texture_2d(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
) -> Result<ID3D11Texture2D, Error> {
    let mut texture = None;
    let texture_desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    unsafe {
        device.CreateTexture2D(&texture_desc, None, Some(&mut texture))?;
    }
    Ok(texture.unwrap())
}

#[inline]
fn create_capture_session(
    device: &IDirect3DDevice,
    item: &GraphicsCaptureItem,
    format: DirectXPixelFormat,
) -> Result<(GraphicsCaptureSession, Direct3D11CaptureFramePool), Error> {
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(device, format, 1, item.Size()?)?;
    let session = pool.CreateCaptureSession(item)?;
    Ok((session, pool))
}

#[inline]
fn create_graphics_capture_item(handle: HWND) -> Result<GraphicsCaptureItem, Error> {
    let factory = unsafe {
        RoGetActivationFactory::<IGraphicsCaptureItemInterop>(&HSTRING::from(
            GraphicsCaptureItem::NAME,
        ))?
    };
    Ok(unsafe { factory.CreateForWindow(handle)? })
}

#[inline]
fn create_d3d11_device() -> Result<(ID3D11Device, ID3D11DeviceContext), Error> {
    let feature_flags = [
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
    ];
    let mut d3d_device = None;
    let mut feature_level = D3D_FEATURE_LEVEL_10_0;
    let mut d3d_device_context = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&feature_flags),
            D3D11_SDK_VERSION,
            Some(&mut d3d_device),
            Some(&mut feature_level),
            Some(&mut d3d_device_context),
        )?
    };
    Ok((d3d_device.unwrap(), d3d_device_context.unwrap()))
}

#[inline]
fn create_d3d_device(d3d11_device: &ID3D11Device) -> Result<IDirect3DDevice, Error> {
    let dxgi_device = d3d11_device.cast::<IDXGIDevice>()?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)? };
    let d3d_device = inspectable.cast()?;
    Ok(d3d_device)
}
