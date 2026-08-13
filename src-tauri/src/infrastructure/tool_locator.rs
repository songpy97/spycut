use std::{env, path::PathBuf};

use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaTool {
    Ffmpeg,
    Ffprobe,
}

impl MediaTool {
    pub fn executable_name(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::Ffprobe => "ffprobe",
        }
    }

    fn override_variable(self) -> &'static str {
        match self {
            Self::Ffmpeg => "SPYCUT_FFMPEG_PATH",
            Self::Ffprobe => "SPYCUT_FFPROBE_PATH",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ToolLocatorError {
    #[error("{0} override does not point to a file: {1}")]
    InvalidOverride(&'static str, String),
    #[error("{0} was not found on PATH")]
    NotFound(&'static str),
}

pub fn locate_media_tool(tool: MediaTool) -> Result<PathBuf, ToolLocatorError> {
    let override_name = tool.override_variable();
    if let Some(value) = env::var_os(override_name) {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
        return Err(ToolLocatorError::InvalidOverride(
            override_name,
            path.to_string_lossy().into_owned(),
        ));
    }

    let executable = tool.executable_name();

    if let Ok(current_exe) = env::current_exe()
        && let Some(directory) = current_exe.parent()
        && let Some(adjacent) = find_in_directory(directory, executable)
    {
        return Ok(adjacent);
    }

    let directories = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    let directories = {
        let mut directories = directories;
        directories.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
        ]);
        directories
    };

    for directory in directories {
        if let Some(candidate) = find_in_directory(&directory, executable) {
            return Ok(candidate);
        }
    }

    Err(ToolLocatorError::NotFound(executable))
}

pub fn media_command(executable: impl AsRef<std::ffi::OsStr>) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new(executable);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(executable)
    }
}

fn find_in_directory(directory: &std::path::Path, executable: &str) -> Option<PathBuf> {
    let candidate = directory.join(executable);
    if candidate.is_file() {
        return Some(candidate);
    }
    #[cfg(windows)]
    {
        let candidate = directory.join(format!("{executable}.exe"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
