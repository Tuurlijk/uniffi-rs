/* This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::bindings::RunScriptOptions;
use crate::cargo_metadata::CrateConfigSupplier;
use crate::library_mode::generate_bindings;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::Metadata;
use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::ffi::OsStr;
use std::process::{Command, Stdio};
use uniffi_testing::UniFFITestHelper;

/// Run Swift tests for a UniFFI test fixture
pub fn run_test(tmp_dir: &str, package_name: &str, script_file: &str) -> Result<()> {
    run_script(
        tmp_dir,
        package_name,
        script_file,
        vec![],
        &RunScriptOptions::default(),
    )
}

/// Run a Swift script
///
/// This function will set things up so that the script can import the UniFFI bindings for a crate
pub fn run_script(
    tmp_dir: &str,
    package_name: &str,
    script_file: &str,
    args: Vec<String>,
    options: &RunScriptOptions,
) -> Result<()> {
    let script_path = Utf8Path::new(script_file).canonicalize_utf8()?;
    let test_helper = UniFFITestHelper::new(package_name)?;
    let out_dir = test_helper.create_out_dir(tmp_dir, &script_path)?;
    let cdylib_path = test_helper.copy_cdylib_to_out_dir(&out_dir)?;
    let generated_sources = GeneratedSources::new(
        test_helper.crate_name(),
        &cdylib_path,
        test_helper.cargo_metadata(),
        &out_dir,
    )?;

    // Compile the generated sources together to create a single swift module
    compile_swift_module(
        &out_dir,
        &generated_sources.main_module,
        &generated_sources.generated_swift_files,
        &generated_sources.module_map,
        options,
    )?;

    // Run the test script against compiled bindings
    let mut command = create_command("swift", options);
    command
        .current_dir(&out_dir)
        .arg("-I")
        .arg(&out_dir)
        .arg("-L")
        .arg(&out_dir)
        .args(calc_library_args(&out_dir)?)
        .arg("-Xcc")
        .arg(format!(
            "-fmodule-map-file={}",
            generated_sources.module_map
        ))
        .arg(&script_path)
        .args(args);
    let status = command
        .spawn()
        .context("Failed to spawn `swiftc` when running test script")?
        .wait()
        .context("Failed to wait for `swiftc` when running test script")?;
    if !status.success() {
        bail!("running `swift` to run test script failed ({:?})", command)
    }
    Ok(())
}

fn compile_swift_module<T: AsRef<OsStr>>(
    out_dir: &Utf8Path,
    module_name: &str,
    sources: impl IntoIterator<Item = T>,
    module_map: &Utf8Path,
    options: &RunScriptOptions,
) -> Result<()> {
    let output_filename = format!("{DLL_PREFIX}testmod_{module_name}{DLL_SUFFIX}");
    let mut command = create_command("swiftc", options);
    command
        .current_dir(out_dir)
        .arg("-emit-module")
        .arg("-module-name")
        .arg(module_name)
        .arg("-o")
        .arg(output_filename)
        .arg("-emit-library")
        .arg("-Xcc")
        .arg(format!("-fmodule-map-file={module_map}"))
        .arg("-I")
        .arg(out_dir)
        .arg("-L")
        .arg(out_dir)
        .args(calc_library_args(out_dir)?)
        .args(sources);
    let status = command
        .spawn()
        .context("Failed to spawn `swiftc` when compiling bindings")?
        .wait()
        .context("Failed to wait for `swiftc` when compiling bindings")?;
    if !status.success() {
        bail!(
            "running `swiftc` to compile bindings failed ({:?})",
            command
        )
    };
    Ok(())
}

// Stores sources generated by `uniffi-bindgen-swift`
struct GeneratedSources {
    main_module: String,
    generated_swift_files: Vec<Utf8PathBuf>,
    module_map: Utf8PathBuf,
}

impl GeneratedSources {
    fn new(
        crate_name: &str,
        cdylib_path: &Utf8Path,
        cargo_metadata: Metadata,
        out_dir: &Utf8Path,
    ) -> Result<Self> {
        let sources = generate_bindings(
            cdylib_path,
            None,
            &super::SwiftBindingGenerator,
            &CrateConfigSupplier::from(cargo_metadata),
            None,
            out_dir,
            false,
        )?;
        let main_source = sources
            .iter()
            .find(|s| s.ci.crate_name() == crate_name)
            .unwrap();
        let main_module = main_source.config.module_name();
        let modulemap_glob = glob(&out_dir.join("*.modulemap"))?;
        let module_map = match modulemap_glob.len() {
            // write_bindings should have generated exactly 1 module map
            1 => modulemap_glob.into_iter().next().unwrap(),
            n => bail!("{n} modulemap files found in {out_dir}"),
        };

        Ok(GeneratedSources {
            main_module,
            generated_swift_files: glob(&out_dir.join("*.swift"))?,
            module_map,
        })
    }
}

fn create_command(program: &str, options: &RunScriptOptions) -> Command {
    let mut command = Command::new(program);
    if !options.show_compiler_messages {
        // This prevents most compiler messages, but not remarks
        command.arg("-suppress-warnings");
        // This gets the remarks.  Note: swift will eventually get a `-suppress-remarks` argument,
        // maybe we can eventually move to that
        command.stderr(Stdio::null());
    }
    command
}

// Wraps glob to use Utf8Paths and flattens errors
fn glob(globspec: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    glob::glob(globspec.as_str())?
        .map(|globresult| Ok(Utf8PathBuf::try_from(globresult?)?))
        .collect()
}

fn calc_library_args(out_dir: &Utf8Path) -> Result<Vec<String>> {
    let results = glob::glob(out_dir.join(format!("{DLL_PREFIX}*{DLL_SUFFIX}")).as_str())?;
    results
        .map(|globresult| {
            let path = Utf8PathBuf::try_from(globresult.unwrap())?;
            Ok(format!(
                "-l{}",
                path.file_name()
                    .unwrap()
                    .strip_prefix(DLL_PREFIX)
                    .unwrap()
                    .strip_suffix(DLL_SUFFIX)
                    .unwrap()
            ))
        })
        .collect()
}
