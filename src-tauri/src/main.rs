// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Elevated helper mode: `mask.exe --install-virtual-driver <inf>` is
    // spawned by the onboarding flow with admin rights, installs the virtual
    // audio driver, and exits (0 ok, 2 reboot required, 1 failed).
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--install-virtual-driver") {
        let code = match args.get(2) {
            Some(inf) => mask_lib::driver_install::install(inf),
            None => 1,
        };
        std::process::exit(code);
    }

    mask_lib::run()
}
