use std::{
    fs, io,
    path::{Path, PathBuf},
};

use inkpaper_app::{AppPlatform, FrontlightSetting, PlatformEntry};

use crate::{fake_fs::simulator_listing, host_epub::HostFileSource};

pub(super) struct SimulatorPlatform {
    state_directory: PathBuf,
}

impl SimulatorPlatform {
    pub(super) fn new() -> Self {
        let state_directory = simulator_state_directory();

        fs::create_dir_all(&state_directory).expect("simulator state directory must be creatable");

        Self { state_directory }
    }

    fn state_path(&self, name: &str) -> PathBuf {
        self.state_directory.join(name)
    }
}

impl AppPlatform for SimulatorPlatform {
    type Error = io::Error;
    type RandomAccessSource = HostFileSource;

    async fn list_directory(&mut self, path: &str) -> Result<Vec<PlatformEntry>, Self::Error> {
        simulator_listing(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "simulated directory does not exist",
            )
        })
    }

    async fn open_random_access(
        &mut self,
        path: &str,
    ) -> Result<Self::RandomAccessSource, Self::Error> {
        let host_path = simulator_epub_path(path).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "simulated EPUB does not exist")
        })?;

        HostFileSource::open(&host_path)
    }

    async fn set_frontlight(&mut self, _: FrontlightSetting) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn load_state(
        &mut self,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let path = self.state_path(name);

        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };

        if bytes.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted state is too large",
            ));
        }

        Ok(Some(bytes))
    }

    async fn save_state(&mut self, name: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        let path = self.state_path(name);
        let temporary = path.with_extension("tmp");

        fs::write(&temporary, bytes)?;
        fs::rename(temporary, path)?;

        Ok(())
    }

    async fn enter_usb_drive(&mut self) -> Result<(), Self::Error> {
        // The simulator only exercise the app-side USB drive state. It does not expose
        // a host block device
        Ok(())
    }
}

fn simulator_state_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("INKPAPER_SIM_STATE_DIR") {
        return PathBuf::from(path);
    }

    std::env::temp_dir()
        .join("inkpaper-simulator")
        .join(".inkpaper")
}

fn simulator_epub_path(path: &str) -> Option<PathBuf> {
    let file_name = path.strip_prefix("/Fixtures/")?;

    let relative = Path::new(file_name);

    if relative.components().count() != 1 {
        return None;
    }

    if relative
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("epub")
    {
        return None;
    }

    Some(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(relative),
    )
}
