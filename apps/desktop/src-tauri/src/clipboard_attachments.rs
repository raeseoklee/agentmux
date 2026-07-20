use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::Manager;

const CLIPBOARD_IMAGE_DIRECTORY: &str = "clipboard";
const CLIPBOARD_IMAGE_PREFIX: &str = "clipboard-image-";
const CLIPBOARD_IMAGE_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_CLIPBOARD_IMAGE_BYTES: usize = 256 * 1024 * 1024;

static CLIPBOARD_IMAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardAttachmentPayload {
    pub kind: &'static str,
    pub paths: Vec<String>,
}

impl ClipboardAttachmentPayload {
    fn none() -> Self {
        Self {
            kind: "none",
            paths: Vec::new(),
        }
    }
}

pub async fn materialize(app: tauri::AppHandle) -> Result<ClipboardAttachmentPayload, String> {
    let attachment_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?
        .join(CLIPBOARD_IMAGE_DIRECTORY);

    tauri::async_runtime::spawn_blocking(move || materialize_windows(&attachment_root))
        .await
        .map_err(|error| error.to_string())?
}

fn materialize_windows(root: &Path) -> Result<ClipboardAttachmentPayload, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;

    if let Ok(paths) = clipboard.get().file_list() {
        let paths = paths
            .into_iter()
            .map(path_to_string)
            .filter(|path| !path.trim().is_empty())
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            return Ok(ClipboardAttachmentPayload {
                kind: "files",
                paths,
            });
        }
    }

    let Ok(image) = clipboard.get_image() else {
        return Ok(ClipboardAttachmentPayload::none());
    };
    let path = save_clipboard_image(root, image.width, image.height, image.bytes.as_ref())?;
    cleanup_expired_images(root, Some(&path));
    Ok(ClipboardAttachmentPayload {
        kind: "image",
        paths: vec![path_to_string(path)],
    })
}

fn save_clipboard_image(
    root: &Path,
    width: usize,
    height: usize,
    rgba: &[u8],
) -> Result<PathBuf, String> {
    let width = u32::try_from(width).map_err(|_| "clipboard image is too wide".to_string())?;
    let height = u32::try_from(height).map_err(|_| "clipboard image is too tall".to_string())?;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "clipboard image dimensions overflow".to_string())?;
    if expected == 0 || expected != rgba.len() || expected > MAX_CLIPBOARD_IMAGE_BYTES {
        return Err("clipboard image has invalid or unsupported dimensions".to_string());
    }

    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let path = next_clipboard_image_path(root);
    image::save_buffer_with_format(
        &path,
        rgba,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|error| error.to_string())?;
    Ok(path)
}

fn next_clipboard_image_path(root: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = CLIPBOARD_IMAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    root.join(format!(
        "{CLIPBOARD_IMAGE_PREFIX}{timestamp}-{}-{sequence}.png",
        std::process::id()
    ))
}

fn cleanup_expired_images(root: &Path, keep: Option<&Path>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if keep.is_some_and(|keep| keep == path) {
            continue;
        }
        let is_managed_png = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(CLIPBOARD_IMAGE_PREFIX) && name.ends_with(".png"));
        if !is_managed_png {
            continue;
        }
        let expired = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > CLIPBOARD_IMAGE_RETENTION);
        if expired {
            let _ = fs::remove_file(path);
        }
    }
}

fn path_to_string(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agentmux-{name}-{}-{}",
            std::process::id(),
            CLIPBOARD_IMAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn saves_rgba_clipboard_images_as_png() {
        let root = test_root("clipboard-image");
        let path = save_clipboard_image(&root, 2, 1, &[255, 0, 0, 255, 0, 255, 0, 255])
            .expect("clipboard image should be saved");

        assert!(path.exists());
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("png")
        );
        let decoded = image::open(&path)
            .expect("saved PNG should decode")
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.as_raw(), &[255, 0, 0, 255, 0, 255, 0, 255]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_malformed_rgba_buffers() {
        let root = test_root("clipboard-invalid-image");
        let error = save_clipboard_image(&root, 2, 2, &[0; 4])
            .expect_err("invalid RGBA length must be rejected");

        assert!(error.contains("invalid"));
        assert!(!root.exists());
    }
}
