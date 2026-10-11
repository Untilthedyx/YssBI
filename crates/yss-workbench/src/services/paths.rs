use anyhow::{Context, Result};
use directories::BaseDirs;
use std::path::{Path, PathBuf};
use yss_application::runtime::ApplicationPaths;

const APPLICATION_ID: &str = "com.zjy.yssbi";

pub(crate) struct NativePaths {
    pub application: ApplicationPaths,
    pub logs: PathBuf,
}

impl NativePaths {
    pub fn resolve() -> Result<Self> {
        let base =
            BaseDirs::new().context(crate::text::t("native.services.dataDirectoryUnavailable"))?;
        let (data, logs) = if let Some(path) = std::env::var_os("YSSBI_APP_DATA_DIR") {
            let data = PathBuf::from(path);
            anyhow::ensure!(
                data.is_absolute(),
                crate::text::t("native.services.absolutePathRequired")
            );
            let logs = data.join("logs");
            (data, logs)
        } else {
            let data = base.data_dir().join(APPLICATION_ID);
            let logs = if cfg!(target_os = "macos") {
                base.home_dir().join("Library/Logs").join(APPLICATION_ID)
            } else {
                base.data_local_dir().join(APPLICATION_ID).join("logs")
            };
            (data, logs)
        };
        let executable = std::env::current_exe()?;
        let executable_directory = executable
            .parent()
            .context(crate::text::t("native.services.executableParentMissing"))?;
        let samples = samples_directory(executable_directory);
        Ok(Self {
            application: ApplicationPaths {
                app_data_dir: data,
                samples_dir: samples,
            },
            logs,
        })
    }
}

fn samples_directory(executable_directory: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    if executable_directory.ends_with("Contents/MacOS") {
        let bundled = executable_directory.join("../Resources/samples");
        if bundled.is_dir() {
            return bundled;
        }
    }
    let adjacent = executable_directory.join("resources/samples");
    if adjacent.is_dir() {
        adjacent
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/samples")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_samples_take_precedence_over_checkout() -> Result<()> {
        let installation = tempfile::tempdir()?;
        let samples = installation.path().join("resources/samples");
        std::fs::create_dir_all(&samples)?;
        assert_eq!(samples_directory(installation.path()), samples);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn app_bundle_uses_contents_resources() -> Result<()> {
        let installation = tempfile::tempdir()?;
        let executable_directory = installation.path().join("YssBI.app/Contents/MacOS");
        let samples = installation
            .path()
            .join("YssBI.app/Contents/Resources/samples");
        std::fs::create_dir_all(&executable_directory)?;
        std::fs::create_dir_all(&samples)?;
        assert_eq!(
            samples_directory(&executable_directory).canonicalize()?,
            samples.canonicalize()?,
        );
        Ok(())
    }
}
