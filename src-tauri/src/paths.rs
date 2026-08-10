use std::path::{Path, PathBuf};

pub fn normalize_path_string(value: &str) -> String {
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = value.strip_prefix(r"\\?\") {
        return rest.to_owned();
    }
    if let Some(rest) = value.strip_prefix(r"\??\") {
        return rest.to_owned();
    }
    value.to_owned()
}

pub fn normalize_path(path: PathBuf) -> PathBuf {
    PathBuf::from(normalize_path_string(&path.to_string_lossy()))
}

pub fn path_string(path: &Path) -> String {
    normalize_path_string(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_windows_verbatim_drive_prefix() {
        assert_eq!(
            normalize_path_string(r"\\?\C:\Servers\Survival"),
            r"C:\Servers\Survival"
        );
    }

    #[test]
    fn converts_windows_verbatim_unc_prefix() {
        assert_eq!(
            normalize_path_string(r"\\?\UNC\host\share\server"),
            r"\\host\share\server"
        );
    }

    #[test]
    fn leaves_regular_paths_unchanged() {
        assert_eq!(
            normalize_path_string(r"D:\Minecraft\Paper"),
            r"D:\Minecraft\Paper"
        );
    }
}
