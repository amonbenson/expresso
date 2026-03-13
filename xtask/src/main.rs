use std::path::{Path, PathBuf};
use std::process::Command;

/// Expresso workspace task runner.
#[derive(argh::FromArgs)]
struct Args {
    #[argh(subcommand)]
    task: Task,
}

#[derive(argh::FromArgs)]
#[argh(subcommand)]
enum Task {
    CheckAll(CheckAll),
    BuildAll(BuildAll),
    TestAll(TestAll),
    FlashFw(FlashFw),
    DevSw(DevSw),
    Cubemx(Cubemx),
}

/// Check lib + firmware + software.
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "check-all")]
struct CheckAll {}

/// Build lib + firmware + software.
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "build-all")]
struct BuildAll {}

/// Test lib + software.
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "test-all")]
struct TestAll {}

/// Build + flash firmware via USB DFU (device must be in DFU mode first).
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "flash-fw")]
struct FlashFw {}

/// Run desktop software in development mode (npm run tauri dev).
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "dev-sw")]
struct DevSw {}

/// Open fw/expresso.ioc in STM32CubeMX.
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "cubemx")]
struct Cubemx {}

fn main() {
    let args: Args = argh::from_env();
    match args.task {
        Task::CheckAll(_) => check_all(),
        Task::BuildAll(_) => build_all(),
        Task::TestAll(_) => test_all(),
        Task::FlashFw(_) => flash_fw(),
        Task::DevSw(_) => dev_sw(),
        Task::Cubemx(_) => open_cubemx(),
    }
}

fn build_all() {
    cargo(&["build-lib"]);
    cargo(&["build-fw"]);
    cargo(&["build-sw"]);
}

fn check_all() {
    cargo(&["check-lib"]);
    cargo(&["check-fw"]);
    cargo(&["check-sw"]);
}

fn test_all() {
    cargo(&["test-lib"]);
    cargo(&["test-sw"]);
}

fn dev_sw() {
    npm(&["install", "--prefix", "sw"]);
    npm(&["run", "--prefix", "sw", "tauri", "dev"]);
}

fn flash_fw() {
    cargo(&["build-fw"]);

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
        .arg("-v")
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", programmer.display()));

    if !status.success() {
        eprintln!("Flashing failed!");
        std::process::exit(status.code().unwrap_or(1));
    }

    eprintln!("Done! The device will boot the new firmware.");
}

fn open_cubemx() {
    let ioc = workspace_root().join("fw/expresso.ioc");
    let cubemx = find_cubemx();

    eprintln!("Opening {} in STM32CubeMX ...", ioc.display());

    let status = Command::new(&cubemx)
        .arg(&ioc)
        .status()
        .unwrap_or_else(|e| panic!("failed to launch {}: {e}", cubemx.display()));

    if !status.success() {
        eprintln!("STM32CubeMX exited with an error.");
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// Locate STM32CubeMX, searching PATH then known install locations.
fn find_cubemx() -> PathBuf {
    let bin = if cfg!(windows) {
        "STM32CubeMX.exe"
    } else {
        "STM32CubeMX"
    };

    if which(bin).is_some() {
        return PathBuf::from(bin);
    }

    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\ST\STM32CubeMX\STM32CubeMX.exe",
            r"C:\ST\STM32CubeCLT\STM32CubeMX\STM32CubeMX.exe",
            r"C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeMX\STM32CubeMX.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/Applications/STMicroelectronics/STM32CubeMX.app/Contents/MacOs/STM32CubeMX",
            "/Applications/STM32CubeMX.app/Contents/MacOs/STM32CubeMX",
        ]
    } else {
        // Linux / WSL
        &[
            "/opt/ST/STM32CubeMX/STM32CubeMX",
            "/usr/local/STM32CubeMX/STM32CubeMX",
        ]
    };

    for path in candidates {
        if Path::new(path).exists() {
            return PathBuf::from(path);
        }
    }

    eprintln!("error: STM32CubeMX not found.");
    eprintln!("Install STM32CubeMX and make sure it is in PATH.");
    std::process::exit(1);
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
    manifest
        .parent()
        .expect("xtask must be inside workspace")
        .to_owned()
}

fn npm(args: &[&str]) {
    let status = Command::new("npm")
        .args(args)
        .status()
        .expect("failed to run npm");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn cargo(args: &[&str]) {
    let status = cargo_cmd()
        .args(args)
        .status()
        .expect("failed to run cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn cargo_cmd() -> Command {
    // Reuse the same cargo binary that invoked us
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    // Run from the workspace root so relative paths are correct
    cmd.current_dir(workspace_root());
    cmd
}
