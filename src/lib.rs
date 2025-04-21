use std::sync::Arc;

use log::{error, info, warn};
use futures::executor::block_on;

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

use winit::dpi::LogicalSize;
use winit::window::{Window, WindowAttributes, WindowId};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::event::*;
use winit::application::ApplicationHandler;

struct GfxState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    // The window must be declared after the surface so
    // it gets dropped after it as the surface contains
    // unsafe references to the window's resources.
    //window: Window,
}

impl<'a> GfxState<'a> {
    // Creating some of the wgpu types requires async code
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            backend_options: wgpu::BackendOptions::default(),
            flags: wgpu::InstanceFlags::default()
        });

        let surface = instance.create_surface(window.clone()).expect("Cannot create surface");
        for adapter in instance.enumerate_adapters(wgpu::Backends::all()) {
            log::info!("Available adapter: {:?}", adapter.get_info());
        }

        let adapters = instance.enumerate_adapters(wgpu::Backends::all());
        let adapter = &adapters
            .iter()
            .find(|adapter| adapter.is_surface_supported(&surface)) // Check if this adapter supports our surface
            .expect("No suitable adapter found");

        log::info!("Adapter is: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web we'll have to disable some.
                    required_limits: if cfg!(target_arch = "wasm32") {
                        wgpu::Limits::downlevel_webgl2_defaults()
                    } else {
                        wgpu::Limits::default()
                    },
                    label: None,
                    memory_hints: wgpu::MemoryHints::Performance,
                    trace: wgpu::Trace::Off // Trace path
                }
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an sRGB surface texture. Using a different
        // one will result all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps.formats.iter()
            .copied()
            .find(|f| f.is_srgb())            
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("color.wgsl").into()),
        });
         
        let render_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"), // 1.
                buffers: &[], // 2.
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState { // 3.
                module: &shader,
                entry_point:  Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState { // 4.
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None, // 1.
            multisample: wgpu::MultisampleState {
                count: 1, // 2.
                mask: !0, // 3.
                alpha_to_coverage_enabled: false, // 4.
            },
            multiview: None, // 5.
            cache: None,
        });
         

        return Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            // window
        };
    }

    // pub fn window(&self) -> &Window {
    //     &self.window
    // }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn input(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    fn update(&mut self) {

    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline); // 2.
            render_pass.draw(0..3, 0..1);
        }
    
        // submit will accept anything that implements IntoIter
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

struct WindowGfxState<'a> {
    window: Arc<Window>,
    gfx_state: GfxState<'a>
}

impl<'a> WindowGfxState<'a> {
    fn new(event_loop: &ActiveEventLoop) -> Self {
        let window = Arc::new(event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("Rusty Cubes")
                    .with_inner_size(LogicalSize::new(800, 800))
            )
            .expect("Cannot create window"));

        #[cfg(target_arch = "wasm32")]
        {
            // Winit prevents sizing with CSS, so we have to set
            // the size manually when on web.
            use winit::dpi::PhysicalSize;
            let _ = window.request_inner_size(PhysicalSize::new(800, 800));
            
            use winit::platform::web::WindowExtWebSys;
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| {
                    let dst = doc.get_element_by_id("rustycubes")?;
                    let canvas = web_sys::Element::from(window.canvas()?);
                    dst.append_child(&canvas).ok()?;
                    Some(())
                })
                .expect("Couldn't append canvas to document body.");
        }
            
        let gfx_state = block_on(GfxState::new(window.clone()));

        Self {
            window,
            gfx_state
        }
    }
}

#[derive(Default)]
struct Application<'a> {
    state: Option<WindowGfxState<'a>>,
}

impl<'a> ApplicationHandler for Application<'a> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        info!("Resumed.");
        
        self.state = Some(WindowGfxState::new(event_loop));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };

        if window_id == state.window.id() && !state.gfx_state.input(&event) {
            match event {
                WindowEvent::CloseRequested | WindowEvent::KeyboardInput {
                    event: KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        .. 
                    },
                    ..
                } => event_loop.exit(),
                WindowEvent::Resized(physical_size) => {
                    state.gfx_state.resize(physical_size);
                },
                WindowEvent::ScaleFactorChanged { .. } => {
                    // do not handle scale factor for now
                },
                WindowEvent::RedrawRequested if window_id == state.window.id() => {
                    info!("Loop");

                    state.gfx_state.update();

                    match state.gfx_state.render() {
                        Ok(_) => {}
                        // Reconfigure the surface if lost
                        Err(wgpu::SurfaceError::Lost) => state.gfx_state.resize(state.gfx_state.size),
                        // The system is out of memory, we should probably quit
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            error!("Out of memory. Cannot render.");
                            event_loop.exit()
                        },
                        // All other errors (Outdated, Timeout) should be resolved by the next frame
                        Err(e) => error!("{:?}", e),
                    }

                    state.window.request_redraw();
                },
                _ => {}
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _device_id: DeviceId,
        _event: DeviceEvent,
    ) {
        // info!("Device {device_id:?} event: {event:?}");
    }
    
    fn suspended(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        info!("Suspended.");
    }
    
    fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        info!("Exiting.");
    }
    
    fn memory_warning(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        warn!("Memory warning.");
    }
}

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub fn run() {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Warn).expect("Couldn't initialize logger");
        } else {
            env_logger::init();
        }
    }

    let event_loop = EventLoop::builder()
        .build()
        .expect("Cannot build event loop");

    // Continuously run the event loop
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = Application::default();

    event_loop.run_app(&mut app).expect("Cannot run app");
}
