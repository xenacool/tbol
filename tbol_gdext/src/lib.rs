use crate::networking::TokioRuntime;
use godot::classes::Engine;
use godot::prelude::*;

mod local;
mod luau_sandbox;
mod mechanics;
mod networking;

struct RustExtension;

// adapted from https://github.com/2-3-5-41/godot_tokio @ https://github.com/xenacool/godot_tokio/commit/5de8ff41c2a1771499ef5b73d62495d2b3a288ac
#[gdextension]
unsafe impl ExtensionLibrary for RustExtension {
    fn on_stage_init(level: InitStage) {
        match level {
            InitStage::Scene => {
                let mut engine = Engine::singleton();

                engine.register_singleton(TokioRuntime::SINGLETON, &TokioRuntime::new_alloc());
            }
            _ => (),
        }
    }

    fn on_stage_deinit(level: InitStage) {
        match level {
            InitStage::Scene => {
                let mut engine = Engine::singleton();

                // Here is where we free our async runtime singleton from memory.
                if let Some(async_singleton) = engine.get_singleton(TokioRuntime::SINGLETON) {
                    engine.unregister_singleton(TokioRuntime::SINGLETON);
                    async_singleton.free();
                } else {
                    godot_warn!("Failed to free singleton -> {}", TokioRuntime::SINGLETON);
                }
            }
            _ => (),
        }
    }
}
