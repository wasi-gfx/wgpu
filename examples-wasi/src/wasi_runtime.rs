use std::sync::Arc;

use wasi_mini_canvas_wasmtime::{HasMainThreadProxy, MainThreadProxy};
use wasi_webgpu_wasmtime::HasGpuInstance;
use wasmtime::{
    component::{Component, Linker, ResourceTable},
    Config, Engine, Store,
};
use wasmtime_wasi::preview2::{self, WasiCtx, WasiCtxBuilder, WasiView};

wasmtime::component::bindgen!({
    path: "wit",
    world: "example",
    async: {
        only_imports: [],
    },
    with: {
        "wasi:webgpu/graphics-context": wasi_graphics_context_wasmtime,
        "wasi:webgpu/mini-canvas": wasi_mini_canvas_wasmtime,
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
    pub instance: Arc<wgpu_core::global::Global<wgpu_core::identity::IdentityManagerFactory>>,
    pub main_thread_proxy: MainThreadProxy,
}

impl HostState {
    fn new(main_thread_proxy: MainThreadProxy) -> Self {
        Self {
            table: ResourceTable::new(),
            ctx: WasiCtxBuilder::new().inherit_stdio().build(),
            instance: Arc::new(wgpu_core::global::Global::new(
                "webgpu",
                wgpu_core::identity::IdentityManagerFactory,
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
    fn table(&self) -> &ResourceTable {
        &self.table
    }

    fn table_mut(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&self) -> &WasiCtx {
        &self.ctx
    }

    fn ctx_mut(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl HasGpuInstance for HostState {
    fn instance(
        &self,
    ) -> Arc<wgpu_core::global::Global<wgpu_core::identity::IdentityManagerFactory>> {
        Arc::clone(&self.instance)
    }
}

impl HasMainThreadProxy for HostState {
    fn main_thread_proxy(&self) -> &MainThreadProxy {
        &self.main_thread_proxy
    }
}

impl ExampleImports for HostState {
    fn print(&mut self, s: String) -> wasmtime::Result<()> {
        println!("{s}");
        Ok(())
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

    wasi_webgpu_wasmtime::add_to_linker(&mut linker, |state: &mut HostState| state).unwrap();
    wasi_graphics_context_wasmtime::add_to_linker(&mut linker, |state: &mut HostState| state)
        .unwrap();
    wasi_mini_canvas_wasmtime::add_to_linker(&mut linker, |state: &mut HostState| state).unwrap();

    preview2::bindings::io::poll::add_to_linker(&mut linker, |state| state).unwrap();
    preview2::bindings::io::streams::add_to_linker(&mut linker, |state| state).unwrap();

    Example::add_root_to_linker(&mut linker, |state: &mut HostState| state).unwrap();

    let (main_thread_loop, main_thread_proxy) =
        wasi_mini_canvas_wasmtime::MiniCanvas::create_event_loop();
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
