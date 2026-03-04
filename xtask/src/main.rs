use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        Some("build-fw") => build_fw(),
        Some("flash-fw") => flash_fw(),
        _ => {
            eprintln!("Usage: cargo xtask <task>");
            eprintln!();
            eprintln!("Tasks:");
            eprintln!("  build-fw   Build firmware for STM32G431CB");
            eprintln!("  flash-fw   Build + flash firmware via USB DFU");
            eprintln!();
            eprintln!("DFU mode: hold BOOT0 high and reset the device before flashing.");
            std::process::exit(1);
        }
    }
}

fn build_fw() {
    let status = cargo()
        .args(["build", "-p", "expresso-fw", "--target", "thumbv7em-none-eabihf", "--release"])
        .status()
        .expect("failed to run cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn flash_fw() {
    build_fw();

    let root = workspace_root();
    let elf_src = root.join("target/thumbv7em-none-eabihf/release/expresso-fw");
    // STM32CubeProgrammer requires a recognised file extension to detect the format.
    let elf = root.join("target/thumbv7em-none-eabihf/release/expresso-fw.elf");
    std::fs::copy(&elf_src, &elf).expect("failed to copy ELF to .elf");

    let programmer = find_programmer();

    eprintln!("Flashing {} ...", elf.display());
    eprintln!("Make sure the device is in DFU mode (hold BOOT0 high, then reset).");

    let status = Command::new(&programmer)
        .args(["-c", "port=USB1", "-w"])
        .arg(&elf)
        .args(["-v", "-rst"])
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", programmer.display()));

    if !status.success() {
        eprintln!("Flashing failed!");
        std::process::exit(status.code().unwrap_or(1));
    }

    eprintln!("Done! The device will boot the new firmware.");
}

/// Locate STM32_Programmer_CLI, searching PATH then known install locations.
fn find_programmer() -> PathBuf {
    // Check PATH first (works if the user added it themselves)
    let bin = if cfg!(windows) {
        "STM32_Programmer_CLI.exe"
    } else {
        "STM32_Programmer_CLI"
    };

    if which(bin).is_some() {
        return PathBuf::from(bin);
    }

    // Fall back to known install locations per OS
    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\ST\STM32CubeCLT\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe",
            r"C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/Applications/STMicroelectronics/STM32Cube/STM32CubeProgrammer/STM32CubeProgrammer.app/Contents/MacOs/bin/STM32_Programmer_CLI",
        ]
    } else {
        // Linux / WSL
        &[
            "/opt/stm32cubeprogrammer/bin/STM32_Programmer_CLI",
            "/usr/local/bin/STM32_Programmer_CLI",
        ]
    };

    for path in candidates {
        if Path::new(path).exists() {
            return PathBuf::from(path);
        }
    }

    eprintln!("error: STM32_Programmer_CLI not found.");
    eprintln!("Install STM32CubeProgrammer and make sure it is in PATH.");
    std::process::exit(1);
}

/// Check if a binary exists anywhere on PATH.
fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path).find_map(|dir| {
            let full = dir.join(bin);
            full.is_file().then_some(full)
        })
    })
}

/// Return the workspace root (parent of the directory containing this xtask).
fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is set by cargo and points to xtask/
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().expect("xtask must be inside workspace").to_owned()
}

fn cargo() -> Command {
    // Reuse the same cargo binary that invoked us
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    // Run from the workspace root so relative paths are correct
    cmd.current_dir(workspace_root());
    cmd
}
