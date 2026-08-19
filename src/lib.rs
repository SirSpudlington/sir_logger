#[doc = include_str!("../README.md")]
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
use std::borrow::Cow;
use std::io;
use std::path::Path;
use std::sync::OnceLock;
use std::time::SystemTime;
use std::panic;

static PREVENT_MULTI_INIT: OnceLock<()> = OnceLock::new();

/// Create a `SirLoggerBuilder` with either the
/// current crate name or the one specified in
/// $root
#[macro_export]
macro_rules! setup {
    () => {
        SirLoggerBuilder::with_crate_root(env!("CARGO_PKG_NAME"))
    };

    ($root: literal) => {
        SirLoggerBuilder::with_crate_root($root)
    };
}

/// An instance of a builder for the logger.
pub struct SirLoggerBuilder {
    root: &'static str,
    level_override: Option<log::LevelFilter>,
    suppress: Vec<Cow<'static, str>>,
    internal: Vec<Cow<'static, str>>,
    chains: Vec<fern::Output>,
    handle_panics: bool
}

impl SirLoggerBuilder {
    /// It is recommended to use the `setup!` macro instead of
    /// this function to create a new logger. 
    pub fn with_crate_root(root: &'static str) -> Self {
        SirLoggerBuilder {
            root,
            level_override: None,
            suppress: Vec::new(),
            internal: Vec::new(),
            chains: Vec::new(),
            handle_panics: true
        }
    }

    /// Output logs to stderr
    pub fn use_stderr(&mut self) -> &mut Self {
        self.chains.push(std::io::stderr().into());
        self
    }

    /// Output logs to stdout
    pub fn use_stdout(&mut self) -> &mut Self {
        self.chains.push(std::io::stdout().into());
        self
    }

    /// Set a log file to output to
    pub fn log_file(&mut self, log_file: impl AsRef<Path>) -> io::Result<&mut Self> {
        self.chains.push(fern::log_file(log_file.as_ref())?.into());
        Ok(self)
    }

    /// Completely disable the panic handler
    /// 
    /// Useful if you are using a different panic handler
    pub fn no_panic_handler(&mut self) -> &mut Self {
        self.handle_panics = false;
        self
    }

    /// Override whatever is in the `RUST_LOG` env var and
    /// set the log level for internal crates to `level`
    pub fn log_level(&mut self, level: log::LevelFilter) -> &mut Self {
        self.level_override = Some(level);
        self
    }
    
    /// Completely disable logs for certain libraries
    pub fn suppress(&mut self, suppress: impl IntoIterator<Item = impl Into<Cow<'static, str>>>) -> &mut Self {
        self.suppress.extend(suppress.into_iter().map(Into::into));
        self
    }
    
    /// Mark certain packages as internal and treat them with the same log
    /// level as the root package.
    /// 
    /// Useful if you are using workspaces.
    pub fn internal(&mut self, internal: impl IntoIterator<Item = impl Into<Cow<'static, str>>>) -> &mut Self {
        self.internal.extend(internal.into_iter().map(Into::into));
        self
    }
    
    /// Add a custom fern output to the logger
    pub fn custom_output(&mut self, output: impl Into<fern::Output>) -> &mut Self {
        self.chains.push(output.into());
        self
    }

    /// Consume the builder and setup the logger
    pub fn setup(self) -> Result<(), log::SetLoggerError> {
        // This was not in the original, but you can never be *too* safe.
        if PREVENT_MULTI_INIT.get().is_some() {
            log::warn!("Attempted to initialize logger twice");
            return Ok(());
        }

        debug_assert!(!self.chains.is_empty(), "No outputs configured for logging. Did you mean to call `.stdout()` or `.stderr()`?");

        // Check if log level is overridden, if not, attempt to look
        // for the environment variable and fallback to `Info`
        let level = self.level_override.unwrap_or(
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
            .level_for(self.root, level);

        // Apply all the overrides.
        for pkg in self.internal.into_iter() {
            dispatch = dispatch.level_for(pkg, level);
        }

        for pkg in self.suppress.into_iter() {
            dispatch = dispatch.level_for(pkg, log::LevelFilter::Off);
        }

        for chain in self.chains {
            dispatch = dispatch.chain(chain);
        }

        // Apply all the logging info
        dispatch.apply()?;

        // Set a nicer looking panic hook, so incase there ever is a panic, it'll
        // be handled nicer.
        if self.handle_panics {
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
        }

        // This was not in the original, but you can never be *too* safe.
        PREVENT_MULTI_INIT
            .set(())
            .expect("Unable to set initialized flag");

        Ok(())
    }
}


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
/// sir_logger::setup(
///     // The log filter override, if `Some(value)`,
///     // the logger will use that value as the log level displayed.
///     // If `None`, then the logger will try to find the value in
///     // `RUST_LOG`, and then it'll default to `INFO`
///     Some(LevelFilter::Trace),
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
/// 
/// ```
#[deprecated(since="0.2.0", note="Prefer using the `setup!` macro with builder instead of the legacy style.")]
pub fn setup<const S: usize, const H: usize>(
    level_override: Option<log::LevelFilter>,
    suppress: [&'static str; S],
    high_priority: [&'static str; H],
    log_file: Option<&dyn AsRef<Path>>,
    root: &'static str,
) {
    let mut out = SirLoggerBuilder::with_crate_root(root);
    out.use_stdout();

    out.suppress(suppress);
    out.internal(high_priority);

    if let Some(over) = level_override {
        out.log_level(over);
    }

    if let Some(file) = log_file {
        out.log_file(file).unwrap();
    }

    out.setup().unwrap();
}
