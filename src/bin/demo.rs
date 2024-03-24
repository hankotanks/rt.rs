fn main() {
    //pollster::block_on(rtrs::run());

    #[cfg(not(target_arch = "wasm32"))] {
        let _ = pollster::block_on(rtrs::run());
    }
    
    #[cfg(target_arch = "wasm32")] {
        poll_promise::spawn_local(rtrs::run());
    }
}