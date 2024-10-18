// Copyright 2024 Red Hat Inc
//
// SPDX-License-Identifier: Apache-2.0 or BSD-3-Clause

#![deny(
    clippy::undocumented_unsafe_blocks,
    /* groups */
    clippy::correctness,
    clippy::suspicious,
    clippy::complexity,
    clippy::perf,
    clippy::style,
    clippy::nursery,
    //* restriction */
    clippy::dbg_macro,
    clippy::rc_buffer,
    clippy::as_underscore,
    clippy::assertions_on_result_states,
    //* pedantic */
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::ptr_as_ptr,
    clippy::bool_to_int_with_if,
    clippy::borrow_as_ptr,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_ptr_alignment,
    clippy::naive_bytecount
)]
#![allow(
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening
)]

#[cfg(target_env = "gnu")]
pub mod device;
#[cfg(target_env = "gnu")]
pub mod protocol;
#[cfg(target_env = "gnu")]
pub mod virtio_gpu;

use std::path::PathBuf;

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum GpuMode {
    VirglRenderer,
    Gfxstream,
}

#[derive(Debug, Clone)]
/// This structure is the public API through which an external program
/// is allowed to configure the backend.
pub struct GpuConfig {
    /// vhost-user Unix domain socket
    socket_path: PathBuf,
    gpu_mode: GpuMode,
}

impl GpuConfig {
    /// Create a new instance of the GpuConfig struct, containing the
    /// parameters to be fed into the gpu-backend server.
    pub const fn new(socket_path: PathBuf, gpu_mode: GpuMode) -> Self {
        Self {
            socket_path,
            gpu_mode,
        }
    }

    /// Return the path of the unix domain socket which is listening to
    /// requests from the guest.
    pub fn get_socket_path(&self) -> PathBuf {
        self.socket_path.to_path_buf()
    }

    pub const fn gpu_mode(&self) -> GpuMode {
        self.gpu_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_config() {
        // Test the creation of GpuConfig struct
        let socket_path = PathBuf::from("/tmp/socket");
        let gpu_config = GpuConfig::new(socket_path.clone(), GpuMode::VirglRenderer);
        assert_eq!(gpu_config.get_socket_path(), socket_path);
    }
}
