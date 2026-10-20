use std::sync::Arc;

use anyhow::{Context, anyhow};
use winit::window::Window;

pub struct Gpu {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    /// Non-sRGB view format of the swapchain, for egui (which writes
    /// already-encoded colors).
    pub ui_format: wgpu::TextureFormat,
    pub window: Arc<Window>,
    /// "NVIDIA GeForce RTX 2070 SUPER (Dx12)" for diagnostics.
    pub adapter_name: String,
}

impl Gpu {
    pub fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        pollster::block_on(Self::new_async(window))
    }

    async fn new_async(window: Arc<Window>) -> anyhow::Result<Self> {
        let mut size = window.inner_size();
        size.width = size.width.max(1);
        size.height = size.height.max(1);

        // Vulkan first on Windows, DX12 if no Vulkan adapter can show the
        // window. The visuals' shaders build several times faster on Vulkan:
        // the Jellyfish in about 3 s against 15 on DX12, whose FXC compiler
        // left the screen black that long at every start (DXC would be fast
        // too, but needs its DLLs shipped or Visual Studio's ATL to link it
        // in). WGPU_BACKEND=dx12 (or vulkan) picks one.
        let order: Vec<wgpu::Backends> = if cfg!(windows) && std::env::var_os("WGPU_BACKEND").is_none() {
            vec![wgpu::Backends::VULKAN, wgpu::Backends::DX12]
        } else {
            vec![wgpu::InstanceDescriptor::new_without_display_handle_from_env().backends]
        };
        let mut found = None;
        let mut why = String::new();
        for backends in order {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
            desc.backends = backends;
            let instance = wgpu::Instance::new(desc);
            let surface = instance.create_surface(window.clone()).context("create surface")?;
            match instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
            {
                Ok(adapter) => {
                    found = Some((instance, surface, adapter));
                    break;
                }
                Err(e) => {
                    tracing::info!("no {backends:?} adapter: {e}");
                    why = e.to_string();
                }
            }
        }
        let (instance, surface, adapter) = found.ok_or_else(|| anyhow!("no suitable GPU adapter: {why}"))?;
        let info = adapter.get_info();
        tracing::info!("adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("audioviz"),
                required_features: crate::gpu_timer::GpuTimer::features(&adapter),
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("request device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let ui_format = format.remove_srgb_suffix();
        let mut config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| anyhow!("surface not supported by adapter"))?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::Fifo;
        config.alpha_mode = caps.alpha_modes[0];
        if ui_format != format {
            config.view_formats = vec![ui_format];
        }
        surface.configure(&device, &config);
        tracing::info!("surface: {:?} {}x{} {:?}", format, size.width, size.height, config.present_mode);

        let adapter_name = format!("{} ({:?})", info.name, info.backend);
        Ok(Self { instance, surface, device, queue, config, ui_format, window, adapter_name })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    pub fn recreate_surface(&mut self) {
        if let Ok(s) = self.instance.create_surface(self.window.clone()) {
            self.surface = s;
            self.reconfigure();
        }
    }
}
