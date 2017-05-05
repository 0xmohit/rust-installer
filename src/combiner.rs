// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use flate2::read::GzDecoder;
use tar::Archive;

use super::Scripter;
use super::Tarballer;
use remove_dir_all::*;
use util::*;

actor!{
    #[derive(Debug)]
    pub struct Combiner {
        /// The name of the product, for display
        product_name: String = "Product",

        /// The name of the package, tarball
        package_name: String = "package",

        /// The directory under lib/ where the manifest lives
        rel_manifest_dir: String = "packagelib",

        /// The string to print after successful installation
        success_message: String = "Installed.",

        /// Places to look for legacy manifests to uninstall
        legacy_manifest_dirs: String = "",

        /// Installers to combine
        input_tarballs: String = "",

        /// Directory containing files that should not be installed
        non_installed_overlay: String = "",

        /// The directory to do temporary work
        work_dir: String = "./workdir",

        /// The location to put the final image and tarball
        output_dir: String = "./dist",
    }
}

impl Combiner {
    /// Combine the installer tarballs
    pub fn run(self) -> io::Result<()> {
        fs::create_dir_all(&self.work_dir)?;

        let package_dir = Path::new(&self.work_dir).join(&self.package_name);
        if package_dir.exists() {
            remove_dir_all(&package_dir)?;
        }
        fs::create_dir_all(&package_dir)?;

        // Merge each installer into the work directory of the new installer
        let components = fs::File::create(package_dir.join("components"))?;
        for input_tarball in self.input_tarballs.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            // Extract the input tarballs
            let input = fs::File::open(&input_tarball)?;
            let deflated = GzDecoder::new(input)?;
            Archive::new(deflated).unpack(&self.work_dir)?;

            let pkg_name = input_tarball.trim_right_matches(".tar.gz");
            let pkg_name = Path::new(pkg_name).file_name().unwrap();
            let pkg_dir = Path::new(&self.work_dir).join(&pkg_name);

            // Verify the version number
            let mut version = String::new();
            fs::File::open(pkg_dir.join("rust-installer-version"))?
                .read_to_string(&mut version)?;
            if version.trim().parse() != Ok(::RUST_INSTALLER_VERSION) {
                let msg = format!("incorrect installer version in {}", input_tarball);
                return Err(io::Error::new(io::ErrorKind::Other, msg));
            }

            // Move components to the new combined installer
            let mut pkg_components = String::new();
            fs::File::open(pkg_dir.join("components"))?
                .read_to_string(&mut pkg_components)?;
            for component in pkg_components.split_whitespace() {
                // All we need to do is move the component directory
                let component_dir = package_dir.join(&component);
                fs::rename(&pkg_dir.join(&component), &component_dir)?;

                // Merge the component name
                writeln!(&components, "{}", component)?;
            }
        }
        drop(components);

        // Write the installer version
        let version = fs::File::create(package_dir.join("rust-installer-version"))?;
        writeln!(&version, "{}", ::RUST_INSTALLER_VERSION)?;
        drop(version);

        // Copy the overlay
        if !self.non_installed_overlay.is_empty() {
            copy_recursive(self.non_installed_overlay.as_ref(), &package_dir)?;
        }

        // Generate the install script
        let output_script = package_dir.join("install.sh");
        let mut scripter = Scripter::default();
        scripter.product_name(self.product_name)
            .rel_manifest_dir(self.rel_manifest_dir)
            .success_message(self.success_message)
            .legacy_manifest_dirs(self.legacy_manifest_dirs)
            .output_script(output_script.to_str().unwrap());
        scripter.run()?;

        // Make the tarballs
        fs::create_dir_all(&self.output_dir)?;
        let output = Path::new(&self.output_dir).join(&self.package_name);
        let mut tarballer = Tarballer::default();
        tarballer.work_dir(self.work_dir)
            .input(self.package_name)
            .output(output.to_str().unwrap());
        tarballer.run()?;

        Ok(())
    }
}
