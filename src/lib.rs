//! This crate provides a [`Pushd`] type that temporarily changes the current
//! directory.
//!
//! When a [`Pushd`] struct is created it will call [`env::set_current_dir`]
//! to change to the given directory. When the [`Pushd`] is dropped, it will
//! change back to the original directory.
//!
//! If the original directory doesn't exist, this error is ignored, since this
//! may be because the original directory was a temporary directory. All other
//! errors during drop cause a panic by default, but panics can be disabled
//! entirely by using the [`Pushd::new_no_panic`] constructor.
//!
//! # Examples
//!
//! ```
//! use pushd::Pushd;
//! use std::path::PathBuf;
//!
//! fn in_directory(path: PathBuf) {
//!     // When the current function exits and this variable is dropped, the
//!     // current directory will revert back to whatever it was before this
//!     // `Pushd` was created.
//!     let _pd = Pushd::new(path);
//!     // ...
//! }
//! ```
//!
//! # Panics
//!
//! The [`Pushd`] may panic if it cannot change back to the original directory
//! when it's dropped. Use the [`Pushd::new_no_panic`](Pushd::new_no_panic)
//! constructor to prevent this.
use log::{debug, warn};
use std::error::Error as StdError;
use std::{
    env, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

/// `Error` is an enum containing the structured errors that can be returned
/// by this module.
#[derive(Debug, Error)]
pub enum PushdError {
    /// Indicates that the current directory could not be retrieved. It wraps
    /// the [`io::Error`] returned by [`env::current_dir`].
    #[error("Could not get current directory: {source}")]
    GetCurrentDir {
        #[from]
        source: io::Error,
    },
    /// Indicates that the current directory could not be changed. It wraps
    /// the [`io::Error`] returned by [`env::set_current_dir`].
    #[error("Could not set current directory to {path}: {source}")]
    SetCurrentDir { path: PathBuf, source: io::Error },
}

/// A `Pushd` changes the current directory when it's created and returns to
/// the original current directory when it's dropped.
pub struct Pushd {
    orig: PathBuf,
    panic_on_err: bool,
    popped: bool,
}

impl Pushd {
    /// Constructs a new `Pushd` struct.
    ///
    /// This accepts any type that implements [`AsRef<Path>`].
    ///
    /// The `Pushd` returned by this constructor will panic if it cannot
    /// change back to its original directory when it is dropped.
    ///
    /// This will call
    /// [`log::debug!`](https://docs.rs/log/latest/log/macro.debug.html) to
    /// log the directory change.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Pushd, PushdError> {
        let cwd = env::current_dir()?;
        env::set_current_dir(path.as_ref()).map_err(|e| PushdError::SetCurrentDir {
            path: path.as_ref().to_owned(),
            source: e,
        })?;
        debug!(
            "set current dir to {} from {}",
            path.as_ref().display(),
            cwd.display(),
        );
        Ok(Pushd {
            orig: cwd,
            panic_on_err: true,
            popped: false,
        })
    }

    /// Constructs a new `Pushd` struct that will never panic.
    ///
    /// This accepts any type that implements `AsRef<Path>`.
    ///
    /// This will call
    /// [`log::debug!`](https://docs.rs/log/latest/log/macro.debug.html) to
    /// log the directory change.
    ///
    /// If the [`Pushd`] created by this constructor cannot change back to the
    /// original directory when it is dropped, then it will simply call
    /// [`log::warn!`](https://docs.rs/log/latest/log/macro.warn.html) instead
    /// of panicking.
    pub fn new_no_panic<P: AsRef<Path>>(path: P) -> Result<Pushd, PushdError> {
        let mut pd = Self::new(path)?;
        pd.panic_on_err = false;
        Ok(pd)
    }

    /// Changes back to the original directory the first time it is called. If
    /// this method is called repeatedly it will not do anything on subsequent
    /// calls.
    pub fn pop(&mut self) -> Result<(), PushdError> {
        if self.popped {
            return Ok(());
        }

        debug!("setting current dir back to {}", self.orig.display());
        env::set_current_dir(&self.orig).map_err(|e| PushdError::SetCurrentDir {
            path: self.orig.clone(),
            source: e,
        })?;
        self.popped = true;
        Ok(())
    }
}

impl Drop for Pushd {
    /// Changes back to the original directory.
    ///
    /// When the [`Pushd`] struct is dropped, it will change back to the
    /// original directory. If this fails, it's behavior is as follows:
    ///
    /// * If the [`Pushd`] was constructed with [`Pushd::new_no_panic`], it
    /// will log the error by calling
    /// [`log::warn!`](https://docs.rs/log/latest/log/macro.warn.html).
    ///
    /// * If the [`Pushd`] was constructed with [`Pushd::new`] and the error
    /// is an [`io::Error`] and the error's [`io::Error::kind`] method returns
    /// [`io::ErrorKind::NotFound`], it will do nothing.
    ///
    /// * Otherwise it will panic with the error from attempting to change the
    /// current directory.
    fn drop(&mut self) {
        if let Err(e) = self.pop() {
            if !self.panic_on_err {
                warn!("Could not return to original dir: {e}");
                return;
            }

            if let Some(s) = e.source() {
                if let Some(i) = s.downcast_ref::<io::Error>() {
                    if i.kind() == io::ErrorKind::NotFound {
                        return;
                    }
                }
            }

            panic!("Could not return to original dir: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    // Anything that does pushd must be run serially or else chaos ensues.
    use serial_test::serial;
    #[cfg(not(target_os = "windows"))]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(not(target_os = "windows"))]
    use std::panic;
    use tempfile::tempdir;

    #[test]
    #[serial]
    fn no_errors() -> Result<(), Box<dyn StdError>> {
        env::set_current_dir(env::var("CARGO_MANIFEST_DIR")?)?;

        let cwd = fs::canonicalize(env::current_dir()?)?;

        {
            let td = tempdir()?;
            let _pd = Pushd::new(td.path());
            assert_eq!(
                fs::canonicalize(env::current_dir()?)?,
                fs::canonicalize(td.path())?,
            );
        }
        assert_eq!(fs::canonicalize(env::current_dir()?)?, cwd);

        Ok(())
    }

    #[test]
    #[serial]
    fn no_errors_explicit_pop() -> Result<(), Box<dyn StdError>> {
        env::set_current_dir(env::var("CARGO_MANIFEST_DIR")?)?;

        let cwd = fs::canonicalize(env::current_dir()?)?;

        {
            let td = tempdir()?;
            let mut pd = Pushd::new(td.path())?;
            assert_eq!(
                fs::canonicalize(env::current_dir()?)?,
                fs::canonicalize(td.path())?,
            );
            pd.pop()?;
            assert_eq!(fs::canonicalize(env::current_dir()?)?, cwd);
        }

        assert_eq!(fs::canonicalize(env::current_dir()?)?, cwd);

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    #[serial]
    fn not_found_error_on_drop() -> Result<(), Box<dyn StdError>> {
        env::set_current_dir(env::var("CARGO_MANIFEST_DIR")?)?;

        let td1 = tempdir()?;
        env::set_current_dir(td1.path())?;

        {
            let td2 = tempdir()?;
            let _pd = Pushd::new(td2.path());
            assert_eq!(
                fs::canonicalize(env::current_dir()?)?,
                fs::canonicalize(td2.path())?,
            );
            // This should delete the original directory before the Pushd's
            // drop method is called.
            td1.close()?;
        }

        let cwd = env::current_dir();
        assert!(cwd.is_err());
        assert_eq!(cwd.unwrap_err().kind(), io::ErrorKind::NotFound);

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    #[serial]
    fn permissions_error_panic_on_drop() -> Result<(), Box<dyn StdError>> {
        let result = panic::catch_unwind(|| {
            let man_dir = match env::var("CARGO_MANIFEST_DIR") {
                Ok(md) => md,
                Err(e) => {
                    println!("Error getting CARGO_MANIFEST_DIR: {e}");
                    return;
                }
            };
            match env::set_current_dir(man_dir) {
                Ok(_) => (),
                Err(e) => {
                    println!("Error setting current dir to CARGO_MANIFEST_DIR: {e}");
                    return;
                }
            }

            let td1 = match tempdir() {
                Ok(td) => td,
                Err(e) => {
                    println!("Error creating tempdir: {e}");
                    return;
                }
            };
            match env::set_current_dir(td1.path()) {
                Ok(_) => (),
                Err(e) => {
                    println!("Error setting current dir to tempdir: {e}");
                    return;
                }
            }

            {
                let td2 = match tempdir() {
                    Ok(td) => td,
                    Err(e) => {
                        println!("Error creating tempdir: {e}");
                        return;
                    }
                };

                let _pd = Pushd::new(td2.path());
                let md = match fs::metadata(td1.path()) {
                    Ok(md) => md,
                    Err(e) => {
                        println!("Error getting metadata for tempdir: {e}");
                        return;
                    }
                };

                let mut perms = md.permissions();
                perms.set_mode(0o0400);
                match fs::set_permissions(td1.path(), perms) {
                    Ok(_) => (),
                    Err(e) => {
                        println!("Error setting permissions for tempdir: {e}");
                        return;
                    }
                }
            }

            println!("We should never get here");
        });

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .downcast_ref::<String>()
            .unwrap()
            .contains("Permission denied"));

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    #[serial]
    fn permissions_error_no_panic_on_drop() -> Result<(), Box<dyn StdError>> {
        env::set_current_dir(env::var("CARGO_MANIFEST_DIR")?)?;

        let td1 = tempdir()?;
        env::set_current_dir(td1.path())?;

        {
            let td2 = tempdir()?;
            let _pd = Pushd::new_no_panic(td2.path());
            assert_eq!(
                fs::canonicalize(env::current_dir()?)?,
                fs::canonicalize(td2.path())?,
            );
            let mut perms = fs::metadata(td1.path())?.permissions();
            perms.set_mode(0o0400);
            fs::set_permissions(td1.path(), perms)?;
        }

        let cwd = env::current_dir();
        assert!(cwd.is_err());
        assert_eq!(cwd.unwrap_err().kind(), io::ErrorKind::NotFound);

        Ok(())
    }
}
