///
/// The `sir_logger` crate is a simple, logging crate designed for debugging
/// and testing. All documentation is in the `setup` function.
///
//
// `sir_logger` - A simple logging library for rust
// 
// Copyright (C) 2025  SirSpudlington
// 
// This library is free software; you can redistribute it and/or
// modify it under the terms of the GNU Lesser General Public
// License as published by the Free Software Foundation; either
// version 2.1 of the License, or (at your option) any later version.
// 
// This library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// Lesser General Public License for more details.
// 
// You should have received a copy of the GNU Lesser General Public
// License along with this library; if not, write to the Free Software
// Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301
// USA


use fern::colors::{Color, ColoredLevelConfig};
use log::{LevelFilter, debug, error};
use std::cell::OnceCell;
use std::panic;
use std::path::Path;
use std::time::SystemTime;

const PREVENT_MULTI_INIT: OnceCell<()> = OnceCell::new();

/// Setup the logger, you should only run this
/// function **once**.
///
/// If `level_override` is `Some(_)`, then the environment variable
/// `RUST_LOG` will be ignored.
///
/// The specified log level will only apply to other crates if one
/// of `trace`, `error` or `off`. Unless overridden `warn`
/// is the default for external crates.
/// This behaviour can be overridden by using the `suppress` or
/// `high_priority` parameters.
///
/// 9/10 times, root should be the output of `env!("CARGO_PKG_NAME")`,
/// if using workspaces, put the names of extra crates into `high_priority`
/// 
/// ## Example
/// 
/// ```rust
/// fn main() {
///     sir_logger::setup(
///         // The log filter override, if `Some(value)`,
///         // the logger will use that value as the log level displayed.
///         // If `None`, then the logger will try to find the value in
///         // `RUST_LOG`, and then it'll default to `INFO`
///         Some(LevelFilter::Trace),
/// 
///         // The names of crates that should be disabled for the logger
///         ["very_verbose_crate"],
/// 
///         // The names of libraries that should be at the same log
///         // level as the main program.
///         ["super_important_crate"],
/// 
///         // A path to a file to store logs, or `None`
///         Some("path/to/log.txt"),
/// 
///         // The name of this executable, this'll help the library
///         // set the correct log level for all crates.
///         env!("CARGO_PKG_NAME")
///     );
/// }
/// 
/// ```
pub fn setup<const S: usize, const H: usize>(
    level_override: Option<log::LevelFilter>,
    suppress: [&'static str; S],
    high_priority: [&'static str; H],
    log_file: Option<&dyn AsRef<Path>>,
    root: &'static str,
) {
    // This was not in the original, but you can never be *too* safe.
    if PREVENT_MULTI_INIT.get().is_some() {
        log::warn!("Attempted to initialize logger twice, ensure you call `setup` once.");
        return;
    }

    // Check if log level is overridden, if not, attempt to look
    // for the environment variable and fallback to `Info`
    let level = level_override.unwrap_or(
        std::env::var("RUST_LOG")
            .ok()
            .and_then(|f| f.to_uppercase().parse::<LevelFilter>().ok())
            .unwrap_or(LevelFilter::Info),
    );

    // Setup the colors of each level, this'll only be used when
    // printing the name of the log level e.g. "INFO".
    let colors_level = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::White)
        .trace(Color::BrightBlack);

    // Declare the main logging module
    let mut dispatch = fern::Dispatch::new()
        // Tell fern how to format logs nicely.
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[\x1B[34m{date}\x1B[0m {color_line}{level}\x1B[0m \x1B[32m{target}\x1B[0m] {message}",
                color_line = format_args!(
                    "\x1B[{}m",
                    colors_level.get_color(&record.level()).to_fg_str()
                ),
                date = humantime::format_rfc3339_seconds(SystemTime::now()),
                target = record.target(),
                level = colors_level.color(record.level()),
                message = message,
            ));
        })
        // Setup the default logging levels for all crates.
        .level(match level {
            log::LevelFilter::Trace => {
                log::LevelFilter::Trace
            }
            log::LevelFilter::Error => {
                log::LevelFilter::Error
            }
            log::LevelFilter::Off => {
                log::LevelFilter::Off
            }
            _ => {
                log::LevelFilter::Warn
            }
        })
        // Override the main crate to have different
        // log levels.
        .level_for(root, level)

        // Ensure that stdout gets logging info
        .chain(std::io::stdout());


    // Apply all the overrides.
    for pkg in high_priority.into_iter() {
        dispatch = dispatch.level_for(pkg, level);
    }

    for pkg in suppress.into_iter() {
        dispatch = dispatch.level_for(pkg, log::LevelFilter::Off);
    }

    // If the log file is be set, use it.
    if let Some(log_file) = log_file {
        dispatch = dispatch.chain(fern::log_file(log_file).unwrap());
    }

    // Apply all the logging info
    dispatch.apply().unwrap();

    // Set a nicer looking panic hook, so incase there ever is a panic, it'll
    // be handled nicer.
    panic::set_hook(Box::new(|info| {
        // Print debug info and where the panic happened.
        if let Some(location) = info.location() {
            debug!(
                "panic occurred in file '{}:{}'",
                location.file(),
                location.line()
            );
        }

        // Try to downcast the panic error object into a `&str` or `String`,
        // if this fails, just debug-print the error.
        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => &format!("{:?}", info.payload()),
            },
        };

        error!("{msg}");

        // Exit with a failure error code
        std::process::exit(1);
    }));

    // This was not in the original, but you can never be *too* safe.
    PREVENT_MULTI_INIT
        .set(())
        .expect("Unable to set initialized flag");
}
