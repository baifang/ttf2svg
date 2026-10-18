# Building on Windows

The recommended toolchain is `x86_64-pc-windows-msvc`, which requires the
Visual Studio C++ workload **and** a Windows SDK. `rustup` usually detects both
automatically.

If `cargo build` fails with `linker link.exe not found` or
`cannot open input file 'kernel32.lib'`, the Windows SDK is missing.

## Option 1: install the Windows SDK (recommended)

Open the Visual Studio Installer, choose *Modify*, and enable
*Windows 10/11 SDK* under *Individual components*. Then re-run the build.

## Option 2: use MinGW-w64 without admin rights

This is useful on machines where Visual Studio cannot be modified.

1. Download a MinGW-w64 build, for example
   [winlibs](https://winlibs.com/) (choose the UCRT, POSIX threads, SEH variant
   for x86-64) and extract it to a path such as `C:\mingw64`.

2. Add the GNU target:

   ```bash
   rustup target add x86_64-pc-windows-gnu
   ```

3. Copy `.cargo/config.toml.example` to `.cargo/config.toml` and point
   `linker` at your `x86_64-w64-mingw32-gcc.exe`. The file is ignored by git,
   so it stays local.

4. Build:

   ```bash
   cargo build --release
   # target/x86_64-pc-windows-gnu/release/
   ```

To switch back to MSVC, delete `.cargo/config.toml` and run
`rustup default stable-x86_64-pc-windows-msvc` if you had changed the
toolchain default.
