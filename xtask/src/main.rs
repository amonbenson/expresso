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
    TestFw(TestFw),
    RunSw(RunSw),
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

/// Build firmware and run Renode integration tests.
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "test-fw")]
struct TestFw {}

/// Run desktop software in development mode (npm run tauri dev).
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "run-sw")]
struct RunSw {}

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
        Task::TestFw(_) => test_fw(),
        Task::RunSw(_) => run_sw(),
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
    cargo(&["test-fw"]);
}

fn test_fw() {
    // Build firmware ELF first.
    cargo(&["build-fw"]);

    let root = workspace_root();

    // STM32CubeProgrammer (and Renode) require a recognised .elf extension.
    let elf_src = root.join("target/thumbv7em-none-eabihf/release/expresso-fw");
    let elf = root.join("target/thumbv7em-none-eabihf/release/expresso-fw.elf");
    std::fs::copy(&elf_src, &elf).expect("failed to copy ELF to .elf");

    let renode_dir = find_renode();
    let python = find_python();
    let run_tests = renode_dir.join("tests/run_tests.py");
    let css = renode_dir.join("tests/robot.css");

    // Write results into target/ so they are covered by the root .gitignore.
    let results_dir = root.join("target/renode-results");
    std::fs::create_dir_all(&results_dir).expect("failed to create renode-results dir");

    let robot_tests_dir = root.join("tests/renode/tests");

    // Collect all .robot files; run_tests.py needs explicit file paths, not a directory.
    let mut robot_files: Vec<PathBuf> = std::fs::read_dir(&robot_tests_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", robot_tests_dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("robot"))
        .collect();
    robot_files.sort();

    if robot_files.is_empty() {
        eprintln!("error: no .robot files found in {}", robot_tests_dir.display());
        std::process::exit(1);
    }

    eprintln!("Running {} Renode test file(s) ...", robot_files.len());

    let status = Command::new(&python)
        .env("PYTHONIOENCODING", "utf-8")
        .arg(&run_tests)
        .arg("--css-file")
        .arg(&css)
        .arg("--robot-framework-remote-server-full-directory")
        .arg(&renode_dir)
        .arg("-r")
        .arg(&results_dir)
        .args(&robot_files)
        .current_dir(&root)
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", python.display()));

    if !status.success() {
        eprintln!("Renode tests failed! Results in {}", results_dir.display());
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// Locate the Renode installation directory (the one containing `tests/run_tests.py`).
///
/// Search order:
///   1. `RENODE_DIR` environment variable (CI override).
///   2. Known install locations for the current OS.
fn find_renode() -> PathBuf {
    if let Ok(dir) = std::env::var("RENODE_DIR") {
        let p = PathBuf::from(dir);
        if p.join("tests/run_tests.py").exists() {
            return p;
        }
        eprintln!("warning: RENODE_DIR set but tests/run_tests.py not found there");
    }

    let candidates: &[&str] = if cfg!(windows) {
        &[r"C:\Program Files\Renode"]
    } else if cfg!(target_os = "macos") {
        &["/Applications/Renode.app/Contents/MacOS"]
    } else {
        &["/opt/renode", "/usr/lib/renode"]
    };

    for path in candidates {
        let p = PathBuf::from(path);
        if p.join("tests/run_tests.py").exists() {
            return p;
        }
    }

    eprintln!("error: Renode not found.");
    eprintln!(
        "Install Renode from https://renode.io or set RENODE_DIR to the installation directory."
    );
    std::process::exit(1);
}

/// Locate a Python 3 interpreter that has `robotframework` installed.
///
/// Search order:
///   1. `RENODE_PYTHON` environment variable (CI override).
///   2. `python3` / `python` on PATH (Linux / macOS).
///   3. Known Miniconda / Anaconda / CPython install paths (Windows).
fn find_python() -> PathBuf {
    if let Ok(py) = std::env::var("RENODE_PYTHON") {
        return PathBuf::from(py);
    }

    if !cfg!(windows) {
        for bin in ["python3", "python"] {
            if which(bin).is_some() {
                return PathBuf::from(bin);
            }
        }
        eprintln!("error: Python 3 not found. Install Python 3 and robotframework.");
        std::process::exit(1);
    }

    // Windows: prefer PATH, but skip the Windows Store stub
    // (C:\Users\…\AppData\Local\Microsoft\WindowsApps\python.exe) which just
    // opens the Microsoft Store instead of running Python.
    if let Some(p) = which("python.exe") {
        let path_str = p.to_string_lossy().to_lowercase();
        if !path_str.contains("windowsapps") {
            return PathBuf::from("python.exe");
        }
    }

    // User-profile conda installs.
    if let Ok(profile) = std::env::var("USERPROFILE") {
        for subdir in ["miniconda3", "Miniconda3", "anaconda3", "Anaconda3"] {
            let p = PathBuf::from(&profile).join(subdir).join("python.exe");
            if p.exists() {
                return p;
            }
        }
    }

    // System-wide conda installs.
    for path in &[
        r"C:\ProgramData\miniconda3\python.exe",
        r"C:\ProgramData\Miniconda3\python.exe",
        r"C:\ProgramData\anaconda3\python.exe",
        r"C:\Python313\python.exe",
        r"C:\Python312\python.exe",
        r"C:\Python311\python.exe",
        r"C:\Python310\python.exe",
    ] {
        let p = PathBuf::from(path);
        if p.exists() {
            return p;
        }
    }

    eprintln!("error: Python not found.");
    eprintln!("Install Python 3 (or set RENODE_PYTHON) and run: pip install robotframework");
    std::process::exit(1);
}

fn run_sw() {
    npm(&["install"]);
    npm(&["run", "tauri", "dev"]);
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
    // On Windows npm is a batch script (npm.cmd), not a plain executable.
    let bin = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let status = Command::new(bin)
        .args(args)
        .current_dir(workspace_root().join("sw"))
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
