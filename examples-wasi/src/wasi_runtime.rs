use std::sync::Arc;

use wasi_graphics_context_wasmtime::WasiGraphicsContextView;
use wasi_surface_wasmtime::{MiniCanvas, MiniCanvasDesc, WasiMiniCanvasView};
use wasi_webgpu_wasmtime::reexports::{wgpu_core, wgpu_types};
use wasi_webgpu_wasmtime::WasiWebGpuView;
use wasmtime::{
    component::{Component, Linker, ResourceTable},
    Config, Engine, Store,
};
use wasmtime_wasi::{self, WasiCtx, WasiCtxBuilder, WasiView};

wasmtime::component::bindgen!({
    path: "wit",
    world: "example",
    async: {
        only_imports: [],
    },
    with: {
        "wasi:webgpu/graphics-context": wasi_graphics_context_wasmtime,
        "wasi:webgpu/surface": wasi_surface_wasmtime,
        "wasi:webgpu/webgpu": wasi_webgpu_wasmtime,
    },
});

fn get_example_name() -> Option<String> {
    std::env::args().nth(1)
}

fn print_unknown_example(result: Option<String>) {
    if let Some(example) = result {
        println!("Unknown example: {}", example);
    } else {
        println!("Please specify an example as the first argument!");
    }
}

struct HostState {
    pub table: ResourceTable,
    pub ctx: WasiCtx,
    pub instance: Arc<wgpu_core::global::Global>,
    pub main_thread_proxy: wasi_surface_wasmtime::WasiWinitEventLoopProxy,
}

impl HostState {
    fn new(main_thread_proxy: wasi_surface_wasmtime::WasiWinitEventLoopProxy) -> Self {
        Self {
            table: ResourceTable::new(),
            ctx: WasiCtxBuilder::new().inherit_stdio().build(),
            instance: Arc::new(wgpu_core::global::Global::new(
                "webgpu",
                wgpu_types::InstanceDescriptor {
                    backends: wgpu_types::Backends::all(),
                    flags: wgpu_types::InstanceFlags::from_build_config(),
                    dx12_shader_compiler: wgpu_types::Dx12Compiler::Fxc,
                    gles_minor_version: wgpu_types::Gles3MinorVersion::default(),
                },
            )),
            main_thread_proxy,
        }
    }
}

impl WasiView for HostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

struct UiThreadSpawner(wasi_surface_wasmtime::WasiWinitEventLoopProxy);

impl wasi_webgpu_wasmtime::MainThreadSpawner for UiThreadSpawner {
    async fn spawn<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T + Send + Sync + 'static,
        T: Send + Sync + 'static,
    {
        self.0.spawn(f).await
    }
}

impl WasiWebGpuView for HostState {
    fn instance(&self) -> Arc<wgpu_core::global::Global> {
        Arc::clone(&self.instance)
    }

    fn ui_thread_spawner(&self) -> Box<impl wasi_webgpu_wasmtime::MainThreadSpawner> {
        Box::new(UiThreadSpawner(self.main_thread_proxy.clone()))
    }
}

impl WasiMiniCanvasView for HostState {
    fn create_canvas(&self, desc: MiniCanvasDesc) -> MiniCanvas {
        futures::executor::block_on(self.main_thread_proxy.create_window(desc))
    }
}

impl WasiGraphicsContextView for HostState {}

impl ExampleImports for HostState {
    fn print(&mut self, s: String) {
        println!("{s}");
    }
}

#[tokio::main]
async fn main() {
    #[cfg(target_arch = "wasm32")]
    print_examples();

    let Some(example) = get_example_name() else {
        print_unknown_example(None);
        return;
    };

    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let mut config = Config::default();
    config.wasm_component_model(true);
    config.async_support(true);
    let engine = Engine::new(&config).unwrap();
    let mut linker = Linker::new(&engine);

    wasi_webgpu_wasmtime::add_to_linker(&mut linker).unwrap();
    wasi_graphics_context_wasmtime::add_to_linker(&mut linker).unwrap();
    wasi_surface_wasmtime::add_to_linker(&mut linker).unwrap();

    fn type_annotate<F>(val: F) -> F
    where
        F: Fn(&mut HostState) -> &mut dyn ExampleImports,
    {
        val
    }
    let closure = type_annotate::<_>(|t| t);
    Example::add_to_linker_imports_get_host(&mut linker, closure).unwrap();

    let (main_thread_loop, main_thread_proxy) =
        wasi_surface_wasmtime::create_wasi_winit_event_loop();
    let host_state = HostState::new(main_thread_proxy);

    let mut store = Store::new(&engine, host_state);

    let wasm_path = format!("./target/examples_component.wasm");

    let component = Component::from_file(&engine, &wasm_path).unwrap();

    let (instance, _) = Example::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();

    tokio::spawn(async move {
        instance.call_start(&mut store, &example).await.unwrap();
    });

    main_thread_loop.run();
}
