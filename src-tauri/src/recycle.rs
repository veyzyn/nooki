use std::path::Path;

use crate::error::{Error, Result};

#[cfg(windows)]
pub fn move_to_recycle_bin(path: &Path) -> Result<()> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL,
                COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
            UI::Shell::{
                FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName,
                FOFX_ADDUNDORECORD, FOFX_RECYCLEONDELETE, FOF_NO_UI,
            },
        },
    };

    if !path.exists() {
        return Err(Error::NotFound(
            "The server folder no longer exists.".into(),
        ));
    }

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let initialize =
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    if initialize.is_err() && initialize != RPC_E_CHANGED_MODE {
        return Err(Error::Internal(format!(
            "Windows could not initialize the Recycle Bin operation ({initialize:?})."
        )));
    }
    let _com = ComGuard(initialize.is_ok());

    let display_path = path.to_string_lossy();
    let shell_path = display_path
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| display_path.strip_prefix(r"\\?\").map(str::to_owned))
        .unwrap_or_else(|| display_path.into_owned());
    let wide = OsStr::new(&shell_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    let operation_result = unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation as *const _, None, CLSCTX_ALL).map_err(|error| {
                Error::Internal(format!("Windows could not open the Recycle Bin: {error}"))
            })?;
        operation
            .SetOperationFlags(FOF_NO_UI | FOFX_ADDUNDORECORD | FOFX_RECYCLEONDELETE)
            .map_err(|error| {
                Error::Internal(format!("Windows rejected the Recycle Bin options: {error}"))
            })?;
        let item: IShellItem =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).map_err(|error| {
                Error::Internal(format!("Windows could not read the server folder: {error}"))
            })?;
        operation.DeleteItem(&item, None).map_err(|error| {
            Error::Internal(format!(
                "Windows could not queue the folder for recycling: {error}"
            ))
        })?;
        operation.PerformOperations().map_err(|error| {
            Error::Internal(format!(
                "Windows could not recycle the server folder: {error}"
            ))
        })?;
        operation
            .GetAnyOperationsAborted()
            .map_err(|error| {
                Error::Internal(format!(
                    "Windows could not confirm the Recycle Bin operation: {error}"
                ))
            })?
            .as_bool()
    };

    // The Shell can report an aborted sub-operation after the requested folder was
    // successfully moved. The source path is the authoritative completion check.
    if !path.exists() {
        return Ok(());
    }
    if operation_result {
        return Err(Error::Conflict(
            "Windows refused to move this folder to the Recycle Bin. Close any Explorer windows or programs using the server folder, then try again.".into(),
        ));
    }
    Err(Error::Internal(
        "Windows reported success, but the server folder is still present.".into(),
    ))
}

#[cfg(not(windows))]
pub fn move_to_recycle_bin(path: &Path) -> Result<()> {
    trash::delete(path).map_err(|error| Error::Internal(error.to_string()))
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn converts_verbatim_paths_for_the_windows_shell() {
        let drive = r"\\?\C:\Users\test\server";
        let unc = r"\\?\UNC\host\share\server";
        assert_eq!(
            drive.strip_prefix(r"\\?\").unwrap(),
            r"C:\Users\test\server"
        );
        assert_eq!(
            unc.strip_prefix(r"\\?\UNC\")
                .map(|rest| format!(r"\\{rest}"))
                .unwrap(),
            r"\\host\share\server"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "moves a temporary probe into the real Windows Recycle Bin"]
    fn recycles_a_real_directory_on_windows() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("nooki-recycle-probe");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("probe.txt"), b"Nooki recycle test").unwrap();
        super::move_to_recycle_bin(&target).unwrap();
        assert!(!target.exists());
    }
}
